use std::ffi::{OsStr, OsString};


fn exit_with_usage() -> ! {
    let myself = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "servicerestarter".to_owned());

    eprintln!("Usage: {} [MODE [SERVICENAME]]", myself);
    eprintln!();
    eprintln!("MODE is one of:");
    eprintln!();
    eprintln!("  run        Runs this application as a console application. This is the default");
    eprintln!("             if no mode is given.");
    eprintln!();
    eprintln!("  service    Runs this application as a service. This option only makes sense");
    eprintln!("             when passed by the operating system's service control manager.");
    eprintln!();
    eprintln!("  start      Starts the service corresponding to this application.");
    eprintln!();
    eprintln!("  stop       Stops the service corresponding to this application.");
    eprintln!();
    eprintln!("  install    Installs this application as a service into the operating system.");
    eprintln!();
    eprintln!("  delete     Removes this application's corresponding service from the operating");
    eprintln!("             system. If the service is running, it is stopped first.");
    eprintln!();
    eprintln!("SERVICENAME is used as the service name when operating the service as well as");
    eprintln!("reading the configuration from the registry. If it is missing, the name of the");
    eprintln!("executable binary (without the file extension) is used as the service name.");

    std::process::exit(1);
}


/// The arguments to the program.
pub(crate) struct Args {
    pub mode: OperMode,
    pub service_name: OsString,
}
impl Args {
    pub fn parse_args(args: impl Iterator<Item = impl Into<OsString>>) -> Args {
        let arg_vec: Vec<OsString> = args
            .map(|a| a.into())
            .collect();

        if arg_vec.len() > 3 {
            eprintln!("too many arguments");
            exit_with_usage();
        }

        let mode: OperMode = if arg_vec.len() < 2 {
            OperMode::default()
        } else {
            match arg_vec[1].as_os_str().try_into() {
                Ok(om) => om,
                Err(_) => {
                    eprintln!("unknown mode {:?}", arg_vec[1]);
                    exit_with_usage();
                },
            }
        };

        let service_name: OsString = if arg_vec.len() < 3 {
            // take from .exe name
            let exe_path = match std::env::current_exe() {
                Ok(pb) => pb,
                Err(e) => {
                    eprintln!("no service name given and failed to get executable path: {:?}", e);
                    exit_with_usage();
                },
            };
            match exe_path.file_stem() {
                Some(fs) => fs.to_os_string(),
                None => {
                    eprintln!("no service name given and executable path does not contain file name");
                    exit_with_usage();
                },
            }
        } else {
            arg_vec[2].clone()
        };

        Args {
            mode,
            service_name,
        }
    }

    pub fn parse() -> Args {
        Self::parse_args(std::env::args_os())
    }
}


/// The chosen mode of operation.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum OperMode {
    /// Run as a standard console application.
    Run,

    /// Run as a service. Triggered only by the operating system Service Control Manager.
    Service,

    /// Start the service.
    Start,

    /// Stop the service.
    Stop,

    /// Install the service.
    Install,

    /// Delete the service. Stop it first if it is running.
    Delete,
}
impl Default for OperMode {
    fn default() -> Self { Self::Run }
}
impl TryFrom<&OsStr> for OperMode {
    type Error = ();

    fn try_from(value: &OsStr) -> Result<Self, Self::Error> {
        if value == "run" {
            Ok(Self::Run)
        } else if value == "service" {
            Ok(Self::Service)
        } else if value == "start" {
            Ok(Self::Start)
        } else if value == "stop" {
            Ok(Self::Stop)
        } else if value == "install" {
            Ok(Self::Install)
        } else if value == "delete" {
            Ok(Self::Delete)
        } else {
            Err(())
        }
    }
}
