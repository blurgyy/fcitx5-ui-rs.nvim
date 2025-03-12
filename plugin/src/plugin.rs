//! Plugin state management

use fcitx5_dbus::controller::ControllerProxyBlocking;
use fcitx5_dbus::input_context::InputContextProxyBlocking;
use nvim_oxi::api::Buffer;
use std::sync::{Arc, Mutex};

// Structure to hold the plugin state
pub struct Fcitx5Plugin {
    pub controller: Option<ControllerProxyBlocking<'static>>,
    pub ctx: Option<InputContextProxyBlocking<'static>>,
    pub augroup_id: Option<u32>,
    pub initialized: bool,
}

impl Fcitx5Plugin {
    pub fn new() -> Self {
        Self {
            controller: None,
            ctx: None,
            augroup_id: None,
            initialized: false,
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub fn get_controller(&self) -> Option<&ControllerProxyBlocking<'static>> {
        self.controller.as_ref()
    }

    pub fn get_ctx(&self) -> Option<&InputContextProxyBlocking<'static>> {
        self.ctx.as_ref()
    }
}

// Use lazy_static for thread-safe initialization
lazy_static::lazy_static! {
    static ref PLUGIN_STATE: Arc<Mutex<Fcitx5Plugin>> = Arc::new(Mutex::new(Fcitx5Plugin::new()));
}

// Get a reference to the global state
pub fn get_state() -> Arc<Mutex<Fcitx5Plugin>> {
    PLUGIN_STATE.clone()
}
