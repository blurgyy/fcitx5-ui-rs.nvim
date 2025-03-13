//! Autocommand setup for Neovim

use nvim_oxi::{
    self as oxi,
    api::{
        self,
        opts::{CreateAugroupOpts, CreateAutocmdOpts, EchoOpts},
        Buffer,
    },
    Error as OxiError,
};

use crate::plugin::Fcitx5Plugin;
use crate::utils::as_api_error;
use crate::{
    fcitx5::commands::{set_im_en, set_im_zh},
    plugin::get_candidate_state,
};
use std::sync::{Arc, Mutex};

/// Setup autocommands for input method switching
pub fn setup_autocommands(state: Arc<Mutex<Fcitx5Plugin>>) -> oxi::Result<()> {
    let mut state_guard = state.lock().unwrap();

    // If already initialized, clean up first
    if let Some(augroup_id) = state_guard.augroup_id {
        api::del_augroup_by_id(augroup_id)?;
    }

    // Create augroup for our autocommands
    let augroup_id = api::create_augroup(
        "fcitx5-ui-rs-nvim",
        &CreateAugroupOpts::builder().clear(true).build(),
    )?;
    state_guard.augroup_id = Some(augroup_id);

    // Ensure we have controller and ctx
    let controller = state_guard
        .controller
        .as_ref()
        .expect("Controller not initialized");
    let ctx = state_guard
        .ctx
        .as_ref()
        .expect("Input context not initialized");

    // Clone for use in callbacks
    let controller_clone = controller.clone();
    let ctx_clone = ctx.clone();

    let opts = CreateAutocmdOpts::builder()
        .buffer(Buffer::current())
        .group(augroup_id)
        .desc("Switch to Pinyin input method when entering insert mode")
        .callback({
            let state_ref = state.clone();
            move |_| {
                let state_guard = state_ref.lock().unwrap();
                if !state_guard.initialized {
                    return Ok(false);
                }
                set_im_zh(&controller_clone, &ctx_clone).map_err(as_api_error)?;
                Ok::<_, OxiError>(false) // NB: return false to keep this autocmd
            }
        })
        .build();
    api::create_autocmd(["InsertEnter"], &opts)?;

    let opts = CreateAutocmdOpts::builder()
        .buffer(Buffer::current())
        .group(augroup_id)
        .desc("Switch to English input method when leaving insert mode")
        .callback({
            let controller_clone = controller.clone();
            let ctx_clone = ctx.clone();
            let state_ref = state.clone();
            move |_| {
                let state_guard = state_ref.lock().unwrap();
                if !state_guard.initialized {
                    return Ok(false);
                }
                set_im_en(&controller_clone, &ctx_clone).map_err(as_api_error)?;
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
            move |_| {
                let state_guard = state_ref.lock().unwrap();
                if !state_guard.initialized {
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
    setup_insert_char_pre(state.clone())?;

    Ok(())
}

/// Setup InsertCharPre event to handle candidate selection
pub fn setup_insert_char_pre(state: Arc<Mutex<Fcitx5Plugin>>) -> oxi::Result<()> {
    let state_guard = state.lock().unwrap();

    // Only proceed if initialized
    if !state_guard.initialized {
        return Ok(());
    }

    let augroup_id = state_guard
        .augroup_id
        .expect("Augroup should be initialized");
    let ctx_clone = state_guard.ctx.as_ref().unwrap().clone();

    // Drop lock before creating autocmd
    drop(state_guard);

    // Get a reference to the candidate state
    let candidate_state = get_candidate_state();

    let opts = CreateAutocmdOpts::builder()
        .group(augroup_id)
        .desc("Process key events for Fcitx5 input method")
        .callback(move |_| {
            // Get the character being inserted using the Neovim API
            let char_arg = if let Ok(char_obj) = api::get_vvar::<String>("char") {
                char_obj
            } else {
                return Ok::<_, oxi::Error>(false);
            };
            api::set_vvar("char", "")?;
            let char_arg = char_arg.as_str();

            api::echo(vec![(char_arg, None)], false, &EchoOpts::builder().build())?;

            if char_arg.is_empty() {
                return Ok(false);
            }

            // Clone state for use inside callback
            let candidate_state_clone = candidate_state.clone();
            let mut guard = candidate_state_clone.lock().unwrap();

            // Get the first character (should be only one)
            let c = char_arg.chars().next().unwrap();

            if guard.is_visible && !guard.candidates.is_empty() {
                match c {
                    '1'..='9' => {
                        // Direct candidate selection by number
                        let idx = (c as u8 - b'1') as usize;
                        if idx < guard.candidates.len() {
                            guard.selected_index = idx;

                            // Select this candidate
                            if let Some(candidate) = guard.get_selected_candidate() {
                                // Use the candidate's text
                                let text = candidate.text.clone();

                                // // Hide the candidate window
                                // guard.hide()?;

                                // // We need to clear the character that triggered this (the number)
                                // // and insert the candidate instead
                                // api::input("<BS>")?; // Delete the number key

                                // // Insert the candidate text
                                // api::input(text)?;
                                api::set_vvar("char", text)?;
                                guard.update_display()?;
                            }
                        }
                    }
                    // Tab for next candidate
                    '\t' => {
                        guard.select_next();
                        guard.update_display()?;
                    }
                    // Shift-Tab for previous candidate
                    '\u{19}' => {
                        // Shift-Tab character
                        guard.select_previous();
                        guard.update_display()?;
                    }
                    // Escape to cancel
                    '\u{1b}' => {
                        // Escape character
                        guard.hide()?;
                    }
                    // Other keys should be passed through to Fcitx5
                    _ => {}
                }
            }

            // Send key to Fcitx5
            let code = fcitx5_dbus::utils::key_event::KeyVal::from_char(c);
            let state = fcitx5_dbus::utils::key_event::KeyState::NoState;

            // Process the key in Fcitx5
            ctx_clone
                .process_key_event(code, 0, state, false, 0)
                .map_err(as_api_error)?;

            // By default, don't interfere with the key press
            Ok(false)
        })
        .build();

    // Register the autocmd for InsertCharPre
    api::create_autocmd(["InsertCharPre"], &opts)?;

    api::echo(
        vec![("InsertCharPre autocmd registered", None)],
        false,
        &EchoOpts::builder().build(),
    )?;

    Ok(())
}
