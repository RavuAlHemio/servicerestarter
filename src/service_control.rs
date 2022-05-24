use std::ffi::{OsStr, OsString};
use std::hash::{Hash, Hasher};
use std::ptr::null_mut;

use bitflags::bitflags;
use from_to_repr::FromToRepr;
use windows::core::{Error, PCWSTR, PWSTR};
use windows::Win32::Security::SC_HANDLE;
use windows::Win32::Storage::FileSystem::READ_CONTROL;
use windows::Win32::System::Services::{
    CloseServiceHandle, ControlService, CreateServiceW, DeleteService, ENUM_SERVICE_TYPE,
    OpenSCManagerW, OpenServiceW, QueryServiceStatus, SC_MANAGER_CONNECT, SC_MANAGER_CREATE_SERVICE,
    SC_MANAGER_ENUMERATE_SERVICE, SC_MANAGER_LOCK, SC_MANAGER_MODIFY_BOOT_CONFIG,
    SC_MANAGER_QUERY_LOCK_STATUS, SERVICE_ADAPTER, SERVICE_AUTO_START, SERVICE_BOOT_START,
    SERVICE_CHANGE_CONFIG, SERVICE_CONTINUE_PENDING, SERVICE_CONTROL_STOP, SERVICE_DEMAND_START,
    SERVICE_DISABLED, SERVICE_ENUMERATE_DEPENDENTS, SERVICE_ERROR_CRITICAL, SERVICE_ERROR_IGNORE,
    SERVICE_ERROR_NORMAL, SERVICE_ERROR_SEVERE, SERVICE_ERROR, SERVICE_FILE_SYSTEM_DRIVER,
    SERVICE_INTERROGATE, SERVICE_KERNEL_DRIVER, SERVICE_PAUSE_CONTINUE, SERVICE_PAUSE_PENDING,
    SERVICE_PAUSED, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_RECOGNIZER_DRIVER,
    SERVICE_RUNNING, SERVICE_START_PENDING, SERVICE_START_TYPE, SERVICE_START, SERVICE_STATUS,
    SERVICE_STATUS_CURRENT_STATE, SERVICE_STOP_PENDING, SERVICE_STOP, SERVICE_STOPPED,
    SERVICE_SYSTEM_START, SERVICE_USER_DEFINED_CONTROL, SERVICE_WIN32_OWN_PROCESS,
    SERVICE_WIN32_SHARE_PROCESS, SERVICES_ACTIVE_DATABASEW, StartServiceW,
};
use windows::Win32::System::SystemServices::{
    DELETE, SERVICE_INTERACTIVE_PROCESS, WRITE_DAC, WRITE_OWNER,
};

use crate::extensions::ExpectExtension;
use crate::windows_utils::{OptionalWideString, WideString};


#[derive(Debug, Eq, PartialEq)]
#[repr(transparent)]
pub(crate) struct ServiceControlManagerHandle(SC_HANDLE);
impl ServiceControlManagerHandle {
    pub(crate) fn open_local_active(desired_access: ServiceControlManagerPermissions) -> Result<Self, Error> {
        // open SCM
        let services_active_database = WideString::from(SERVICES_ACTIVE_DATABASEW);
        let sc_handle = unsafe {
            OpenSCManagerW(
                PCWSTR::default(),
                PCWSTR::from(&services_active_database),
                desired_access.bits(),
            )
        }?;
        Ok(Self(sc_handle))
    }

    pub(crate) fn create_service(
        &self,
        service_name: &OsStr,
        display_name: Option<&OsStr>,
        desired_access: ServicePermissions,
        service_type: ServiceType,
        start_type: ServiceStartType,
        error_control: ServiceErrorControl,
        path_and_args: &OsStr,
        load_order_group: Option<&OsStr>,
        dependencies: Vec<&OsStr>,
        start_name: Option<&OsStr>,
        password: Option<&OsStr>,
    ) -> Result<ServiceHandle, Error> {
        let service_name_ws = WideString::from(service_name);
        let display_name_ws = OptionalWideString::from(display_name);
        let path_and_args_ws = WideString::from(path_and_args);
        let load_order_group_ws = OptionalWideString::from(load_order_group);
        let start_name_ws = OptionalWideString::from(start_name);
        let password_ws = OptionalWideString::from(password);

        let mut deps_os_str = OsString::new();
        for dep in dependencies {
            deps_os_str.push(dep);
            deps_os_str.push("\0");
        }
        deps_os_str.push("\0");
        let deps_ws = WideString::from(&deps_os_str);

        let service_handle = unsafe {
            CreateServiceW(
                self.0,
                service_name_ws.as_pcwstr(),
                display_name_ws.as_pcwstr(),
                desired_access.bits(),
                service_type.into(),
                start_type.into(),
                error_control.into(),
                path_and_args_ws.as_pcwstr(),
                load_order_group_ws.as_pcwstr(),
                null_mut(),
                deps_ws.as_pcwstr(),
                start_name_ws.as_pcwstr(),
                password_ws.as_pcwstr(),
            )
        }?;
        Ok(ServiceHandle(service_handle))
    }

