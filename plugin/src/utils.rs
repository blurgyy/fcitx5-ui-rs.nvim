//! Shared utility functions

use nvim_oxi::api::{self, opts::ExecOpts, Error as ApiError};

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
