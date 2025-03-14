//! Fcitx5 integration for Neovim
//!
//! This plugin provides automatic switching between input methods
//! based on Neovim editor modes.

mod fcitx5;
mod neovim;
mod plugin;
mod utils;

use nvim_oxi::{self as oxi};

#[oxi::plugin]
fn fcitx5_ui_rs() -> oxi::Result<()> {
    // Initialize the plugin's commands
    neovim::commands::register_commands()?;

    Ok(())
}
