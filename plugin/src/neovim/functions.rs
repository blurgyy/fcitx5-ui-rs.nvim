use nvim_oxi::{
    self as oxi,
    api::{self, opts::SetKeymapOpts},
    conversion::{FromObject, ToObject},
    lua,
};
use serde::{Deserialize, Serialize};

use crate::plugin::{get_state, PLUGIN_NAME};

use super::commands::toggle_plugin;

#[derive(Clone, Deserialize, Serialize)]
pub struct PluginConfig {
    #[serde(default)]
    pub on_key: Option<String>,

    #[serde(default = "default_im_active")]
    pub im_active: String,

    #[serde(default = "default_im_inactive")]
    pub im_inactive: String,
}

fn default_im_active() -> String {
    "pinyin".to_owned()
}
fn default_im_inactive() -> String {
    "keyboard-us".to_owned()
}

impl FromObject for PluginConfig {
    fn from_object(obj: oxi::Object) -> Result<Self, oxi::conversion::Error> {
        Self::deserialize(oxi::serde::Deserializer::new(obj)).map_err(Into::into)
    }
}

impl ToObject for PluginConfig {
    fn to_object(self) -> Result<oxi::Object, oxi::conversion::Error> {
        self.serialize(oxi::serde::Serializer::new())
            .map_err(Into::into)
    }
}

impl lua::Poppable for PluginConfig {
    unsafe fn pop(lstate: *mut lua::ffi::lua_State) -> Result<Self, lua::Error> {
        let obj = oxi::Object::pop(lstate)?;
        Self::from_object(obj).map_err(lua::Error::pop_error_from_err::<Self, _>)
    }
}

impl lua::Pushable for PluginConfig {
    unsafe fn push(
        self,
        lstate: *mut lua::ffi::lua_State,
    ) -> Result<std::ffi::c_int, lua::Error> {
        self.to_object()
            .map_err(lua::Error::push_error_from_err::<Self, _>)?
            .push(lstate)
    }
}

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
