//! Fcitx5 connection management

use fcitx5_dbus::utils::CapabilityFlag;
use fcitx5_dbus::zbus::{blocking::Connection, Result};
use fcitx5_dbus::{
    controller::ControllerProxyBlocking, input_context::InputContextProxyBlocking,
    input_method::InputMethodProxyBlocking,
};

use crate::fcitx5::commands::set_im_en;

/// Establishes a connection with Fcitx5 and creates an input context
pub fn prepare() -> Result<(
    ControllerProxyBlocking<'static>,
    InputContextProxyBlocking<'static>,
)> {
    let conn = Connection::session()?;
    let controller = ControllerProxyBlocking::new(&conn)?;
    let input_method = InputMethodProxyBlocking::new(&conn)?;

    let (p, _) =
        input_method.create_input_context(&[("program", "fcitx5-ui-rs.nvim")])?;

    let ctx = InputContextProxyBlocking::builder(&conn).path(p)?.build()?;
    ctx.set_capability(CapabilityFlag::ClientSideInputPanel)?;
    set_im_en(&controller, &ctx)?;

    Ok((controller, ctx))
}
