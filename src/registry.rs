use std::ffi::{OsStr, OsString};
use std::mem::size_of;
use std::os::windows::prelude::{OsStrExt, OsStringExt};
use std::ptr::null_mut;

use bitflags::bitflags;
use windows::core::Error;
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, NO_ERROR};
use windows::Win32::Storage::FileSystem::READ_CONTROL;
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CLASSES_ROOT, HKEY_CURRENT_CONFIG, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, HKEY_USERS,
    KEY_CREATE_SUB_KEY, KEY_ENUMERATE_SUB_KEYS, KEY_QUERY_VALUE, KEY_NOTIFY, KEY_SET_VALUE,
    REG_BINARY, RegCloseKey, REG_DWORD, REG_DWORD_BIG_ENDIAN, REG_EXPAND_SZ,
    REG_FULL_RESOURCE_DESCRIPTOR, REG_LINK, REG_MULTI_SZ, REG_NONE, RegOpenKeyExW,
    REG_RESOURCE_LIST, REG_RESOURCE_REQUIREMENTS_LIST, RegQueryValueExW, REG_QWORD, REG_SAM_FLAGS,
    REG_SZ, REG_VALUE_TYPE,
};
use windows::Win32::System::SystemServices::{DELETE, WRITE_DAC, WRITE_OWNER};

use crate::windows_utils::{OptionalWideString, WideString};


bitflags! {
    pub struct RegistryPermissions: u32 {
        const QUERY_VALUE = KEY_QUERY_VALUE.0;
        const SET_VALUE = KEY_SET_VALUE.0;
        const CREATE_SUB_KEY = KEY_CREATE_SUB_KEY.0;
        const ENUMERATE_SUB_KEYS = KEY_ENUMERATE_SUB_KEYS.0;
        const NOTIFY = KEY_NOTIFY.0;
        const DELETE = DELETE;
        const READ_CONTROL = READ_CONTROL.0;
        const WRITE_DAC = WRITE_DAC;
        const WRITE_OWNER = WRITE_OWNER;
    }
}
impl From<RegistryPermissions> for REG_SAM_FLAGS {
    fn from(perms: RegistryPermissions) -> Self {
        REG_SAM_FLAGS(perms.bits())
    }
}


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RegistryValue {
    None(Vec<u8>),
    String(OsString),
    ExpandString { unexpanded: OsString, expanded: OsString },
    Binary(Vec<u8>),
    Dword(u32),
    DwordBigEndian(u32),
    Link(OsString),
    MultiString(Vec<OsString>),
    ResourceList(Vec<u8>),
    FullResourceDescriptor(Vec<u8>),
    ResourceRequirementsList(Vec<u8>),
    Qword(u64),
}
impl RegistryValue {
    pub fn to_reg_value_type(&self) -> REG_VALUE_TYPE {
        match self {
            Self::None(_) => REG_NONE,
            Self::String(_) => REG_SZ,
            Self::ExpandString { unexpanded: _, expanded: _ } => REG_EXPAND_SZ,
            Self::Binary(_) => REG_BINARY,
            Self::Dword(_) => REG_DWORD,
            Self::DwordBigEndian(_) => REG_DWORD_BIG_ENDIAN,
            Self::Link(_) => REG_LINK,
            Self::MultiString(_) => REG_MULTI_SZ,
            Self::ResourceList(_) => REG_RESOURCE_LIST,
            Self::FullResourceDescriptor(_) => REG_FULL_RESOURCE_DESCRIPTOR,
            Self::ResourceRequirementsList(_) => REG_RESOURCE_REQUIREMENTS_LIST,
            Self::Qword(_) => REG_QWORD,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Self::None(bs) => bs.clone(),
            Self::String(s) => os_str_to_bytes(s),
            Self::ExpandString { unexpanded, expanded: _ } => os_str_to_bytes(unexpanded),
            Self::Binary(bs) => bs.clone(),
            Self::Dword(dw) => Vec::from(dw.to_le_bytes()),
            Self::DwordBigEndian(dw) => Vec::from(dw.to_be_bytes()),
            Self::Link(s) => os_str_to_bytes(s),
            Self::MultiString(ss) => {
                let mut ws = Vec::new();
                for (i, s) in ss.iter().enumerate() {
                    let s_ws: Vec<u16> = s.encode_wide().collect();
                    if s_ws.contains(&0x00) {
                        panic!("string at index {} in a multi-string contains a NUL character", i);
                    }
                    if s_ws.len() == 0 {
                        panic!("string at index {} in a multi-string is empty", i);
                    }
                    ws.extend(&s_ws);
                    ws.push(0x0000);
                }
                ws.push(0x0000);

                let mut bs = Vec::with_capacity(ws.len() * size_of::<u16>());
                for w in ws {
                    bs.extend(w.to_ne_bytes());
                }
                bs
            },
            Self::ResourceList(bs) => bs.clone(),
            Self::FullResourceDescriptor(bs) => bs.clone(),
            Self::ResourceRequirementsList(bs) => bs.clone(),
            Self::Qword(qw) => Vec::from(qw.to_le_bytes()),
        }
    }

