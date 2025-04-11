use std::sync::{Arc, Mutex};

use fcitx5_dbus::utils::key_event::{
    KeyState as Fcitx5KeyState, KeyVal as Fcitx5KeyVal,
};
use nvim_oxi::{
    self as oxi,
    api::{self, opts::SetKeymapOpts, Buffer},
};

use crate::{
    ignore_dbus_no_interface_error,
    plugin::{get_candidate_state, get_state, Fcitx5Plugin},
    utils::{as_api_error, do_feedkeys_noremap, CURSOR_INDICATOR},
};

use super::commands::process_candidate_updates;

fn handle_special_key(nvim_keycode: &str, buf: &Buffer) -> oxi::Result<()> {
    let state = get_state();
    let state_guard = state.lock().unwrap();
    let candidate_guard = state_guard.candidate_state.lock().unwrap();
    if !candidate_guard.is_visible {
        // call the original keymap, if there is one
        if let Some(buf_keymaps) =
            state_guard.existing_keymaps_insert.get(&buf.handle())
        {
            if let Some(km) = buf_keymaps.get(&nvim_keycode.to_lowercase()) {
                if let Some(callback) = km.callback.as_ref() {
                    // ignore the error
                    match callback.call(()) {
                        Ok(()) => {}
                        Err(_) => {
                            // fallback to vanilla key input, ignore any possible error
                            let _ = do_feedkeys_noremap(nvim_keycode);
                        }
                    }
                } else if let Some(rhs) = km.rhs.as_ref() {
                    // ignore any possible error
                    let _ = do_feedkeys_noremap(rhs);
                }
            } else {
                // eprintln!(
                //     "{}: no existing keymaps of key '{}' for current buffer ({})",
                //     PLUGIN_NAME,
                //     nvim_keycode,
                //     buf.handle(),
                // );
                // ignore any possible error
                let _ = do_feedkeys_noremap(nvim_keycode);
            }
        } else {
            // eprintln!(
            //     "{}: warning: current buffer ({}) has no existing keymaps, or they are is not registered",
            //     PLUGIN_NAME,
            //     buf.handle(),
            // );
            // ignore any possible error
            let _ = do_feedkeys_noremap(nvim_keycode);
        }
        return Ok(());
    }

    // if plugin is unloaded, don't do anything
    if !state_guard.initialized(buf) {
        return Ok(());
    }

    drop(candidate_guard);
    drop(state_guard);

    match nvim_keycode.to_lowercase().as_str() {
        key @ ("<bs>" | "<left>" | "<right>" | "<tab>" | "<s-tab>" | "<c-w>" | "") => {
            let state_guard = state.lock().unwrap();
            let ctx = state_guard.ctx.get(&buf.handle()).unwrap();
            let key_code = match key {
                "<bs>" | "<c-w>" | "" => Fcitx5KeyVal::DELETE,
                "<left>" => Fcitx5KeyVal::LEFT,
                "<right>" => Fcitx5KeyVal::RIGHT,
                // REF: <https://github.com/fcitx/fcitx5/blob/b4405d70a6d58ac94b9f06a446e84f777ea5f3b7/src/lib/fcitx-utils/keylist#L3>
                "<tab>" | "<s-tab>" => Fcitx5KeyVal::from_char('\u{FF09}'),
                _ => unreachable!(),
            };
            let key_state = if key.starts_with("<c-s-") || key.starts_with("<s-c-") {
                Fcitx5KeyState::Ctrl_Alt
            } else if key.starts_with("<s-") {
                Fcitx5KeyState::Shift
            } else if key == "" || key.starts_with("<c-") {
                Fcitx5KeyState::Ctrl
            } else {
                Fcitx5KeyState::NoState
            };
            ctx.process_key_event(key_code, 0, key_state, false, 0)
                .map_err(as_api_error)?;
            let mut candidate_guard = state_guard.candidate_state.lock().unwrap();
            candidate_guard.mark_for_update();
            drop(candidate_guard);
            drop(state_guard);
            process_candidate_updates(get_candidate_state())?;
            Ok(())
        }
        "<cr>" => {
            let state_guard = state.lock().unwrap();
            let candidate_state = state_guard.candidate_state.clone();
            let mut candidate_guard = candidate_state.lock().unwrap();
            let insert_text = candidate_guard
                .preedit_text
                .replace([' ', CURSOR_INDICATOR], "")
                .clone();
            candidate_guard.mark_for_insert(insert_text);
            ignore_dbus_no_interface_error!(state_guard.reset_im_ctx(buf));
            drop(candidate_guard);
            oxi::schedule({
                let candidate_state = candidate_state.clone();
                move |_| process_candidate_updates(candidate_state.clone())
            });
            Ok(())
        }
        "<esc>" => {
            let state_guard = state.lock().unwrap();
            ignore_dbus_no_interface_error!(state_guard.reset_im_ctx(buf));
            oxi::schedule(move |_| process_candidate_updates(get_candidate_state()));
            Ok(())
        }
        _ => Ok(()),
    }
}

