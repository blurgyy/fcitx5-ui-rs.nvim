//! Shared utility functions

use nvim_oxi::api::Error as ApiError;

pub static CURSOR_INDICATOR: char = '│';

#[macro_export]
macro_rules! ignore_dbus_no_interface_error {
    ($expr:expr) => {
        match $expr {
            Err(fcitx5_dbus::zbus::Error::MethodError(
                object_name,
                Some(message),
                _,
            )) if object_name.to_string()
                == "org.freedesktop.DBus.Error.UnknownObject"
                && message.starts_with(
                    "Unknown object '/org/freedesktop/portal/inputcontext/",
                ) =>
            {
                oxi::print!(
                    "{}: Input context gone, maybe fcitx5 restarted.  Ignoring.",
                    crate::plugin::PLUGIN_NAME,
                );
            }
            Err(e) => {
                nvim_oxi::print!(
                    "{}, Ignoring unhandled dbus error: {e:#?}",
                    crate::plugin::PLUGIN_NAME,
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