    pub fn decode_raw(reg_value_type: REG_VALUE_TYPE, bs: &[u8]) -> RegistryValue {
        match reg_value_type {
            REG_NONE => RegistryValue::None(Vec::from(bs)),
            REG_SZ => RegistryValue::String(bytes_to_os_string(bs)),
            REG_EXPAND_SZ => os_string_to_expand_value(bytes_to_os_string(bs)),
            REG_BINARY => RegistryValue::Binary(Vec::from(bs)),
            REG_DWORD => RegistryValue::Dword(u32::from_le_bytes(bs.try_into().expect("DWORD value has incorrect length"))),
            REG_DWORD_BIG_ENDIAN => RegistryValue::DwordBigEndian(u32::from_be_bytes(bs.try_into().expect("DWORD value has incorrect length"))),
            REG_LINK => RegistryValue::Link(bytes_to_os_string(bs)),
            REG_MULTI_SZ => RegistryValue::MultiString(bytes_to_multi_os_string(bs)),
            REG_RESOURCE_LIST => RegistryValue::ResourceList(Vec::from(bs)),
            REG_FULL_RESOURCE_DESCRIPTOR => RegistryValue::FullResourceDescriptor(Vec::from(bs)),
            REG_RESOURCE_REQUIREMENTS_LIST => Self::ResourceRequirementsList(Vec::from(bs)),
            REG_QWORD => Self::Qword(u64::from_le_bytes(bs.try_into().expect("QWORD value has incorrect length"))),
            _ => panic!("unknown registry value type 0x{:X}", reg_value_type.0),
        }
    }
}


#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PredefinedKey {
    ClassesRoot,
    CurrentConfig,
    CurrentUser,
    LocalMachine,
    Users,
}
impl From<PredefinedKey> for HKEY {
    fn from(pk: PredefinedKey) -> Self {
        match pk {
            PredefinedKey::ClassesRoot => HKEY_CLASSES_ROOT,
            PredefinedKey::CurrentConfig => HKEY_CURRENT_CONFIG,
            PredefinedKey::CurrentUser => HKEY_CURRENT_USER,
            PredefinedKey::LocalMachine => HKEY_LOCAL_MACHINE,
            PredefinedKey::Users => HKEY_USERS,
        }
    }
}

#[derive(Debug)]
pub struct RegistryKeyHandle(HKEY);
impl RegistryKeyHandle {
    fn open_relative(
        parent: HKEY,
        subkey: Option<&OsStr>,
        permissions: RegistryPermissions,
    ) -> Result<Self, Error> {
        let mut hkey = HKEY::default();
        let subkey_ws = OptionalWideString::from(subkey);

        let err_code = unsafe {
            RegOpenKeyExW(
                parent,
                subkey_ws.as_pcwstr(),
                0,
                permissions.into(),
                &mut hkey,
            )
        };
        if err_code == NO_ERROR {
            Ok(Self(hkey))
        } else {
            Err(err_code.into())
        }
    }

    pub fn open_predefined(
        predefined: PredefinedKey,
        subkey: Option<&OsStr>,
        permissions: RegistryPermissions,
    ) -> Result<Self, Error> {
        let parent_hkey = HKEY::from(predefined);
        Self::open_relative(parent_hkey, subkey, permissions)
    }

    pub fn open_subkey(
        &self,
        subkey: Option<&OsStr>,
        permissions: RegistryPermissions,
    ) -> Result<Self, Error> {
        Self::open_relative(self.0, subkey, permissions)
    }

