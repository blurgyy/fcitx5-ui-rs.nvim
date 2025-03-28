//! Fcitx5 connection management

use fcitx5_dbus::utils::CapabilityFlag;
use fcitx5_dbus::zbus::{blocking::Connection, Result};
use fcitx5_dbus::{
    controller::ControllerProxyBlocking, input_context::InputContextProxyBlocking,
    input_method::InputMethodProxyBlocking,
};

/// Establishes a connection with Fcitx5 and creates an input context
pub fn prepare() -> Result<
    Option<(
        ControllerProxyBlocking<'static>,
        InputContextProxyBlocking<'static>,
    )>,
> {
    let conn = if let Ok(conn) = Connection::session() {
        conn
    } else {
        return Ok(None);
    };
    let controller = ControllerProxyBlocking::new(&conn)?;
    let input_method = InputMethodProxyBlocking::new(&conn)?;

    let (p, _) =
        input_method.create_input_context(&[("program", "fcitx5-ui-rs.nvim")])?;

    let ctx = InputContextProxyBlocking::builder(&conn).path(p)?.build()?;
    ctx.set_capability(CapabilityFlag::ClientSideInputPanel)?;

    Ok(Some((controller, ctx)))
}
