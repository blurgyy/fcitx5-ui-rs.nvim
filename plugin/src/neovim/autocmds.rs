//! Autocommand setup for Neovim

use nvim_oxi::{
    self as oxi,
    api::{
        self,
        opts::{CreateAugroupOpts, CreateAutocmdOpts},
        Buffer,
    },
    libuv::AsyncHandle,
    Error as OxiError,
};

use crate::plugin::{get_state, Fcitx5Plugin};
use crate::utils::as_api_error;
use crate::{ignore_dbus_no_interface_error, plugin::get_candidate_state};
use std::sync::{Arc, Mutex};

/// Setup autocommands for input method switching
pub fn register_autocommands(
    state: Arc<Mutex<Fcitx5Plugin>>,
    trigger: AsyncHandle,
    buf: &Buffer,
) -> oxi::Result<()> {
    let mut state_guard = state.lock().unwrap();

    // If already registered, clean up first
    if let Some(augroup_id) = state_guard.augroup_id.get(&buf.handle()) {
        api::del_augroup_by_id(*augroup_id)?;
    }

    // Create augroup for our autocommands
    let augroup_id = api::create_augroup(
        &format!("fcitx5-ui-rs-nvim-buf#{}", buf.handle()),
        &CreateAugroupOpts::builder().clear(true).build(),
    )?;
    state_guard.augroup_id.insert(buf.handle(), augroup_id);

    // Ensure we have controller and ctx
    let ctx = state_guard
        .ctx
        .get(&buf.handle())
        .expect("Input context not initialized"); // FIXME: we probably do not want to panic here

    let opts = CreateAutocmdOpts::builder()
        .group(augroup_id)
        .desc("Switch to Pinyin input method when entering insert mode")
        .callback({
            let state_ref = state.clone();
            let buf = buf.clone();
            move |_| {
                let insertmode = api::get_vvar::<String>("insertmode")?;
                if insertmode != "i" {
                    return Ok(false);
                }

                let state_guard = state_ref.lock().unwrap();
                if !state_guard.initialized(&buf) {
                    return Ok(false);
                }
                ignore_dbus_no_interface_error!(state_guard.activate_im(&buf));
                Ok::<_, OxiError>(false) // NB: return false to keep this autocmd
            }
        })
        .build();
    api::create_autocmd(["InsertEnter"], &opts)?;

    let opts = CreateAutocmdOpts::builder()
        .group(augroup_id)
        .desc("Switch to English input method when leaving insert mode")
        .callback({
            let state_ref = state.clone();
            let buf = buf.clone();
            move |_| {
                let state_guard = state_ref.lock().unwrap();
                if !state_guard.initialized(&buf) {
                    return Ok(false);
                }
                ignore_dbus_no_interface_error!(state_guard.deactivate_im(&buf));
                Ok::<_, OxiError>(false) // NB: return false to keep this autocmd
            }
        })
        .build();
    api::create_autocmd(["InsertLeave"], &opts)?;

    let opts = CreateAutocmdOpts::builder()
        .buffer(Buffer::current())
        .group(augroup_id)
        .desc("Reset input context when leaving window or buffer")
        .callback({
            let ctx_clone = ctx.clone();
            let state_ref = state.clone();
            let buf = buf.clone();
            move |_| {
                let state_guard = state_ref.lock().unwrap();
                if !state_guard.initialized(&buf) {
                    return Ok(false);
                }
                ctx_clone.reset().map_err(as_api_error)?;
                Ok::<_, OxiError>(false) // NB: return false to keep this autocmd
            }
        })
        .build();
    api::create_autocmd(["WinLeave", "BufLeave"], &opts)?;

    // Release the lock before setting up InsertCharPre autocmd
    drop(state_guard);

    // Set up the InsertCharPre event handler
    setup_insert_char_pre(trigger.clone(), buf)?;

    Ok(())
}

pub fn deregister_autocommands(
    state: Arc<Mutex<Fcitx5Plugin>>,
    buf: &Buffer,
) -> oxi::Result<()> {
    let mut state_guard = state.lock().unwrap();
    if let Some(augroup_id) = state_guard.augroup_id.remove(&buf.handle()) {
        api::del_augroup_by_id(augroup_id).map_err(|e| e.into())
    } else {
        Ok(())
    }
}

/// Setup InsertCharPre event to handle candidate selection
pub fn setup_insert_char_pre(trigger: AsyncHandle, buf: &Buffer) -> oxi::Result<()> {
    let state = get_state();
    let state_guard = state.lock().unwrap();

    // Only proceed if initialized
    if !state_guard.initialized(buf) {
        return Ok(());
    }

    let augroup_id = state_guard
        .augroup_id
        .get(&buf.handle())
        .expect("Augroup should be initialized")
        .to_owned();
    let ctx_clone = state_guard.ctx.get(&buf.handle()).unwrap().clone();

    // Drop lock before creating autocmd
    drop(state_guard);

    // Get a reference to the candidate state
    let candidate_state = get_candidate_state();

    let opts = CreateAutocmdOpts::builder()
        .buffer(Buffer::current())
        .group(augroup_id)
        .desc("Process key events for Fcitx5 input method")
        .callback(move |_| {
            // Get the character being inserted using the Neovim API
            let char_arg = if let Ok(char_obj) = api::get_vvar::<String>("char") {
                char_obj
            } else {
                return Ok::<_, oxi::Error>(false);
            };
            let char_arg = char_arg.as_str();

            if char_arg.is_empty() {
                return Ok(false);
            }

            // Clone state for use inside callback
            let candidate_state_clone = candidate_state.clone();
            let mut guard = candidate_state_clone.lock().unwrap();

            // Get the first character (should be only one)
            let c = char_arg.chars().next().unwrap();

            // Send key to Fcitx5
            let code = fcitx5_dbus::utils::key_event::KeyVal::from_char(c);
            let state = fcitx5_dbus::utils::key_event::KeyState::NoState;

            // Process the key in Fcitx5
            if let Ok(accept) = ctx_clone.process_key_event(code, 0, state, false, 0) {
                if accept {
                    api::set_vvar("char", "")?;
                }
            }

            // After processing key:
            guard.mark_for_update(); // Mark that content needs updating

            // Schedule an update on main thread
            trigger.send()?;

            Ok(false)
        })
        .build();

    // Register the autocmd for InsertCharPre
    api::create_autocmd(["InsertCharPre"], &opts)?;

    Ok(())
}
