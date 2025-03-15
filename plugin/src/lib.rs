//! Fcitx5 integration for Neovim
//!
//! This plugin provides automatic switching between input methods
//! based on Neovim editor modes.

mod fcitx5;
mod neovim;
mod plugin;
mod utils;

use nvim_oxi::{self as oxi, Dictionary, Function};

#[oxi::plugin]
fn fcitx5_ui_rs() -> oxi::Dictionary {
    Dictionary::from_iter([("setup", Function::from_fn(neovim::functions::setup))])
}
