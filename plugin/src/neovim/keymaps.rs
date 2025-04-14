use std::sync::{Arc, Mutex};

use nvim_oxi::{
    self as oxi,
    api::{self, opts::SetKeymapOpts, Buffer},
};

use crate::{
    plugin::{
        get_candidate_state, get_state, Fcitx5Plugin, KEYMAPS, PASSTHROUGH_KEYMAPS,
        PLUGIN_NAME,
    },
    utils::{as_api_error, do_feedkeys_noremap},
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
        key @ _ if PASSTHROUGH_KEYMAPS.keys().any(|k| k.to_lowercase() == key) => {
            let state_guard = state.lock().unwrap();
            let ctx = state_guard.ctx.get(&buf.handle()).unwrap();
            let (key_state, key_code) = PASSTHROUGH_KEYMAPS.get(key).unwrap_or_else(|| {
                unreachable!("{PLUGIN_NAME}: A key '{key}' is supplied, but there has not been a mapping defined for it!")
            });
            ctx.process_key_event(*key_code, 0, *key_state, false, 0)
                .map_err(as_api_error)?;
            let mut candidate_guard = state_guard.candidate_state.lock().unwrap();
            candidate_guard.mark_for_update();
            drop(candidate_guard);
            drop(state_guard);
            process_candidate_updates(get_candidate_state())?;
            Ok(())
        }
        key @ _ if KEYMAPS.keys().any(|k| k.to_lowercase() == key) => {
            KEYMAPS.get(key).unwrap()(state, buf)
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

    for k in KEYMAPS.keys().chain(PASSTHROUGH_KEYMAPS.keys()) {
        buf.set_keymap(
            api::types::Mode::Insert,
            &k,
            "",
            &SetKeymapOpts::builder()
                .noremap(true)
                .silent(true)
                .callback(move |_| handle_special_key(&k, &api::get_current_buf()))
                .build(),
        )?;
    }

    Ok(())
}
