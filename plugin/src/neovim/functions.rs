use nvim_oxi::{
    self as oxi,
    api::{self, opts::SetKeymapOpts},
};

use crate::plugin::{config::PluginConfig, get_state, PLUGIN_NAME};

use super::commands::toggle_plugin;

pub fn setup(config: PluginConfig) -> bool {
    // set config into plugin state
    let state = get_state();
    let mut state_guard = state.lock().unwrap();
    state_guard.config = Some(config.clone());
    // drop to not block
    drop(state_guard);

    // Initialize the plugin's commands
    match crate::neovim::commands::register_commands() {
        Err(e) => {
            oxi::print!("{PLUGIN_NAME}: Could not setup commands: {e}");
            return false;
        }
        Ok(()) => {}
    }

    if let Some(on_key) = config.on_key {
        match api::set_keymap(
            api::types::Mode::Normal,
            &on_key,
            "",
            &SetKeymapOpts::builder()
                .noremap(true)
                .silent(true)
                .callback(move |_| toggle_plugin(get_state(), &api::get_current_buf()))
                .build(),
        )
        .and_then(|_| {
            api::set_keymap(
                api::types::Mode::Insert,
                &on_key,
                "",
                &SetKeymapOpts::builder()
                    .noremap(true)
                    .silent(true)
                    .callback(move |_| {
                        toggle_plugin(get_state(), &api::get_current_buf())
                    })
                    .build(),
            )
        }) {
            Err(e) => {
                oxi::print!(
                    "{PLUGIN_NAME}: Could not setup default enable keymap for '{on_key}': {e}"
                );
                return false;
            }
            Ok(()) => {}
        }
    }

    true
}

// must accept 1 parameter, use `()` to let the exported lua function take no parameter
pub fn get_im(_: ()) -> oxi::String {
    let state = get_state();
    let state_guard = state.lock().unwrap();
    if let Ok(im) = state_guard.get_im(&api::get_current_buf()) {
        im.into()
    } else {
        "".into()
    }
}