    pub(crate) fn open_service(
        &self,
        service_name: &OsStr,
        desired_access: ServicePermissions,
    ) -> Result<ServiceHandle, Error> {
        let service_name_ws = WideString::from(service_name);

        let service_handle = unsafe {
            OpenServiceW(
                self.0,
                service_name_ws.as_pcwstr(),
                desired_access.bits(),
            )
        }?;
        Ok(ServiceHandle(service_handle))
    }
}
impl Drop for ServiceControlManagerHandle {
    fn drop(&mut self) {
        // return the handle
        let handle_closed = unsafe { CloseServiceHandle(self.0) }.as_bool();
        if !handle_closed {
            eprintln!("failed to close service control manager handle: {}", std::io::Error::last_os_error());
        }
    }
}
impl Hash for ServiceControlManagerHandle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.0.hash(state);
    }
}


#[derive(Debug, Eq, PartialEq)]
#[repr(transparent)]
pub(crate) struct ServiceHandle(SC_HANDLE);
impl ServiceHandle {
    pub fn start(
        &self,
        args: Vec<&OsStr>,
    ) -> Result<(), Error> {
        let mut args_ws: Vec<OptionalWideString> = Vec::with_capacity(args.len() + 1);
        for arg in args {
            args_ws.push(OptionalWideString::some(arg.into()));
        }

        let args_ptrs: Vec<PWSTR> = args_ws.iter_mut()
            .map(|a| a.as_pwstr())
            .collect();

        let succeeded = unsafe {
            StartServiceW(
                self.0,
                args_ptrs.as_slice(),
            )
        }.as_bool();
        if succeeded {
            Ok(())
        } else {
            Err(Error::from_win32())
        }
    }

    pub fn stop(&self) -> Result<(), Error> {
        let mut service_status = SERVICE_STATUS::default();

        let succeeded = unsafe {
            ControlService(
                self.0,
                SERVICE_CONTROL_STOP,
                &mut service_status,
            )
        }.as_bool();
        if succeeded {
            Ok(())
        } else {
            Err(Error::from_win32())
        }
    }

    pub fn get_state(&self) -> Result<ServiceState, Error> {
        let mut service_status = SERVICE_STATUS::default();

        let succeeded = unsafe {
            QueryServiceStatus(
                self.0,
                &mut service_status,
            )
        }.as_bool();
        if succeeded {
            let service_status = service_status.dwCurrentState
                .try_into().expect_log("unexpected service status value");
            Ok(service_status)
        } else {
            Err(Error::from_win32())
        }
    }

    pub fn delete(&self) -> Result<(), Error> {
        let succeeded = unsafe { DeleteService(self.0) }.as_bool();
        if succeeded {
            Ok(())
        } else {
            Err(Error::from_win32())
        }
    }
}
impl Drop for ServiceHandle {
    fn drop(&mut self) {
        // return the handle
        let handle_closed = unsafe { CloseServiceHandle(self.0) }.as_bool();
        if !handle_closed {
            eprintln!("failed to close service handle: {}", std::io::Error::last_os_error());
        }
    }
}
impl Hash for ServiceHandle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.0.hash(state);
    }
}


