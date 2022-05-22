use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};

use windows::core::{PCWSTR, PWSTR};


/// A NUL-terminated string consisting of u16 characters.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct WideString(Vec<u16>);
impl WideString {
    #[inline]
    pub fn len_chars(&self) -> usize { self.0.len() }

    #[inline]
    pub fn len_bytes(&self) -> usize { self.0.len() * std::mem::size_of::<u16>() }

    #[inline]
    pub fn as_pcwstr(&self) -> PCWSTR { self.into() }

    #[inline]
    pub fn as_pwstr(&mut self) -> PWSTR { self.into() }

    #[inline]
    pub fn to_os_string(&self) -> OsString { self.into() }

    pub fn from_pointer(s: *const u16, max_len: Option<isize>) -> Self {
        let mut ret = Vec::new();
        let mut i = 0;
        while max_len.is_none() || i < max_len.unwrap() {
            unsafe {
                let s_offset = s.offset(i);
                ret.push(*s_offset);
                if *s_offset == 0x00 {
                    break;
                }
            }
            i += 1;
        }
        Self(ret)
    }
}
impl From<&str> for WideString {
    fn from(s: &str) -> Self {
        let mut ret: Vec<u16> = s.encode_utf16().collect();
        if !ret.last().map(|l| *l == 0x0000).unwrap_or(false) {
            ret.push(0x0000);
        }
        Self(ret)
    }
}
impl From<&String> for WideString {
    fn from(s: &String) -> Self { s.as_str().into() }
}
impl From<&OsStr> for WideString {
    fn from(s: &OsStr) -> Self {
        let mut ret: Vec<u16> = s.encode_wide().collect();
        if !ret.last().map(|l| *l == 0x0000).unwrap_or(false) {
            ret.push(0x0000);
        }
        Self(ret)
    }
}
impl From<&OsString> for WideString {
    fn from(s: &OsString) -> Self { s.as_os_str().into() }
}
impl From<&WideString> for PCWSTR {
    fn from(s: &WideString) -> Self { PCWSTR(s.0.as_ptr()) }
}
impl From<&mut WideString> for PWSTR {
    fn from(s: &mut WideString) -> Self { PWSTR(s.0.as_mut_ptr()) }
}
impl From<&WideString> for OsString {
    fn from(s: &WideString) -> Self {
        OsString::from_wide(&s.0[0..s.0.len()-1])
    }
}
impl From<*const u16> for WideString {
    fn from(s: *const u16) -> Self { Self::from_pointer(s, None) }
}
impl From<*mut u16> for WideString {
    fn from(s: *mut u16) -> Self { (s as *const u16).into() }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct OptionalWideString(Option<WideString>);
impl OptionalWideString {
    #[inline]
    pub fn as_pcwstr(&self) -> PCWSTR { self.into() }

    #[inline]
    pub fn as_pwstr(&mut self) -> PWSTR { self.into() }

    #[inline]
    pub fn none() -> Self { Self(None) }

    #[inline]
    pub fn some(ws: WideString) -> Self { Self(Some(ws)) }
}
impl From<Option<&OsStr>> for OptionalWideString {
    fn from(s: Option<&OsStr>) -> Self {
        match s {
            Some(os) => Self(Some(os.into())),
            None => Self(None),
        }
    }
}
impl From<&OptionalWideString> for Option<OsString> {
    fn from(s: &OptionalWideString) -> Self {
        s.0.as_ref().map(|os| os.into())
    }
}
impl From<&OptionalWideString> for PCWSTR {
    fn from(s: &OptionalWideString) -> Self {
        match s.0.as_ref() {
            Some(ws) => ws.into(),
            None => PCWSTR::default(),
        }
    }
}
impl From<&mut OptionalWideString> for PWSTR {
    fn from(s: &mut OptionalWideString) -> Self {
        match s.0.as_mut() {
            Some(ws) => ws.into(),
            None => PWSTR::default(),
        }
    }
}