    pub fn read_value(
        &self,
        value_name: Option<&OsStr>,
    ) -> Result<RegistryValue, Error> {
        let value_name_ws = OptionalWideString::from(value_name);

        // get buffer size
        let mut byte_count = 0u32;
        let size_status = unsafe {
            RegQueryValueExW(
                self.0,
                value_name_ws.as_pcwstr(),
                null_mut(),
                null_mut(),
                null_mut(),
                &mut byte_count,
            )
        };
        if size_status != NO_ERROR {
            return Err(size_status.into());
        }

        let byte_count_usize: usize = byte_count.try_into().unwrap();
        let mut buf = vec![0u8; byte_count_usize];
        let mut reg_value_type = REG_VALUE_TYPE::default();
        let status = unsafe {
            RegQueryValueExW(
                self.0,
                value_name_ws.as_pcwstr(),
                null_mut(),
                &mut reg_value_type,
                buf.as_mut_ptr(),
                &mut byte_count,
            )
        };
        if status != NO_ERROR {
            return Err(status.into());
        }

        Ok(RegistryValue::decode_raw(reg_value_type, &buf))
    }

    pub fn read_value_optional(
        &self,
        value_name: Option<&OsStr>,
    ) -> Result<Option<RegistryValue>, Error> {
        match self.read_value(value_name) {
            Ok(v) => Ok(Some(v)),
            Err(e) => {
                if e.win32_error().map(|we| we == ERROR_FILE_NOT_FOUND).unwrap_or(false) {
                    Ok(None)
                } else {
                    Err(e)
                }
            },
        }
    }
}
impl Drop for RegistryKeyHandle {
    fn drop(&mut self) {
        let err_code = unsafe {
            RegCloseKey(self.0)
        };
        if err_code != NO_ERROR {
            eprintln!("failed to close registry key: {}", Error::from(err_code));
        }
    }
}


fn os_str_to_bytes(os_str: &OsStr) -> Vec<u8> {
    let mut ws = Vec::new();
    ws.extend(os_str.encode_wide());
    ws.push(0x0000);

    let mut bs = Vec::with_capacity(ws.len() * size_of::<u16>());
    for w in &ws {
        let wbs = w.to_ne_bytes();
        bs.extend(wbs);
    }
    bs
}

fn bytes_to_os_string(bs: &[u8]) -> OsString {
    if bs.len() % 2 != 0 {
        panic!("bytes length not divisible by 2");
    }

    let mut ws = Vec::with_capacity(bs.len()/2);
    for i in 0..bs.len()/2 {
        let byte_array = [
            bs[2*i + 0],
            bs[2*i + 1],
        ];
        ws.push(u16::from_ne_bytes(byte_array));
    }

    // strip single trailing NUL
    if ws.last().map(|l| *l == 0x0000).unwrap_or(false) {
        ws.remove(ws.len() - 1);
    }

    OsString::from_wide(&ws)
}

fn bytes_to_multi_os_string(bs: &[u8]) -> Vec<OsString> {
    if bs.len() % 2 != 0 {
        panic!("bytes length not divisible by 2");
    }

    let mut ws = Vec::with_capacity(bs.len()/2);
    for i in 0..bs.len()/2 {
        let byte_array = [
            bs[2*i + 0],
            bs[2*i + 1],
        ];
        ws.push(u16::from_ne_bytes(byte_array));
    }

    let mut ss = Vec::with_capacity(ws.iter().filter(|w| **w == 0x0000).count());
    for slice in ws.split(|w| *w == 0x0000) {
        if slice.len() == 0 {
            break;
        }
        ss.push(OsString::from_wide(slice));
    }

    ss
}

fn os_string_to_expand_value(os_string: OsString) -> RegistryValue {
    if os_string.len() == 0 {
        let unexpanded = os_string.clone();
        let expanded = os_string;
        return RegistryValue::ExpandString {
            unexpanded,
            expanded,
        };
    }

    // check length
    let os_string_ws = WideString::from(&os_string);
    let mut empty_buf = [];
    let new_length = unsafe {
        ExpandEnvironmentStringsW(
            os_string_ws.as_pcwstr(),
            &mut empty_buf,
        )
    };
    if new_length == 0 {
        panic!("failed to get expanded environment string length: {}", Error::from_win32());
    }
    let new_length_usize: usize = new_length.try_into().unwrap();

    let mut buf = vec![0u16; new_length_usize];
    let expanded_length = unsafe {
        ExpandEnvironmentStringsW(
            os_string_ws.as_pcwstr(),
            buf.as_mut_slice(),
        )
    };
    if expanded_length == 0 {
        panic!("failed to get expanded environment string: {}", Error::from_win32());
    }
    let expanded_length_usize: usize = expanded_length.try_into().unwrap();

    let expanded = OsString::from_wide(&buf[0..expanded_length_usize]);
    RegistryValue::ExpandString {
        unexpanded: os_string,
        expanded,
    }
}
