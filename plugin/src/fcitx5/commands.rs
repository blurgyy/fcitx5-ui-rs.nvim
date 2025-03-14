//! Input method control functions

use fcitx5_dbus::zbus::Result;
use fcitx5_dbus::{
    controller::ControllerProxyBlocking, input_context::InputContextProxyBlocking,
};

/// Toggle between input methods
pub fn toggle_im(
    controller: &ControllerProxyBlocking,
    ctx: &InputContextProxyBlocking,
) -> Result<()> {
    ctx.focus_in()?;
    controller.toggle()?;
    Ok(())
}

/// Switch to English input method if not already active
pub fn set_im_en(
    controller: &ControllerProxyBlocking,
    ctx: &InputContextProxyBlocking,
) -> Result<()> {
    ctx.focus_in()?;
    if controller.current_input_method()? == "pinyin" {
        controller.toggle()?;
    }
    Ok(())
}

/// Switch to Chinese Pinyin input method if not already active
pub fn set_im_zh(
    controller: &ControllerProxyBlocking,
    ctx: &InputContextProxyBlocking,
) -> Result<()> {
    ctx.focus_in()?;
    if controller.current_input_method()? != "pinyin" {
        controller.toggle()?;
    }
    Ok(())
}
