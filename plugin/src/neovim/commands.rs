//! Command definitions for Neovim plugin

use std::sync::{Arc, Mutex};

use nvim_oxi::{
    self as oxi,
    api::{self, opts::CreateCommandOpts, Buffer},
    libuv::AsyncHandle,
};

use crate::{
    fcitx5::candidates::setup_im_window_receivers,
    ignore_dbus_no_interface_error,
    plugin::{get_im_window_state, PLUGIN_NAME},
};
use crate::{
    fcitx5::candidates::IMWindowState, neovim::autocmds::register_autocommands,
};
use crate::{
    fcitx5::{candidates::UpdateType, connection::prepare},
    plugin::Fcitx5Plugin,
};
use crate::{lock_logged, plugin::get_state, utils::do_feedkeys_noremap};
use crate::{plugin::get_im_window, utils::as_api_error};

use super::{autocmds::deregister_autocommands, keymaps::register_keymaps};

/// Register all plugin commands
pub fn register_commands() -> oxi::Result<()> {
    let state = get_state();

    // Define user commands
    api::create_user_command(
        "Fcitx5PluginLoad",
        move |_| load_plugin(get_state(), &api::get_current_buf()),
        &CreateCommandOpts::builder()
            .desc("Setup input method auto-activation")
            .build(),
    )?;

    api::create_user_command(
        "Fcitx5PluginUnload",
        move |_| unload_plugin(get_state(), &api::get_current_buf()),
        &CreateCommandOpts::builder()
            .desc("Disable input method auto-activation")
            .build(),
    )?;

    api::create_user_command(
        "Fcitx5PluginToggle",
        move |_| toggle_plugin(get_state(), &api::get_current_buf()),
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
                let buf = api::get_current_buf();
                if !state_guard.initialized(&buf) {
                    oxi::print!(
                        "{PLUGIN_NAME}: not loaded. Run :Fcitx5PluginLoad first"
                    );
                    return Ok(());
                }

                ignore_dbus_no_interface_error!(state_guard.toggle_im(&buf));

                oxi::print!("{}", state_guard.get_im(&buf).map_err(as_api_error)?);

                Ok::<_, oxi::Error>(())
            }
        },
        &CreateCommandOpts::builder()
            .desc("Toggle input method (not toggling the plugin)")
            .build(),
    )?;

    api::create_user_command(
        "Fcitx5IMActivate",
        {
            let state = state.clone();
            move |_| {
                let state_guard = state.lock().unwrap();
                let buf = api::get_current_buf();
                if !state_guard.initialized(&buf) {
                    oxi::print!(
                        "{PLUGIN_NAME}: not loaded. Run :Fcitx5PluginLoad first"
                    );
                    return Ok(());
                }

                ignore_dbus_no_interface_error!(state_guard.activate_im(&buf));
                Ok::<_, oxi::Error>(())
            }
        },
        &CreateCommandOpts::default(),
    )?;

    api::create_user_command(
        "Fcitx5IMDeactivate",
        {
            let state = state.clone();
            move |_| {
                let state_guard = state.lock().unwrap();
                let buf = api::get_current_buf();
                if !state_guard.initialized(&buf) {
                    oxi::print!(
                        "{PLUGIN_NAME}: not loaded. Run :Fcitx5PluginLoad first"
                    );
                    return Ok(());
                }

                ignore_dbus_no_interface_error!(state_guard.deactivate_im(&buf));
                Ok::<_, oxi::Error>(())
            }
        },
        &CreateCommandOpts::default(),
    )?;

    Ok(())
}

