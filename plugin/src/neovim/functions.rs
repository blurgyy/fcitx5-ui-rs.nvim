use nvim_oxi::{
    self as oxi,
    api::{
        self,
        opts::{EchoOpts, SetKeymapOpts},
    },
};

use crate::plugin::{
    config::PluginConfig, get_im_window_state, get_state, PLUGIN_NAME,
};

use super::commands::toggle_plugin;

pub fn setup(config: PluginConfig) -> bool {
    // set config into plugin state
    let state = get_state();
    let mut state_guard = state.lock().unwrap();
    state_guard.config = Some(config.clone());
    // drop to not block
    drop(state_guard);

    // Create the global candidate buffer if it doesn't exist.
    // This is a "safe" context for api::create_buf.
    let im_window_state = get_im_window_state();
    let mut im_state_guard = im_window_state.lock().unwrap();
    if im_state_guard.buffer.is_none() {
        match api::create_buf(false, true) {
            Ok(buf) => {
                im_state_guard.buffer = Some(buf);
            }
            Err(e) => {
                let _ = api::echo(
                    vec![(
                        format!(
                            "{PLUGIN_NAME}: Failed to create candidate buffer: {e}"
                        )
                        .as_str(),
                        Some("ErrorMsg"),
                    )],
                    true,
                    &EchoOpts::default(),
                );
                return false; // Indicate setup failure
            }
        }
    }
    drop(im_state_guard);

    // Initialize the plugin's commands
    if let Err(e) = crate::neovim::commands::register_commands() {
        oxi::print!("{PLUGIN_NAME}: Could not setup commands: {e}");
        return false;
    }

    if let Some(on_key) = config.on_key {
        if let Err(e) = api::set_keymap(
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
            let _ = api::echo(
                vec![(format!("{PLUGIN_NAME}: Could not setup default enable keymap for '{on_key}': {e}").as_str(), Some("WarningMsg"))],
                true,
                &EchoOpts::default(),
            );
            return false;
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
