use std::sync::{Arc, Mutex};

use nvim_oxi::{
    self as oxi,
    api::{self, opts::SetKeymapOpts, Buffer},
};

use crate::plugin::Fcitx5Plugin;

pub fn register_keymaps(
    state: Arc<Mutex<Fcitx5Plugin>>,
    buf: &Buffer,
) -> oxi::Result<()> {
    let mut state_guard = state.lock().unwrap();

    // Only proceed if initialized
    if !state_guard.initialized(&buf) {
        return Ok(());
    }

    // Save existing keymaps for fallback
    let mut buf = api::get_current_buf();
    state_guard.store_existing_keymaps(&buf)?;

    drop(state_guard);

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

    buf.set_keymap(
        api::types::Mode::Insert,
        "<Left>",
        "<Cmd>Fcitx5TryInsertLeft<CR>",
        &opts,
    )?;

    buf.set_keymap(
        api::types::Mode::Insert,
        "<Right>",
        "<Cmd>Fcitx5TryInsertRight<CR>",
        &opts,
    )?;

    Ok(())
}

pub fn deregister_keymaps(state: Arc<Mutex<Fcitx5Plugin>>) -> oxi::Result<()> {
    let mut buf = api::get_current_buf();

    buf.del_keymap(api::types::Mode::Insert, "<BS>")?;
    buf.del_keymap(api::types::Mode::Insert, "<CR>")?;
    buf.del_keymap(api::types::Mode::Insert, "<Esc>")?;
    buf.del_keymap(api::types::Mode::Insert, "<Left>")?;
    buf.del_keymap(api::types::Mode::Insert, "<Right>")?;

    let mut state_guard = state.lock().unwrap();
    state_guard.restore_existing_keymaps(&buf)?;

    Ok(())
}
