use std::ffi::{OsStr, OsString};

use windows::core::{Error, PWSTR};
use windows::Win32::System::Services::{
    LPSERVICE_MAIN_FUNCTIONW, RegisterServiceCtrlHandlerW, SERVICE_STATUS, SERVICE_STATUS_HANDLE,
    SERVICE_TABLE_ENTRYW, SetServiceStatus, StartServiceCtrlDispatcherW,
};

use crate::windows_utils::WideString;


pub(crate) struct ServiceTableEntry {
    pub name: OsString,
    pub main_func: LPSERVICE_MAIN_FUNCTIONW,
}


pub(crate) fn start_service_dispatcher(service_table: &[ServiceTableEntry]) -> Result<(), Error> {
    let mut service_names: Vec<WideString> = service_table.iter()
        .map(|ste| WideString::from(&ste.name))
        .collect();

    let mut raw_entries: Vec<SERVICE_TABLE_ENTRYW> = Vec::with_capacity(service_table.len() + 1);
    for i in 0..service_table.len() {
        let entry = SERVICE_TABLE_ENTRYW {
            lpServiceName: service_names[i].as_pwstr(),
            lpServiceProc: service_table[i].main_func,
        };
        raw_entries.push(entry);
    }

    // sentinel entry at the end
    raw_entries.push(SERVICE_TABLE_ENTRYW {
        lpServiceName: PWSTR::default(),
        lpServiceProc: None,
    });

    let success = unsafe {
        StartServiceCtrlDispatcherW(raw_entries.as_mut_ptr())
    }.as_bool();

    // what happens now:
    // * a new thread is started for each service in service_table, wherein its main_func is called
    // * each service registers a control message handler function using RegisterServiceCtrlHandler
    // * this thread pumps messages from the service control manager, delivering service control
    //   messages by calling the corresponding control message handler functions (on this thread!)
    // * StartServiceCtrlDispatcherW only returns if an error occurs or all the services in its care
    //   have stopped
    //
    // => we should be using synchronization primitives to deliver the control messages from this
    //    thread to the service thread(s)

    if success {
        Ok(())
    } else {
        Err(Error::from_win32())
    }
}

pub(crate) fn register_service_control_handler(
    service_name: &OsStr,
    handler_function: Option<unsafe extern "system" fn(dwcontrol: u32)>,
) -> Result<ServiceStatusHandle, Error> {
    let service_name_ws = WideString::from(service_name);
    let handle = unsafe {
        RegisterServiceCtrlHandlerW(
            service_name_ws.as_pcwstr(),
            handler_function,
        )
    }?;
    Ok(ServiceStatusHandle(handle))
}


#[derive(Debug)]
pub(crate) struct ServiceStatusHandle(SERVICE_STATUS_HANDLE);
impl ServiceStatusHandle {
    pub fn set_status(
        &self,
        service_status: SERVICE_STATUS,
    ) -> Result<(), Error> {
        let success = unsafe {
            SetServiceStatus(self.0, &service_status)
        }.as_bool();

        if success {
            Ok(())
        } else {
            Err(Error::from_win32())
        }
    }
}
