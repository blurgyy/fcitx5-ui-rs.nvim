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
    println!("Path is : {}\n", p);

    let ctx = InputContextProxyBlocking::builder(&conn).path(p)?.build()?;
    ctx.set_capability(CapabilityFlag::ClientSideInputPanel)?;
    set_im_en(&controller, &ctx)?;

    Ok((controller, ctx))
}

#[oxi::plugin]
fn fcitx5_ui_rs() -> oxi::Result<()> {
    let (controller, ctx) = prepare().map_err(as_api_error)?;

    // Create augroup for our autocommands
    let augroup_id = api::create_augroup(
        "fcitx5-ui-rs-nvim",
        &CreateAugroupOpts::builder().clear(true).build(),
    )?;

    let opts = CreateAutocmdOpts::builder()
        .buffer(Buffer::current())
        .group(augroup_id)
        .desc("Switch to Pinyin input method when entering insert mode")
        .callback({
            let (controller, ctx) = (controller.clone(), ctx.clone());
            move |_| {
                set_im_zh(&controller, &ctx).map_err(as_api_error)?;
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
            let (controller, ctx) = (controller.clone(), ctx.clone());
            move |_| {
                api::echo(
                    vec![(format!("Fcitx5Test command executed successfully"), None)],
                    false,
                    &EchoOpts::builder().build(),
                )?;
                set_im_en(&controller, &ctx).map_err(as_api_error)?;
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
            let ctx = ctx.clone();
            move |_| {
                ctx.reset().map_err(as_api_error)?;
                Ok::<_, oxi::Error>(false) // NB: return false to keep this autocmd
            }
        })
        .build();
    api::create_autocmd(["WinLeave", "BufLeave"], &opts)?;

    api::create_user_command(
        "Fcitx5Toggle",
        {
            let (controller, ctx) = (controller.clone(), ctx.clone());
            move |_| {
                toggle_im(&controller, &ctx).map_err(as_api_error)?;
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
            let (controller, ctx) = (controller.clone(), ctx.clone());
            move |_| set_im_zh(&controller, &ctx)
        },
        &CreateCommandOpts::default(),
    )?;

    api::create_user_command(
        "Fcitx5English",
        {
            let (controller, ctx) = (controller.clone(), ctx.clone());
            move |_| set_im_en(&controller, &ctx).map_err(as_api_error)
        },
        &CreateCommandOpts::default(),
    )?;

    api::create_user_command(
        "Fcitx5Reset",
        {
            let ctx = ctx.clone();
            move |_| ctx.reset().map_err(as_api_error)
        },
        &CreateCommandOpts::default(),
    )?;

    // Notify user that the plugin has been initialized
    api::echo(
        vec![(format!("Fcitx5 plugin initialized successfully"), None)],
        false,
        &EchoOpts::builder().build(),
    )?;

    Ok(())
}