pub fn register_keymaps(
    state: Arc<Mutex<Fcitx5Plugin>>,
    buf: &Buffer,
) -> oxi::Result<()> {
    let mut state_guard = state.lock().unwrap();

    // Only proceed if initialized, and we did not register the keymaps before for this buffer.
    if !state_guard.initialized(buf)
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

    buf.set_keymap(
        api::types::Mode::Insert,
        "<BS>",
        "",
        &SetKeymapOpts::builder()
            .noremap(true)
            .silent(true)
            .callback(move |_| handle_special_key("<BS>", &api::get_current_buf()))
            .build(),
    )?;

    buf.set_keymap(
        api::types::Mode::Insert,
        "<CR>",
        "<Cmd>Fcitx5TryInsertCarriageReturn<CR>",
        &SetKeymapOpts::builder()
            .noremap(true)
            .silent(true)
            .callback(move |_| handle_special_key("<CR>", &api::get_current_buf()))
            .build(),
    )?;

    buf.set_keymap(
        api::types::Mode::Insert,
        "<Esc>",
        "",
        &SetKeymapOpts::builder()
            .noremap(true)
            .silent(true)
            .callback(move |_| handle_special_key("<Esc>", &api::get_current_buf()))
            .build(),
    )?;

    buf.set_keymap(
        api::types::Mode::Insert,
        "<Left>",
        "",
        &SetKeymapOpts::builder()
            .noremap(true)
            .silent(true)
            .callback(move |_| handle_special_key("<Left>", &api::get_current_buf()))
            .build(),
    )?;

    buf.set_keymap(
        api::types::Mode::Insert,
        "<Right>",
        "",
        &SetKeymapOpts::builder()
            .noremap(true)
            .silent(true)
            .callback(move |_| handle_special_key("<Right>", &api::get_current_buf()))
            .build(),
    )?;

    buf.set_keymap(
        api::types::Mode::Insert,
        "<Tab>",
        "",
        &SetKeymapOpts::builder()
            .noremap(true)
            .silent(true)
            .callback(move |_| handle_special_key("<Tab>", &api::get_current_buf()))
            .build(),
    )?;

    buf.set_keymap(
        api::types::Mode::Insert,
        "<S-Tab>",
        "",
        &SetKeymapOpts::builder()
            .noremap(true)
            .silent(true)
            .callback(move |_| handle_special_key("<S-Tab>", &api::get_current_buf()))
            .build(),
    )?;

    buf.set_keymap(
        api::types::Mode::Insert,
        "<C-w>",
        "",
        &SetKeymapOpts::builder()
            .noremap(true)
            .silent(true)
            .callback(move |_| handle_special_key("<C-w>", &api::get_current_buf()))
            .build(),
    )?;

    buf.set_keymap(
        api::types::Mode::Insert,
        // This is actually <C-BS>, but nvim sees it as this character (use <C-v>, <C-BS>
        // and see for yourself.
        "",
        "",
        &SetKeymapOpts::builder()
            .noremap(true)
            .silent(true)
            .callback(move |_| handle_special_key("", &api::get_current_buf()))
            .build(),
    )?;

    Ok(())
}
