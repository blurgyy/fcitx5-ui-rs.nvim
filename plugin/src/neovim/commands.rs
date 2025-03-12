//! Command definitions for Neovim plugin

use nvim_oxi::{
    self as oxi,
    api::{
        self,
        opts::{CreateCommandOpts, EchoOpts},
    },
};

use std::io::Error as IoError;
use std::io::ErrorKind;

use crate::fcitx5::{
    candidates::setup_candidate_receivers,
    commands::{set_im_en, set_im_zh, toggle_im},
};
use crate::neovim::autocmds::setup_autocommands;
use crate::plugin::get_state;
use crate::utils::as_api_error;
use crate::{fcitx5::connection::prepare, plugin::get_candidate_state};

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
        "Fcitx5Deactivate",
        |_| deactivate_fcitx5(),
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
        },
        &CreateCommandOpts::default(),
    )?;

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
    let candidate_state = get_candidate_state();

    // Setup candidate receivers
    setup_candidate_receivers(&ctx, candidate_state).map_err(as_api_error)?;

    // Store in state
    state_guard.controller = Some(controller);
    state_guard.ctx = Some(ctx);
    state_guard.initialized = true;

    // Release the lock before setting up autocommands
    drop(state_guard);

    // Setup autocommands
    setup_autocommands(state.clone())?;

    api::echo(
        vec![("Fcitx5 plugin initialized and activated", None)],
        false,
        &EchoOpts::builder().build(),
    )?;

    Ok(())
}

/// Deactivate the plugin but keep connections
pub fn deactivate_fcitx5() -> oxi::Result<()> {
    let state = get_state();
    let mut state_guard = state.lock().unwrap();

    if !state_guard.initialized {
        api::echo(
            vec![("Fcitx5 plugin not initialized", None)],
            false,
            &EchoOpts::builder().build(),
        )?;
        return Ok(());
    }

    // Set initialized to false to disable callbacks
    state_guard.initialized = false;

    // If we have an input context, reset it
    if let Some(ctx) = &state_guard.ctx {
        if let Some(controller) = &state_guard.controller {
            set_im_en(controller, ctx).map_err(as_api_error)?;
        }
        ctx.reset().map_err(as_api_error)?;
    }

    api::echo(
        vec![("Fcitx5 plugin deactivated", None)],
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
