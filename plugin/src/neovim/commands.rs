//! Command definitions for Neovim plugin

use nvim_oxi::{
    self as oxi,
    api::{
        self,
        opts::{CreateCommandOpts, EchoOpts},
    },
    libuv::AsyncHandle,
};

use std::{io::Error as IoError, sync::Arc};
use std::{io::ErrorKind, sync::Mutex};

use crate::fcitx5::{candidates::UpdateType, connection::prepare};
use crate::plugin::get_state;
use crate::utils::as_api_error;
use crate::{fcitx5::candidates::CandidateState, neovim::autocmds::setup_autocommands};
use crate::{
    fcitx5::{
        candidates::setup_candidate_receivers,
        commands::{set_im_en, set_im_zh, toggle_im},
    },
    plugin::get_candidate_state,
};

fn simulate_backspace() -> oxi::Result<()> {
    let mut buf = api::get_current_buf();
    let win = api::get_current_win();
    if let Ok((row_1b, col_0b)) = win.get_cursor() {
        let row_0b = row_1b - 1;
        if col_0b > 0 {
            if let Some(line) = buf.get_lines(row_0b..=row_0b, true)?.next() {
                // String::len() returns number of bytes, should calculate number of characters
                let s = line.to_string();
                let chars = s.chars();
                let n_chars = chars.clone().count();
                let new_line: String = chars.take(n_chars - 1).collect();
                // replace whole line
                buf.set_text(row_0b..row_0b, 0, col_0b, vec![new_line])?;
            }
        } else {
            assert!(col_0b == 0);
            if row_0b > 0 {
                if let Some(line) = buf.get_lines(row_0b - 1..row_0b, true)?.next() {
                    buf.set_text(row_0b - 1..row_0b, 0, 0, vec![line])?;
                }
            }
        }
    }
    Ok(())
}

/// Register all plugin commands
pub fn register_commands() -> oxi::Result<()> {
    let state = get_state();

    // Define user commands
    api::create_user_command(
        "Fcitx5Initialize",
        |_| initialize_fcitx5(),
        &CreateCommandOpts::default(),
    )?;

    api::create_user_command(
        "Fcitx5Reset",
        |_| reset_fcitx5(),
        &CreateCommandOpts::default(),
    )?;

    // These commands will check if initialized before proceeding
    api::create_user_command(
        "Fcitx5Toggle",
        {
            let state = state.clone();
            move |_| {
                let state_guard = state.lock().unwrap();
                if !state_guard.initialized {
                    return Err(as_api_error(IoError::new(
                        ErrorKind::Other,
                        "Fcitx5 plugin not initialized. Run :Fcitx5Initialize first",
                    )));
                }

                let controller = state_guard.controller.as_ref().unwrap();
                let ctx = state_guard.ctx.as_ref().unwrap();

                toggle_im(controller, ctx).map_err(as_api_error)?;

                api::echo(
                    vec![(
                        format!(
                            "current IM: {}",
                            controller.current_input_method().map_err(as_api_error)?
                        ),
                        None,
                    )],
                    false,
                    &EchoOpts::builder().build(),
                )
            }
        },
        &CreateCommandOpts::default(),
    )?;

    api::create_user_command(
        "Fcitx5Pinyin",
        {
            let state = state.clone();
            move |_| {
                let state_guard = state.lock().unwrap();
                if !state_guard.initialized {
                    return Err(as_api_error(IoError::new(
                        ErrorKind::Other,
                        "Fcitx5 plugin not initialized. Run :Fcitx5Initialize first",
                    )));
                }

                let controller = state_guard.controller.as_ref().unwrap();
                let ctx = state_guard.ctx.as_ref().unwrap();

                set_im_zh(controller, ctx).map_err(as_api_error)
            }
        },
        &CreateCommandOpts::default(),
    )?;

    api::create_user_command(
        "Fcitx5English",
        {
            let state = state.clone();
            move |_| {
                let state_guard = state.lock().unwrap();
                if !state_guard.initialized {
                    return Err(as_api_error(IoError::new(
                        ErrorKind::Other,
                        "Fcitx5 plugin not initialized. Run :Fcitx5Initialize first",
                    )));
                }

                let controller = state_guard.controller.as_ref().unwrap();
                let ctx = state_guard.ctx.as_ref().unwrap();

                set_im_en(controller, ctx).map_err(as_api_error)
            }
        },
        &CreateCommandOpts::default(),
    )?;

    api::create_user_command(
        "Fcitx5TryDeleteChar",
        {
            let state = state.clone();
            move |_| {
                let state_guard = state.lock().unwrap();
                if !state_guard.initialized {
                    // eprintln!("passing through");
                    oxi::schedule(move |_| simulate_backspace());
                    return Ok::<_, oxi::Error>(());
                }
                let mut candidate_guard = state_guard.candidate_state.lock().unwrap();
                if !candidate_guard.is_visible {
                    oxi::schedule(move |_| simulate_backspace());
                    return Ok::<_, oxi::Error>(());
                }
                let ctx = state_guard.ctx.as_ref().unwrap();
                let fcitx5_key_code = fcitx5_dbus::utils::key_event::KeyVal::DELETE;
                let fcitx5_key_state = fcitx5_dbus::utils::key_event::KeyState::NoState;
                ctx.process_key_event(fcitx5_key_code, 0, fcitx5_key_state, false, 0)
                    .map_err(as_api_error)?;
                candidate_guard.mark_for_update();
                drop(candidate_guard);
                oxi::schedule(move |_| {
                    let _ = process_candidate_updates(get_candidate_state());
                });
                Ok(())
            }
        },
        &CreateCommandOpts::default(),
    )?;

    Ok(())
}

