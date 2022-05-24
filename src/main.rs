mod args;
mod extensions;
mod logging;
mod registry;
mod service_control;
mod service_running;
mod wait_stopper;
mod windows_utils;


use std::ffi::{OsStr, OsString};
use std::time::Duration;

use log::Level;
use once_cell::sync::OnceCell;
use windows::core::PWSTR;
use windows::Win32::Foundation::NO_ERROR;
use windows::Win32::System::Services::{
    SERVICE_ACCEPT_STOP, SERVICE_CONTROL_INTERROGATE, SERVICE_CONTROL_STOP, SERVICE_RUNNING,
    SERVICE_STATUS, SERVICE_STOPPED, SERVICE_WIN32_OWN_PROCESS,
};

use crate::args::{Args, OperMode};
use crate::extensions::ExpectExtension;
use crate::registry::{PredefinedKey, RegistryKeyHandle, RegistryPermissions, RegistryValue};
use crate::service_control::{
    ServiceControlManagerHandle, ServiceControlManagerPermissions, ServiceErrorControl,
    ServicePermissions, ServiceStartType, ServiceState, ServiceType,
};
use crate::service_running::{
    register_service_control_handler, ServiceStatusHandle, ServiceTableEntry,
    start_service_dispatcher,
};
use crate::wait_stopper::WaitStopper;
use crate::windows_utils::WideString;


struct ServiceInfo {
    pub wait_stopper: WaitStopper,
    pub service_status_handle: ServiceStatusHandle,
}


static SERVICE_INFO: OnceCell<Option<ServiceInfo>> = OnceCell::new();


fn get_my_registry_path(service_name: &OsStr) -> OsString {
    let mut mrp = OsString::new();
    mrp.push("SYSTEM\\CurrentControlSet\\Services\\");
    mrp.push(&service_name);
    mrp.push("\\Parameters");
    mrp
}


fn run(service_name: OsString) {
    let my_registry_path = get_my_registry_path(&service_name);

    let mut is_first_loop: bool = true;
    loop {
        // check our settings in the registry
        let registry_res = RegistryKeyHandle::open_predefined(
            PredefinedKey::LocalMachine,
            Some(&my_registry_path),
            RegistryPermissions::QUERY_VALUE,
        );
        let registry = match registry_res {
            Ok(r) => r,
            Err(e) => log_panic!("failed to open my registry path (HKLM subkey {:?}): {}", my_registry_path, e),
        };

        if is_first_loop {
            is_first_loop = false;

            // query initial sleep duration
            let initial_sleep_duration_ms_value = registry.read_value_optional(Some(&OsString::from("InitialSleepDurationMilliseconds")))
                .expect_log("failed to read service parameter InitialSleepDurationMilliseconds");
            if let Some(isdv) = initial_sleep_duration_ms_value {
                let milliseconds: u64 = match isdv {
                    RegistryValue::Dword(dw) => dw.into(),
                    RegistryValue::DwordBigEndian(dw) => dw.into(),
                    RegistryValue::Qword(qw) => qw,
                    other => log_panic!("unexpected service parameter InitialSleepDurationMilliseconds value {:?}", other),
                };

                // sleep
                let wait_stopper = SERVICE_INFO
                    .get().expect_log("SERVICE_INFO not set")
                    .as_ref().map(|si| &si.wait_stopper);
                let stop_result = WaitStopper::wait_until_stop_timeout_opt(wait_stopper, Duration::from_millis(milliseconds));
                if stop_result.wants_to_stop() {
                    // get out
                    return;
                }
            }
        }

        // query services that need to be running
        let run_services = registry.read_value(Some(&OsString::from("ServicesExpectedRunning")))
            .expect_log("failed to read service parameter ServicesExpectedRunning");
        if let RegistryValue::MultiString(names) = run_services {
            // connect to service control manager
            let scm = ServiceControlManagerHandle::open_local_active(
                ServiceControlManagerPermissions::CONNECT,
            )
                .expect_log("failed to connect to service control manager");

            for name in &names {
                // open the service
                let service_res = scm.open_service(
                    name,
                    ServicePermissions::QUERY_STATUS | ServicePermissions::STOP,
                );
                let service = match service_res {
                    Ok(s) => s,
                    Err(e) => {
                        log_panic!("failed to open service {:?}: {}", name, e);
                    },
                };

                // query its state
                let service_state = match service.get_state() {
                    Ok(ss) => ss,
                    Err(e) => {
                        log_panic!("failed to get service {:?} state: {}", name, e);
                    },
                };

                if service_state == ServiceState::Stopped {
                    // start it
                    if let Err(e) = service.start(vec![]) {
                        log_panic!("failed to start service {:?}: {}", name, e);
                    }
                }
            }
        } else {
            log_panic!("unexpected service parameter ServicesExpectedRunning value {:?}", run_services);
        }

        // query regular sleep duration
        let sleep_duration_ms_value = registry.read_value(Some(&OsString::from("SleepDurationMilliseconds")))
            .expect_log("failed to read service parameter SleepDurationMilliseconds");
        let milliseconds: u64 = match sleep_duration_ms_value {
            RegistryValue::Dword(dw) => dw.into(),
            RegistryValue::DwordBigEndian(dw) => dw.into(),
            RegistryValue::Qword(qw) => qw,
            other => log_panic!("unexpected service parameter SleepDurationMilliseconds value {:?}", other),
        };

        // sleep
        let wait_stopper = SERVICE_INFO
            .get().expect_log("SERVICE_INFO not set")
            .as_ref().map(|si| &si.wait_stopper);
        let stop_result = WaitStopper::wait_until_stop_timeout_opt(wait_stopper, Duration::from_millis(milliseconds));
        if stop_result.wants_to_stop() {
            // get out
            return;
        }
    }
}

