use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Local;
use log::{Level, Log, Metadata, Record};
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;

use crate::log_panic;
use crate::registry::{PredefinedKey, RegistryKeyHandle, RegistryPermissions, RegistryValue};


pub(crate) struct StderrLogger {
    pub level: Level,
}
impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let now = Local::now();
            eprintln!("[{}] {:5} - {}", now.format("%Y-%m-%d %H:%M:%S%.3f %z"), record.level(), record.args());
        }
    }

    fn flush(&self) {}
}


pub(crate) struct WriterLogger<W: Send + Write> {
    pub level: Level,
    writer: Mutex<W>,
}
impl<W: Send + Write> WriterLogger<W> {
    pub fn new(level: Level, writer: W) -> Self {
        Self {
            level,
            writer: Mutex::new(writer),
        }
    }
}
impl<W: Send + Write> Log for WriterLogger<W> {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let now = Local::now();
            let mut writer_guard = self.writer.lock().expect("failed to lock file");
            let write_res = writeln!(writer_guard, "[{}] {:5} - {}", now.format("%Y-%m-%d %H:%M:%S%.3f %z"), record.level(), record.args());
            if let Err(e) = write_res {
                eprintln!("failed to write to log writer: {}", e);
            }
        }
    }

    fn flush(&self) {}
}


pub(crate) fn enable_stderr(level: Level) {
    let log_res = log::set_boxed_logger(Box::new(StderrLogger {
        level,
    }));
    if let Err(e) = log_res {
        eprintln!("failed to set logger: {}", e);
    }
}

pub(crate) fn enable_file(level: Level, path: &Path) {
    let file = File::options()
        .append(true)
        .open(path)
        .expect("failed to open log file");
    let log_res = log::set_boxed_logger(Box::new(WriterLogger::new(
        level,
        file,
    )));
    if let Err(e) = log_res {
        eprintln!("failed to set logger: {}", e);
    }
}

pub(crate) fn enable_file_from_registry(top_key: PredefinedKey, sub_key: &OsStr) {
    // open registry
    let registry_res = RegistryKeyHandle::open_predefined(
        top_key,
        Some(sub_key),
        RegistryPermissions::QUERY_VALUE,
    );
    let registry = match registry_res {
        Ok(r) => r,
        Err(e) => {
            if e.win32_error().map(|w| w == ERROR_FILE_NOT_FOUND).unwrap_or(false) {
                // registry key does not exist
                return;
            }
            log_panic!("failed to open logging registry key: {}", e);
        },
    };

    // read the path
    let path_res = registry.read_value_optional(Some(&OsString::from("LogPath")));
    let path_val = match path_res {
        Ok(Some(p)) => p,
        Ok(None) => {
            // registry value does not exist
            return;
        },
        Err(e) => log_panic!("failed to read LogPath value: {}", e),
    };
    let path = match path_val {
        RegistryValue::String(s) => s,
        RegistryValue::ExpandString { unexpanded: _, expanded: s } => s,
        other => log_panic!("LogPath has unexpected type: {:?}", other),
    };

    // read the log level
    let level_res = registry.read_value_optional(Some(&OsString::from("LogLevel")));
    let level_val = match level_res {
        Ok(Some(l)) => l,
        Ok(None) => {
            // registry value does not exist; use a default (Error)
            RegistryValue::Dword((Level::Error as usize) as u32)
        },
        Err(e) => log_panic!("failed to read LogLevel value: {}", e),
    };
    let level_int = match level_val {
        RegistryValue::Dword(d) => d.into(),
        RegistryValue::DwordBigEndian(d) => d.into(),
        RegistryValue::Qword(d) => d,
        other => log_panic!("LogLevel has unexpected type: {:?}", other),
    };
    let int_to_level: BTreeMap<usize, Level> = Level::iter()
        .map(|l| (l as usize, l))
        .collect();
    let level = if let Some(l) = int_to_level.get(&usize::try_from(level_int).unwrap()) {
        *l
    } else {
        // pick the highest level
        let max_level = int_to_level.keys().max().unwrap();
        *int_to_level.get(max_level).unwrap()
    };

    // set it up
    enable_file(level, &PathBuf::from(path))
}
