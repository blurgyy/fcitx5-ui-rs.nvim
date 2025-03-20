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

    // Only proceed if initialized, and we did not register the keymaps before for this buffer.
    if !state_guard.initialized(&buf)
        || *state_guard
            .keymaps_registered
            .get(&buf.handle())
            .unwrap_or(&false)
    {
        return Ok(());
    }

    // Save existing keymaps for fallback
    let mut buf = api::get_current_buf();
    state_guard.store_original_keymaps(&buf)?;
    state_guard.keymaps_registered.insert(buf.handle(), true);

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
