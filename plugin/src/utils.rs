//! Shared utility functions

use nvim_oxi::api::{self, opts::ExecOpts, Error as ApiError};
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use lazy_static::lazy_static;

pub static CURSOR_INDICATOR: char = '│';

#[macro_export]
macro_rules! ignore_dbus_no_interface_error {
    ($expr:expr) => {
        match $expr {
            Err(fcitx5_dbus::zbus::Error::MethodError(
                object_name,
                Some(message),
                _,
            )) if object_name == "org.freedesktop.DBus.Error.UnknownObject"
                && message.starts_with(
                    "Unknown object '/org/freedesktop/portal/inputcontext/",
                ) =>
            {
                let _ = nvim_oxi::api::echo(
                    vec![(
                        "Input context gone, maybe fcitx5 restarted.  Ignoring.",
                        Some("WarningMsg"),
                    )],
                    true,
                    &nvim_oxi::api::opts::EchoOpts::default(),
                );
            }
            Err(e) => {
                let msg = format!("{}, Ignoring unhandled dbus error: {e:#?}", e);
                let _ = nvim_oxi::api::echo(
                    vec![(msg.as_str(), Some("WarningMsg"))],
                    true,
                    &nvim_oxi::api::opts::EchoOpts::default(),
                );
            }
            _ => {}
        }
    };
}

/// Convert any error into a Neovim API error
pub fn as_api_error(e: impl std::error::Error) -> ApiError {
    ApiError::Other(e.to_string())
}

/// Delegate to the VimL function nvim_feedkeys() (:h nvim_feedkeys())
/// We use this instead of [`nvim_oxi::api::replace_termcodes`] with [`nvim_oxi::api::feedkeys`],
/// because <Esc>, <Left>, <Right> do not work properly with those (as of nvim-oxi v0.5.1).
pub fn do_feedkeys_noremap(keys: &str) -> nvim_oxi::Result<()> {
    let viml_lines = format!(
        r#"
        let key = nvim_replace_termcodes("{}", v:true, v:false, v:true)
        call nvim_feedkeys(key, 'n', v:false)
        "#,
        keys
    );
    api::exec2(&viml_lines, &ExecOpts::default())?;
    Ok(())
}

// Environment variable that, when set, enables lock logging and
// specifies the file path to append logs to.
const LOCK_LOG_ENV_VAR: &str = "FCITX5_UI_RS_LOCK_LOG_FILE";

lazy_static! {
    // Optional logfile guarded by a mutex so that concurrent writers
    // do not interleave lines. If the env var is not set or the file
    // cannot be opened, logging is silently disabled.
    static ref LOCK_LOG_FILE: Option<Mutex<File>> = {
        match std::env::var(LOCK_LOG_ENV_VAR) {
            Ok(path) if !path.is_empty() => {
                let file_result = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path);
                match file_result {
                    Ok(file) => Some(Mutex::new(file)),
                    Err(_) => None,
                }
            }
            _ => None,
        }
    };
}

fn current_timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Log a lock event with source location info.
/// This is called by the lock_logged! macro.
pub fn log_lock_event_with_location(file: &str, line: u32, col: u32, message: &str) {
    if let Some(file_mutex) = &*LOCK_LOG_FILE {
        if let Ok(mut file_handle) = file_mutex.lock() {
            let _ = writeln!(
                &mut *file_handle,
                "[{}][{:?}] {}:{}:{} {}",
                current_timestamp_millis(),
                std::thread::current().id(),
                file,
                line,
                col,
                message,
            );
        }
    }
}

/// Check if lock logging is enabled (for use in macros to short-circuit formatting).
pub fn is_lock_logging_enabled() -> bool {
    LOCK_LOG_FILE.is_some()
}

/// Macro to lock an Arc<Mutex<T>> with logging that includes the call site location.
///
/// Usage:
///   let guard = lock_logged!(my_arc_mutex, "MutexName");
///
/// This will log lines like:
///   [timestamp][ThreadId(1)] src/foo.rs:42:5 locking Arc<Mutex<MutexName>>: acquiring
///   [timestamp][ThreadId(1)] src/foo.rs:42:5 locking Arc<Mutex<MutexName>>: acquired
#[macro_export]
macro_rules! lock_logged {
    ($arc_mutex:expr, $name:expr) => {{
        use $crate::utils::{is_lock_logging_enabled, log_lock_event_with_location};
        if is_lock_logging_enabled() {
            log_lock_event_with_location(
                file!(),
                line!(),
                column!(),
                &format!("locking Arc<Mutex<{}>>: acquiring", $name),
            );
        }
        let result = $arc_mutex.lock();
        if is_lock_logging_enabled() {
            match &result {
                Ok(_) => {
                    log_lock_event_with_location(
                        file!(),
                        line!(),
                        column!(),
                        &format!("locking Arc<Mutex<{}>>: acquired", $name),
                    );
                }
                Err(_) => {
                    log_lock_event_with_location(
                        file!(),
                        line!(),
                        column!(),
                        &format!("locking Arc<Mutex<{}>>: poisoned", $name),
                    );
                }
            }
        }
        result.unwrap()
    }};
}