pub fn process_im_window_updates(
    im_window_state_arc: Arc<Mutex<IMWindowState>>,
) -> oxi::Result<()> {
    // First, drain all pending updates while holding the mutex, so we do not keep
    // IMWindowState locked while executing UI logic.
    let updates: Vec<UpdateType> = {
        let mut guard = lock_logged!(im_window_state_arc, "IMWindowState");
        let mut drained = Vec::new();
        while let Some(update_type) = guard.pop_update() {
            drained.push(update_type);
        }
        drained
    };

    for update_type in updates {
        match update_type {
            UpdateType::Show => {
                // Build render plan under the IMWindowState lock, then apply it
                // outside to avoid holding the mutex over Neovim calls.
                let (plan, is_visible) = {
                    let mut guard = lock_logged!(im_window_state_arc, "IMWindowState");
                    guard.is_visible = true;
                    (guard.build_render_plan(), guard.is_visible)
                };

                if is_visible {
                    // Apply to buffer and window without holding the IMWindowState lock.
                    let state_guard =
                        lock_logged!(im_window_state_arc, "IMWindowState");
                    if let Some(buffer) = state_guard.buffer.as_ref() {
                        IMWindowState::apply_render_plan_to_buffer(buffer, &plan);
                    }
                    drop(state_guard);

                    // Use the state to drive window configuration using the same plan.
                    let state_guard =
                        lock_logged!(im_window_state_arc, "IMWindowState");
                    state_guard.display_window_from_plan(&plan)?;
                }
            }
            UpdateType::Hide => {
                let plan = {
                    let mut guard = lock_logged!(im_window_state_arc, "IMWindowState");
                    guard.is_visible = false;
                    guard.build_render_plan()
                };

                {
                    let state_guard =
                        lock_logged!(im_window_state_arc, "IMWindowState");
                    if let Some(buffer) = state_guard.buffer.as_ref() {
                        IMWindowState::apply_render_plan_to_buffer(buffer, &plan);
                    }
                }

                // Close the IM window without holding the IMWindowState lock.
                let im_window_global_arc = get_im_window();
                let mut im_window_opt_guard =
                    lock_logged!(im_window_global_arc, "IMWindow");

                if let Some(window_to_close) = im_window_opt_guard.take() {
                    oxi::schedule(move |_| {
                        if window_to_close.is_valid() {
                            match window_to_close.close(true) {
                                Ok(_) => {}
                                Err(e) => eprintln!(
                                    "{}: Error closing window: {}",
                                    PLUGIN_NAME, e,
                                ),
                            }
                        }
                    });
                }
            }
            UpdateType::UpdateContent => {
                let (plan, is_visible) = {
                    let guard = lock_logged!(im_window_state_arc, "IMWindowState");
                    let plan = guard.build_render_plan();
                    (plan, guard.is_visible)
                };

                {
                    let state_guard =
                        lock_logged!(im_window_state_arc, "IMWindowState");
                    if let Some(buffer) = state_guard.buffer.as_ref() {
                        IMWindowState::apply_render_plan_to_buffer(buffer, &plan);
                    }
                }

                if is_visible {
                    let state_guard =
                        lock_logged!(im_window_state_arc, "IMWindowState");
                    state_guard.display_window_from_plan(&plan)?;
                }
            }
            UpdateType::Insert(s) => {
                // The commit_string handler in fcitx5/candidates.rs, which calls
                // mark_for_insert, relies on a subsequent update_client_side_ui signal
                // from fcitx5 to clear preedit/candidates and trigger a Hide action.
                // So, Insert itself doesn't directly hide the window.
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
                        // NB: Force undo break
                        // TODO: Maybe make this configurable from lua side
                        // REF: `:h i_CTRL-G_u`
                        let _ = do_feedkeys_noremap("<C-g>u");
                    }
                });
            }
        }
    }

    Ok(())
}

/// Initialize the connection and input context for current buffer
pub fn load_plugin(state: Arc<Mutex<Fcitx5Plugin>>, buf: &Buffer) -> oxi::Result<()> {
    let mut state_guard = state.lock().unwrap();

    if state_guard.initialized(buf) {
        oxi::print!("{PLUGIN_NAME}: already loaded");
        return Ok(());
    }

    // Initialize the connection
    let (controller, ctx) = if let Ok(Some(pair)) = prepare().map_err(as_api_error) {
        pair
    } else {
        oxi::print!("{PLUGIN_NAME}: failed to connect to DBus");
        return Ok(());
    };

    // Get a reference to the candidate state for setup
    let im_window_state = state_guard.im_window_state.clone();

    // Store in state
    state_guard
        .controller
        .insert(buf.handle(), controller.clone());
    state_guard.ctx.insert(buf.handle(), ctx.clone());
    ignore_dbus_no_interface_error!(state_guard.deactivate_im(buf));

    let trigger =
        AsyncHandle::new(move || process_im_window_updates(get_im_window_state()))?;

    // Setup candidate receivers
    setup_im_window_receivers(&ctx, im_window_state, trigger.clone())
        .map_err(as_api_error)?;

    // if already in insert mode, set the im
    let got_mode = api::get_mode();
    match &std::str::from_utf8(got_mode.mode.as_bytes()) {
        Ok("i") | Ok("R") => {
            ignore_dbus_no_interface_error!(state_guard.activate_im(buf));
        }
        _ => {}
    }

    // Release the lock before setting up autocommands
    drop(state_guard);

    register_autocommands(state.clone(), trigger, buf)?;
    register_keymaps(state.clone(), buf)?;

    Ok(())
}

/// Reset the plugin for current buffer completely - close connections and clean up state
pub fn unload_plugin(state: Arc<Mutex<Fcitx5Plugin>>, buf: &Buffer) -> oxi::Result<()> {
    let mut state_guard = state.lock().unwrap();

    if !state_guard.initialized(buf) {
        oxi::print!("{PLUGIN_NAME}: already unloaded");
        return Ok(());
    }

    // Reset and clear the input context if it exists
    ignore_dbus_no_interface_error!(state_guard.reset_im_ctx(buf));

    state_guard.controller.remove(&buf.handle());
    if let Some(ctx) = state_guard.ctx.remove(&buf.handle()) {
        let _ = ctx.destroy_ic();
    }

    drop(state_guard);

    // Delete the augroup if it exists
    deregister_autocommands(state.clone(), buf)?;
    Ok(())
}

pub fn toggle_plugin(state: Arc<Mutex<Fcitx5Plugin>>, buf: &Buffer) -> oxi::Result<()> {
    let state_guard = state.lock().unwrap();
    if state_guard.initialized(buf) {
        drop(state_guard);
        unload_plugin(get_state(), buf)
    } else {
        drop(state_guard);
        load_plugin(get_state(), buf)
    }
}
