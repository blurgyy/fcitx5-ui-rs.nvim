use fcitx5_dbus::utils::CapabilityFlag;
use fcitx5_dbus::zbus::{blocking::Connection, Result};
use fcitx5_dbus::{
    controller::ControllerProxyBlocking, input_context::InputContextProxyBlocking,
    input_method::InputMethodProxyBlocking,
};
use nvim_oxi::api::Buffer;
use nvim_oxi::{
    self as oxi,
    api::{
        self,
        opts::{CreateAugroupOpts, CreateAutocmdOpts, CreateCommandOpts, EchoOpts},
        Error as ApiError,
    },
};
use std::sync::{Arc, Mutex};

// Structure to hold the plugin state
struct Fcitx5Plugin {
    controller: Option<ControllerProxyBlocking<'static>>,
    ctx: Option<InputContextProxyBlocking<'static>>,
    augroup_id: Option<u32>,
    initialized: bool,
}

impl Fcitx5Plugin {
    fn new() -> Self {
        Self {
            controller: None,
            ctx: None,
            augroup_id: None,
            initialized: false,
        }
    }
}

// Use lazy_static for thread-safe initialization
lazy_static::lazy_static! {
    static ref PLUGIN_STATE: Arc<Mutex<Fcitx5Plugin>> = Arc::new(Mutex::new(Fcitx5Plugin::new()));
}

// Get a reference to the global state
fn get_state() -> Arc<Mutex<Fcitx5Plugin>> {
    PLUGIN_STATE.clone()
}

fn as_api_error(e: impl std::error::Error) -> ApiError {
    ApiError::Other(e.to_string())
}

fn toggle_im(controller: &ControllerProxyBlocking, ctx: &InputContextProxyBlocking) -> Result<()> {
    ctx.focus_in()?;
    controller.toggle()?;
    Ok(())
}

fn set_im_en(controller: &ControllerProxyBlocking, ctx: &InputContextProxyBlocking) -> Result<()> {
    ctx.focus_in()?;
    if controller.current_input_method()? == "pinyin" {
        controller.toggle()?;
    }
    Ok(())
}

fn set_im_zh(controller: &ControllerProxyBlocking, ctx: &InputContextProxyBlocking) -> Result<()> {
    ctx.focus_in()?;
    if controller.current_input_method()? != "pinyin" {
        controller.toggle()?;
    }
    Ok(())
}

fn prepare() -> Result<(
    ControllerProxyBlocking<'static>,
    InputContextProxyBlocking<'static>,
)> {
    let conn = Connection::session()?;
    let controller = ControllerProxyBlocking::new(&conn)?;
    let input_method = InputMethodProxyBlocking::new(&conn)?;

    let (p, _) = input_method.create_input_context(&[("program", "fcitx5-ui-rs.nvim")])?;

    let ctx = InputContextProxyBlocking::builder(&conn).path(p)?.build()?;
    ctx.set_capability(CapabilityFlag::ClientSideInputPanel)?;
    set_im_en(&controller, &ctx)?;

    Ok((controller, ctx))
}

// Setup autocommands for input method switching
fn setup_autocommands(state: Arc<Mutex<Fcitx5Plugin>>) -> oxi::Result<()> {
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
                Ok::<_, oxi::Error>(false) // NB: return false to keep this autocmd
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
                Ok::<_, oxi::Error>(false) // NB: return false to keep this autocmd
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
                Ok::<_, oxi::Error>(false) // NB: return false to keep this autocmd
            }
        })
        .build();
    api::create_autocmd(["WinLeave", "BufLeave"], &opts)?;

    Ok(())
}

// Initialize the connection and input context
fn initialize_fcitx5() -> oxi::Result<()> {
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

// Deactivate the plugin but keep connections
fn deactivate_fcitx5() -> oxi::Result<()> {
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

// Reset the plugin completely - close connections and clean up state
fn reset_fcitx5() -> oxi::Result<()> {
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

#[oxi::plugin]
fn fcitx5_ui_rs() -> oxi::Result<()> {
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
                    return Err(as_api_error(std::io::Error::new(
                        std::io::ErrorKind::Other,
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
                    return Err(as_api_error(std::io::Error::new(
                        std::io::ErrorKind::Other,
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
                return Err(as_api_error(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Fcitx5 plugin not initialized. Run :Fcitx5Initialize first",
                )));
            }

            let controller = state_guard.controller.as_ref().unwrap();
            let ctx = state_guard.ctx.as_ref().unwrap();

            set_im_en(controller, ctx).map_err(as_api_error)
        },
        &CreateCommandOpts::default(),
    )?;

    // Notify user that the plugin has been loaded (but not initialized)
    api::echo(
        vec![(
            "Fcitx5 plugin loaded. Use :Fcitx5Initialize to activate the plugin.",
            None,
        )],
        false,
        &EchoOpts::builder().build(),
    )?;

    Ok(())
}
