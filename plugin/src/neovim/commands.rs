//! Command definitions for Neovim plugin

use nvim_oxi::{
    self as oxi,
    api::{
        self,
        opts::{CreateCommandOpts, EchoOpts},
    },
    mlua,
};

use std::{io::Error as IoError, sync::Arc};
use std::{io::ErrorKind, sync::Mutex};

use crate::neovim::autocmds::setup_autocommands;
use crate::plugin::get_state;
use crate::utils::as_api_error;
use crate::{fcitx5::connection::prepare, plugin::get_candidate_state};
use crate::{
    fcitx5::{
        candidates::setup_candidate_receivers,
        commands::{set_im_en, set_im_zh, toggle_im},
    },
    plugin::Fcitx5Plugin,
};

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

/// Setup a timer to update the candidate window using a background thread
/// instead of Lua timers to avoid potential Lua callback issues
fn setup_candidate_timer(state: Arc<Mutex<Fcitx5Plugin>>) -> oxi::Result<()> {
    // Spawn a background thread for the timer
    std::thread::spawn(move || {
        loop {
            // Sleep for 100ms
            std::thread::sleep(std::time::Duration::from_millis(100));

            // Try to acquire locks non-blocking
            let state_guard = match state.try_lock() {
                Ok(guard) => guard,
                Err(_) => continue, // Skip this tick if lock is contended
            };

            if !state_guard.initialized {
                continue;
            }

            // Get a clone of candidate state
            let candidate_state = state_guard.candidate_state.clone();
            drop(state_guard);

            let candidate_guard = match candidate_state.try_lock() {
                Ok(guard) => guard,
                Err(_) => continue,
            };

            // eprintln!(
            //     " --> candidate.is_visible: {}, candidate.candidates: {:?}",
            //     candidate_guard.is_visible,
            //     candidate_guard.candidates.clone(),
            // );

            // Check if we need to update the UI
            let should_show = candidate_guard.is_visible && !candidate_guard.candidates.is_empty();
            let should_hide = !candidate_guard.is_visible && candidate_guard.window_id.is_some();

            // Drop lock before interacting with Neovim API
            drop(candidate_guard);

            // Schedule UI updates through Neovim's RPC mechanism (which is thread-safe)
            if should_show {
                // We can't directly call nvim_oxi functions from another thread
                // So we use a Lua command to be executed in the main thread
                let _ = nvim_oxi::api::command("lua fcitx5_ui_rs.show_candidate_window()");
            } else if should_hide {
                let _ = nvim_oxi::api::command("lua fcitx5_ui_rs.hide_candidate_window()");
            }
        }
    });

    register_lua_functions()?;

    Ok(())
}

pub fn register_lua_functions() -> oxi::Result<()> {
    let lua = mlua::lua();

    // Create a module table
    let module = lua.create_table()?;

    // Register update function
    module.set(
        "show_candidate_window",
        lua.create_function(|_, _: ()| {
            let candidate_state = get_candidate_state();
            let mut guard = candidate_state.lock().unwrap();
            if guard.is_visible && !guard.candidates.is_empty() {
                guard
                    .setup_window()
                    .and_then(|_| guard.update_display())
                    .and_then(|_| guard.show());
            }
            Ok(())
        })?,
    )?;

    // Register hide function
    module.set(
        "hide_candidate_window",
        lua.create_function(|_, _: ()| {
            let candidate_state = get_candidate_state();
            let mut guard = candidate_state.lock().unwrap();
            guard.hide();
            Ok(())
        })?,
    )?;

    // Register the module
    lua.globals().set("fcitx5_ui_rs", module)?;

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

    // Setup candidate receivers
    setup_candidate_receivers(&ctx, candidate_state).map_err(as_api_error)?;

    // Release the lock before setting up autocommands
    drop(state_guard);

    // Setup autocommands
    setup_autocommands(state.clone())?;

    setup_candidate_timer(state.clone())?;

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
