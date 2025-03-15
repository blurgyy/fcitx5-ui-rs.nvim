use std::sync::{Arc, Mutex};

use nvim_oxi::{
    self as oxi,
    api::{self, opts::SetKeymapOpts},
};

use crate::plugin::Fcitx5Plugin;

pub fn register_keymaps(state: Arc<Mutex<Fcitx5Plugin>>) -> oxi::Result<()> {
    let state_guard = state.lock().unwrap();

    // Only proceed if initialized
    if !state_guard.initialized() {
        return Ok(());
    }

    drop(state_guard);

    let mut buf = api::get_current_buf();

    let opts = SetKeymapOpts::builder().noremap(true).silent(true).build();
    buf.set_keymap(
        api::types::Mode::Insert,
        "<BS>",
        "<Cmd>Fcitx5TryInsertBackSpace<CR>",
        &opts,
    )?;

    buf.set_keymap(
        api::types::Mode::Insert,
        "<CR>",
        "<Cmd>Fcitx5TryInsertCarriageReturn<CR>",
        &opts,
    )?;

    buf.set_keymap(
        api::types::Mode::Insert,
        "<Esc>",
        "<Cmd>Fcitx5TryInsertEscape<CR>",
        &opts,
    )?;

    Ok(())
}

pub fn deregister_keymaps() -> oxi::Result<()> {
    let mut buf = api::get_current_buf();

    buf.del_keymap(api::types::Mode::Insert, "<BS>")?;

    buf.del_keymap(api::types::Mode::Insert, "<CR>")?;

    buf.del_keymap(api::types::Mode::Insert, "<Esc>")?;

    Ok(())
}