extern "system" fn service_control(control_value: u32) {
    match control_value {
        SERVICE_CONTROL_INTERROGATE => {
            // do nothing
            return;
        },
        SERVICE_CONTROL_STOP => {
            // signal stop
            SERVICE_INFO
                .get().expect_log("SERVICE_INFO not set")
                .as_ref().expect_log("SERVICE_INFO empty")
                .wait_stopper.stop();
        },
        _ => {},
    }
}

extern "system" fn run_service(num_args: u32, args: *mut PWSTR) {
    if num_args < 1 {
        log_panic!("no arguments passed to run_service!");
    }

    let service_name_pwstr = unsafe { *args };
    let service_name_ws = WideString::from(service_name_pwstr.0);
    let service_name = service_name_ws.to_os_string();

    // register our signalling procedure with the event pumping thread
    let service_status_handle = register_service_control_handler(&service_name, Some(service_control))
        .expect_log("failed to register service control handler");

    let service_info = ServiceInfo {
        wait_stopper: WaitStopper::new(),
        service_status_handle,
    };

    // don't care either way
    match SERVICE_INFO.set(Some(service_info)) {
        Ok(_) => {},
        Err(_) => {},
    }

    // announce that we are running
    let service_status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: SERVICE_RUNNING,
        dwControlsAccepted: SERVICE_ACCEPT_STOP,
        dwWin32ExitCode: NO_ERROR.0,
        dwServiceSpecificExitCode: NO_ERROR.0,
        dwCheckPoint: 0,
        dwWaitHint: 0,
    };
    SERVICE_INFO
        .get().expect_log("SERVICE_INFO not set?!")
        .as_ref().expect_log("SERVICE_INFO empty?!")
        .service_status_handle
        .set_status(service_status).expect_log("failed to set service status");

    run(service_name);

    // announce that we are stopped
    let service_status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: SERVICE_STOPPED,
        dwControlsAccepted: 0,
        dwWin32ExitCode: NO_ERROR.0,
        dwServiceSpecificExitCode: NO_ERROR.0,
        dwCheckPoint: 0,
        dwWaitHint: 0,
    };
    SERVICE_INFO
        .get().expect_log("SERVICE_INFO not set?!")
        .as_ref().expect_log("SERVICE_INFO empty?!")
        .service_status_handle
        .set_status(service_status).expect_log("failed to set service status");
}