bitflags! {
    pub(crate) struct ServiceControlManagerPermissions: u32 {
        const CONNECT = SC_MANAGER_CONNECT;
        const CREATE_SERVICE = SC_MANAGER_CREATE_SERVICE;
        const ENUMERATE_SERVICE = SC_MANAGER_ENUMERATE_SERVICE;
        const LOCK = SC_MANAGER_LOCK;
        const QUERY_LOCK_STATUS = SC_MANAGER_QUERY_LOCK_STATUS;
        const MODIFY_BOOT_CONFIG = SC_MANAGER_MODIFY_BOOT_CONFIG;
    }

    pub(crate) struct ServicePermissions: u32 {
        const QUERY_CONFIG = SERVICE_QUERY_CONFIG;
        const CHANGE_CONFIG = SERVICE_CHANGE_CONFIG;
        const QUERY_STATUS = SERVICE_QUERY_STATUS;
        const ENUMERATE_DEPENDENTS = SERVICE_ENUMERATE_DEPENDENTS;
        const START = SERVICE_START;
        const STOP = SERVICE_STOP;
        const PAUSE_CONTINUE = SERVICE_PAUSE_CONTINUE;
        const INTERROGATE = SERVICE_INTERROGATE;
        const USER_DEFINED_CONTROL = SERVICE_USER_DEFINED_CONTROL;
        const DELETE = DELETE;
        const READ_CONTROL = READ_CONTROL.0;
        const WRITE_DAC = WRITE_DAC;
        const WRITE_OWNER = WRITE_OWNER;
    }

    pub(crate) struct ServiceType: u32 {
        const KERNEL_DRIVER = SERVICE_KERNEL_DRIVER.0;
        const FILE_SYSTEM_DRIVER = SERVICE_FILE_SYSTEM_DRIVER.0;
        const ADAPTER = SERVICE_ADAPTER.0;
        const RECOGNIZER_DRIVER = SERVICE_RECOGNIZER_DRIVER.0;
        const WIN32_OWN_PROCESS = SERVICE_WIN32_OWN_PROCESS.0;
        const WIN32_SHARE_PROCESS = SERVICE_WIN32_SHARE_PROCESS.0;
        const INTERACTIVE_PROCESS = SERVICE_INTERACTIVE_PROCESS;
    }
}
impl From<ServiceType> for ENUM_SERVICE_TYPE {
    fn from(st: ServiceType) -> Self { Self(st.bits()) }
}

#[derive(Clone, Copy, Debug, Eq, FromToRepr, Hash, PartialEq)]
#[repr(u32)]
pub(crate) enum ServiceStartType {
    Boot = SERVICE_BOOT_START.0,
    System = SERVICE_SYSTEM_START.0,
    Auto = SERVICE_AUTO_START.0,
    Demand = SERVICE_DEMAND_START.0, // = manual
    Disabled = SERVICE_DISABLED.0,
}
impl From<ServiceStartType> for SERVICE_START_TYPE {
    fn from(t: ServiceStartType) -> Self {
        SERVICE_START_TYPE(t.into())
    }
}

#[derive(Clone, Copy, Debug, Eq, FromToRepr, Hash, PartialEq)]
#[repr(u32)]
pub(crate) enum ServiceErrorControl {
    Ignore = SERVICE_ERROR_IGNORE.0,
    Normal = SERVICE_ERROR_NORMAL.0,
    Severe = SERVICE_ERROR_SEVERE.0,
    Critical = SERVICE_ERROR_CRITICAL.0,
}
impl From<ServiceErrorControl> for SERVICE_ERROR {
    fn from(t: ServiceErrorControl) -> Self {
        SERVICE_ERROR(t.into())
    }
}

#[derive(Clone, Copy, Debug, Eq, FromToRepr, Hash, PartialEq)]
#[repr(u32)]
pub(crate) enum ServiceState {
    Stopped = SERVICE_STOPPED.0,
    StartPending = SERVICE_START_PENDING.0,
    StopPending = SERVICE_STOP_PENDING.0,
    Running = SERVICE_RUNNING.0,
    ContinuePending = SERVICE_CONTINUE_PENDING.0,
    PausePending = SERVICE_PAUSE_PENDING.0,
    Paused = SERVICE_PAUSED.0,
}
impl TryFrom<SERVICE_STATUS_CURRENT_STATE> for ServiceState {
    type Error = SERVICE_STATUS_CURRENT_STATE;

    fn try_from(value: SERVICE_STATUS_CURRENT_STATE) -> Result<Self, Self::Error> {
        ServiceState::try_from(value.0)
            .map_err(|_| value)
    }
}
