use nvim_oxi::{
    self as oxi,
    conversion::{FromObject, ToObject},
    lua,
};
use serde::{Deserialize, Serialize};

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
