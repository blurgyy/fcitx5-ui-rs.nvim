//! Command definitions for Neovim plugin

use nvim_oxi::{
    self as oxi,
    api::{self, opts::CreateCommandOpts},
    libuv::AsyncHandle,
};

use std::{io::Error as IoError, sync::Arc};
use std::{io::ErrorKind, sync::Mutex};

use fcitx5_dbus::utils::key_event::{
    KeyState as Fcitx5KeyState, KeyVal as Fcitx5KeyVal,
};

use crate::plugin::get_state;
use crate::{
    fcitx5::candidates::setup_candidate_receivers, ignore_dbus_no_interface_error,
    plugin::get_candidate_state,
};
use crate::{
    fcitx5::candidates::CandidateState, neovim::autocmds::register_autocommands,
};
use crate::{
    fcitx5::{candidates::UpdateType, connection::prepare},
    plugin::Fcitx5Plugin,
};
use crate::{plugin::get_candidate_window, utils::as_api_error};

use super::{
    autocmds::deregister_autocommands,
    keymaps::{deregister_keymaps, register_keymaps},
};

fn handle_special_key(
    nvim_keycode: &str,
    the_char: char,
    bufnr: &i32,
) -> oxi::Result<()> {
    let state = get_state();
    let state_guard = state.lock().unwrap();
    let candidate_guard = state_guard.candidate_state.lock().unwrap();
    if !candidate_guard.is_visible {
        api::feedkeys(&the_char.to_string(), api::types::Mode::Normal, true);
        return Ok(());
    }

    drop(candidate_guard);
    drop(state_guard);

    match nvim_keycode.to_lowercase().as_str() {
        key @ ("<bs>" | "<left>" | "<right>") => {
            let state_guard = state.lock().unwrap();
            let ctx = state_guard.ctx.get(bufnr).unwrap();
            let key_code = match key {
                "<bs>" => Fcitx5KeyVal::DELETE,
                "<left>" => Fcitx5KeyVal::LEFT,
                "<right>" => Fcitx5KeyVal::RIGHT,
                _ => unreachable!(),
            };
            let key_state = Fcitx5KeyState::NoState;
            ctx.process_key_event(key_code, 0, key_state, false, 0)
                .map_err(as_api_error)?;
            let mut candidate_guard = state_guard.candidate_state.lock().unwrap();
            candidate_guard.mark_for_update();
            drop(candidate_guard);
            drop(state_guard);
            process_candidate_updates(get_candidate_state())?;
            Ok(())
        }
        "<cr>" => {
            let state_guard = state.lock().unwrap();
            let candidate_state = state_guard.candidate_state.clone();
            let mut candidate_guard = candidate_state.lock().unwrap();
            let insert_text = candidate_guard.preedit_text.replace(' ', "").clone();
            candidate_guard.mark_for_insert(insert_text);
            ignore_dbus_no_interface_error!(state_guard.reset_im_ctx(bufnr));
            drop(candidate_guard);
            oxi::schedule({
                let candidate_state = candidate_state.clone();
                move |_| process_candidate_updates(candidate_state.clone())
            });
            Ok(())
        }
        "<esc>" => {
            let state_guard = state.lock().unwrap();
            ignore_dbus_no_interface_error!(state_guard.reset_im_ctx(bufnr));
            oxi::schedule(move |_| process_candidate_updates(get_candidate_state()));
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Register all plugin commands
pub fn register_commands() -> oxi::Result<()> {
    let state = get_state();

    // Define user commands
    api::create_user_command(
        "Fcitx5PluginLoad",
        move |_| load_plugin(get_state(), &api::get_current_buf().handle()),
        &CreateCommandOpts::builder()
            .desc("Setup input method auto-activation")
            .build(),
    )?;

    api::create_user_command(
        "Fcitx5PluginUnload",
        move |_| unload_plugin(get_state(), &api::get_current_buf().handle()),
        &CreateCommandOpts::builder()
            .desc("Disable input method auto-activation")
            .build(),
    )?;

    api::create_user_command(
        "Fcitx5PluginToggle",
        move |_| toggle_plugin(get_state(), &api::get_current_buf().handle()),
        &CreateCommandOpts::builder()
            .desc("Toggle input method auto-activation")
            .build(),
    )?;

    // These commands will check if initialized before proceeding
    api::create_user_command(
        "Fcitx5IMToggle",
        {
            let state = state.clone();
            move |_| {
                let state_guard = state.lock().unwrap();
                let bufnr = api::get_current_buf().handle();
                if !state_guard.initialized(&bufnr) {
                    return Err(as_api_error(IoError::new(
                        ErrorKind::Other,
                        "Fcitx5 plugin not loaded. Run :Fcitx5PluginLoad first",
                    )));
                }

                ignore_dbus_no_interface_error!(state_guard.toggle_im(&bufnr));

                oxi::print!("{}", state_guard.get_im(&bufnr).map_err(as_api_error)?);

                Ok(())
            }
        },
        &CreateCommandOpts::builder()
            .desc("Toggle input method (not toggling the plugin)")
            .build(),
    )?;

    api::create_user_command(
        "Fcitx5IMPinyin",
        {
            let state = state.clone();
            move |_| {
                let state_guard = state.lock().unwrap();
                let bufnr = api::get_current_buf().handle();
                if !state_guard.initialized(&bufnr) {
                    return Err(as_api_error(IoError::new(
                        ErrorKind::Other,
                        "Fcitx5 plugin not loaded. Run :Fcitx5PluginLoad first",
                    )));
                }

                ignore_dbus_no_interface_error!(state_guard.activate_im(&bufnr));
                Ok(())
            }
        },
        &CreateCommandOpts::default(),
    )?;

    api::create_user_command(
        "Fcitx5IMEnglish",
        {
            let state = state.clone();
            move |_| {
                let state_guard = state.lock().unwrap();
                let bufnr = api::get_current_buf().handle();
                if !state_guard.initialized(&bufnr) {
                    return Err(as_api_error(IoError::new(
                        ErrorKind::Other,
                        "Fcitx5 plugin not loaded. Run :Fcitx5PluginLoad first",
                    )));
                }

                ignore_dbus_no_interface_error!(state_guard.deactivate_im(&bufnr));
                Ok(())
            }
        },
        &CreateCommandOpts::default(),
    )?;

    api::create_user_command(
        "Fcitx5TryInsertBackSpace",
        move |_| handle_special_key("<BS>", '\x08', &api::get_current_buf().handle()),
        &CreateCommandOpts::default(),
    )?;

    api::create_user_command(
        "Fcitx5TryInsertCarriageReturn",
        move |_| handle_special_key("<CR>", '\n', &api::get_current_buf().handle()),
        &CreateCommandOpts::default(),
    )?;

    api::create_user_command(
        "Fcitx5TryInsertEscape",
        move |_| handle_special_key("<Esc>", '\x1b', &api::get_current_buf().handle()),
        &CreateCommandOpts::default(),
    )?;

    api::create_user_command(
        "Fcitx5TryInsertLeft",
        move |_| handle_special_key("<Left>", '\x1b', &api::get_current_buf().handle()),
        &CreateCommandOpts::default(),
    )?;

    api::create_user_command(
        "Fcitx5TryInsertRight",
        move |_| {
            handle_special_key("<Right>", '\x1b', &api::get_current_buf().handle())
        },
        &CreateCommandOpts::default(),
    )?;

    Ok(())
}

// Process updates when scheduled
pub fn process_candidate_updates(
    candidate_state: Arc<Mutex<CandidateState>>,
) -> oxi::Result<()> {
    // Get the state and check for pending updates
    let mut guard = candidate_state.lock().unwrap();
    while let Some(update_type) = guard.pop_update() {
        match update_type {
            UpdateType::Show => {
                guard.is_visible = true;
                guard.setup_window()?;
                guard.update_display()?;
            }
            UpdateType::Hide => {
                guard.is_visible = false;
                let candidate_window = get_candidate_window();
                let mut candidate_window_guard = candidate_window.lock().unwrap();
                if let Some(window) = candidate_window_guard.take() {
                    if window.is_valid() {
                        oxi::schedule(move |_| match window.close(true) {
                            Ok(_) => {}
                            Err(e) => eprintln!("Error closing window: {}", e),
                        });
                    }
                }
            }
            UpdateType::UpdateContent => {
                guard.update_display()?;
            }
            UpdateType::Insert(s) => {
                // NB: must use oxi::schedule here, otherwise it hangs/segfaults at runtime
                oxi::schedule(move |_| {
                    // Insert text directly at cursor position
                    let mut win = api::get_current_win();
                    let mut buf = api::get_current_buf();
                    if let Ok((row, col)) = win.get_cursor() {
                        // Convert to 0-indexed for the API
                        let row_idx = row - 1;
                        // Insert text at cursor position
                        let _ = buf.set_text(
                            row_idx..row_idx, // Only modify the current line
                            col,              // Start column
                            col, // End column (same as start to insert without replacing)
                            vec![s.clone()], // Text to insert as a Vec<String>
                        );
                        // Move cursor to end of inserted text
                        let _ = win.set_cursor(row, col + s.len());
                    }
                });
            }
        }
    }

    Ok(())
}

/// Initialize the connection and input context for current buffer
pub fn load_plugin(state: Arc<Mutex<Fcitx5Plugin>>, bufnr: &i32) -> oxi::Result<()> {
    let mut state_guard = state.lock().unwrap();

    if state_guard.initialized(bufnr) {
        oxi::print!("Fcitx5 plugin already loaded");
        return Ok(());
    }

    // Initialize the connection
    let (controller, ctx) = prepare().map_err(as_api_error)?;

    // Get a reference to the candidate state for setup
    let candidate_state = state_guard.candidate_state.clone();

    // Store in state
    state_guard.controller = Some(controller.clone());
    state_guard.ctx.insert(*bufnr, ctx.clone());
    ignore_dbus_no_interface_error!(state_guard.deactivate_im(&bufnr));

    let trigger =
        AsyncHandle::new(move || process_candidate_updates(get_candidate_state()))?;

    // Setup candidate receivers
    setup_candidate_receivers(&ctx, candidate_state, trigger.clone())
        .map_err(as_api_error)?;

    // if already in insert mode, set the im
    if let Ok(got_mode) = api::get_mode() {
        if got_mode.mode == api::types::Mode::Insert {
            ignore_dbus_no_interface_error!(state_guard.activate_im(&bufnr));
        }
    }

    // Release the lock before setting up autocommands
    drop(state_guard);

    // Setup autocommands
    register_autocommands(state.clone(), trigger, &bufnr)?;

    register_keymaps(state.clone(), &bufnr)?;

    Ok(())
}

/// Reset the plugin for current buffer completely - close connections and clean up state
pub fn unload_plugin(state: Arc<Mutex<Fcitx5Plugin>>, bufnr: &i32) -> oxi::Result<()> {
    let state_guard = state.lock().unwrap();

    if !state_guard.initialized(&bufnr) && state_guard.controller.is_none() {
        oxi::print!("Fcitx5 plugin already unloaded");
        return Ok(());
    }

    // Reset and clear the input context if it exists
    ignore_dbus_no_interface_error!(state_guard.reset_im_ctx(&bufnr));

    // Clear state
    // state_guard.controller = None;
    state_guard
        .ctx
        .get(bufnr)
        .and_then(|ic| ic.destroy_ic().ok());

    drop(state_guard);

    // Delete the augroup if it exists
    deregister_autocommands(state, &bufnr)?;
    deregister_keymaps()?;

    Ok(())
}

pub fn toggle_plugin(state: Arc<Mutex<Fcitx5Plugin>>, bufnr: &i32) -> oxi::Result<()> {
    let state_guard = state.lock().unwrap();
    if state_guard.initialized(bufnr) {
        drop(state_guard);
        unload_plugin(get_state(), bufnr)
    } else {
        drop(state_guard);
        load_plugin(get_state(), bufnr)
    }
}