fn main() {
    let arguments = Args::parse();

    match arguments.mode {
        OperMode::Run => {
            // run in foreground
            crate::logging::enable_stderr(Level::Info);

            match SERVICE_INFO.set(None) {
                Ok(_) => {},
                Err(_) => {},
            }

            run(arguments.service_name);
        },
        OperMode::Service => {
            // run as service
            let my_registry_path = get_my_registry_path(&arguments.service_name);
            crate::logging::enable_file_from_registry(PredefinedKey::LocalMachine, &my_registry_path);

            let service_table = [
                ServiceTableEntry {
                    name: arguments.service_name.clone(),
                    main_func: Some(run_service),
                },
            ];
            start_service_dispatcher(&service_table)
                .expect_log("failed to start service dispatcher");
        },
        OperMode::Start => {
            // start service
            crate::logging::enable_stderr(Level::Info);

            // open connection to SCM
            let scm_conn = ServiceControlManagerHandle::open_local_active(
                ServiceControlManagerPermissions::CONNECT,
            )
                .expect_log("failed to connect to service control manager");

            // open service
            let service = scm_conn.open_service(
                &arguments.service_name,
                ServicePermissions::START,
            )
                .expect_log("failed to open service");

            // start service
            service.start(vec![&arguments.service_name])
                .expect_log("failed to start service");
        },
        OperMode::Stop => {
            // stop service
            crate::logging::enable_stderr(Level::Info);

            // open connection to SCM
            let scm_conn = ServiceControlManagerHandle::open_local_active(
                ServiceControlManagerPermissions::CONNECT,
            )
                .expect_log("failed to connect to service control manager");

            // open service
            let service = scm_conn.open_service(
                &arguments.service_name,
                ServicePermissions::STOP,
            )
                .expect_log("failed to open service");

            // stop service
            service.stop()
                .expect_log("failed to stop service");
        },
        OperMode::Install => {
            // install service
            crate::logging::enable_stderr(Level::Info);

            let my_path = std::env::current_exe()
                .expect_log("failed to obtain executable path");
            let my_path_os = my_path.as_os_str();
            let mut my_path_quoted_os = if my_path_os.to_string_lossy().contains(' ') {
                let mut pqos = OsString::with_capacity(my_path_os.len() + 2);
                pqos.push("\"");
                pqos.push(my_path_os);
                pqos.push("\"");
                pqos
            } else {
                my_path_os.to_os_string()
            };
            my_path_quoted_os.push(" service ");
            my_path_quoted_os.push(&arguments.service_name);

            // open connection to SCM
            let scm_perms =
                ServiceControlManagerPermissions::CONNECT
                | ServiceControlManagerPermissions::CREATE_SERVICE
            ;
            let scm_conn = ServiceControlManagerHandle::open_local_active(scm_perms)
                .expect_log("failed to connect to service control manager");

            // create service
            scm_conn.create_service(
                &arguments.service_name,
                None,
                ServicePermissions::empty(),
                ServiceType::WIN32_OWN_PROCESS,
                ServiceStartType::Demand,
                ServiceErrorControl::Normal,
                &my_path_quoted_os,
                None,
                Vec::new(),
                None,
                None,
            )
                .expect_log("failed to create service");
        },
        OperMode::Delete => {
            // delete service after stopping it if necessary
            crate::logging::enable_stderr(Level::Info);

            // open connection to SCM
            let scm_conn = ServiceControlManagerHandle::open_local_active(
                ServiceControlManagerPermissions::CONNECT,
            )
                .expect_log("failed to connect to service control manager");

            // open service
            let service = scm_conn.open_service(
                &arguments.service_name,
                ServicePermissions::QUERY_STATUS | ServicePermissions::STOP | ServicePermissions::DELETE,
            )
                .expect_log("failed to open service");

            // check if the service is stopped
            let service_state = service.get_state()
                .expect_log("failed to obtain service state");
            if service_state != ServiceState::Stopped {
                // stop the service
                service.stop()
                    .expect_log("failed to stop service");
            }

            // remove the service
            service.delete()
                .expect_log("failed to delete service");
        },
    }
}