// Process updates when scheduled
pub fn process_candidate_updates(candidate_state: Arc<Mutex<CandidateState>>) -> oxi::Result<()> {
    // Get the state and check for pending updates
    let mut guard = candidate_state.lock().unwrap();
    // Process any pending updates
    while let Some(update_type) = guard.pop_update() {
        match update_type {
            UpdateType::Show => {
                guard.setup_window()?;
                guard.update_display()?;

                // Show the window (this is now safe to call)
                if let Some(window) = &guard.window_id {
                    if !window.is_valid() {
                        // Window was invalidated, recreate it
                        guard.window_id = None;
                        guard.setup_window()?;
                    }
                }
            }
            UpdateType::Hide => {
                if let Some(window) = guard.window_id.take() {
                    if window.is_valid() {
                        match window.close(true) {
                            Ok(_) => (),
                            Err(e) => eprintln!("Error closing window: {}", e),
                        }
                    }
                }
                guard.is_visible = false;
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

/// Initialize the connection and input context
pub fn initialize_fcitx5() -> oxi::Result<()> {
    let state = get_state();
    let mut state_guard = state.lock().unwrap();

    if state_guard.initialized {
        api::echo(
            vec![("Fcitx5 plugin already initialized", None)],
            false,
            &EchoOpts::builder().build(),
        )?;
        return Ok(());
    }

    // Initialize the connection
    let (controller, ctx) = prepare().map_err(as_api_error)?;

    // Get a reference to the candidate state for setup
    let candidate_state = state_guard.candidate_state.clone();

    // Store in state
    state_guard.controller = Some(controller);
    state_guard.ctx = Some(ctx.clone());
    state_guard.initialized = true;

    // Spawn a thread for updating the candidate window
    let trigger = AsyncHandle::new(move || process_candidate_updates(get_candidate_state()))?;

    // Setup candidate receivers
    setup_candidate_receivers(&ctx, candidate_state, trigger.clone()).map_err(as_api_error)?;

    // Release the lock before setting up autocommands
    drop(state_guard);

    // Setup autocommands
    setup_autocommands(state.clone(), trigger)?;

    api::echo(
        vec![("Fcitx5 plugin initialized and activated", None)],
        false,
        &EchoOpts::builder().build(),
    )?;

    Ok(())
}

/// Reset the plugin completely - close connections and clean up state
pub fn reset_fcitx5() -> oxi::Result<()> {
    let state = get_state();
    let mut state_guard = state.lock().unwrap();

    if !state_guard.initialized && state_guard.controller.is_none() {
        api::echo(
            vec![("Fcitx5 plugin already reset", None)],
            false,
            &EchoOpts::builder().build(),
        )?;
        return Ok(());
    }

    // Delete the augroup if it exists
    if let Some(augroup_id) = state_guard.augroup_id {
        api::del_augroup_by_id(augroup_id)?;
        state_guard.augroup_id = None;
    }

    // Reset and clear the input context if it exists
    if let Some(ctx) = &state_guard.ctx {
        ctx.reset().map_err(as_api_error)?;
    }

    // Clear state
    state_guard.controller = None;
    state_guard.ctx = None;
    state_guard.initialized = false;

    api::echo(
        vec![("Fcitx5 plugin reset completely", None)],
        false,
        &EchoOpts::builder().build(),
    )?;

    Ok(())
}
