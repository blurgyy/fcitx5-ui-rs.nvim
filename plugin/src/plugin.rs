//! Plugin state management

use fcitx5_dbus::controller::ControllerProxyBlocking;
use fcitx5_dbus::input_context::InputContextProxyBlocking;
use fcitx5_dbus::zbus::Result;
use nvim_oxi as oxi;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::fcitx5::candidates::CandidateState;
use crate::neovim::functions::PluginConfig;
use crate::utils::as_api_error;

// Structure to hold the plugin state
pub struct Fcitx5Plugin {
    pub config: Option<PluginConfig>,
    pub controller: Option<ControllerProxyBlocking<'static>>,
    /// Per-buffer input context
    pub ctx: HashMap<i32, InputContextProxyBlocking<'static>>,
    /// Per-buffer augroup_id
    pub augroup_id: HashMap<i32, u32>,
    pub candidate_state: Arc<Mutex<CandidateState>>,
    pub candidate_window: Arc<Mutex<Option<nvim_oxi::api::Window>>>,
}

impl Fcitx5Plugin {
    pub fn new() -> Self {
        Self {
            config: None,
            controller: None,
            ctx: HashMap::new(),
            augroup_id: HashMap::new(),
            candidate_state: Arc::new(Mutex::new(CandidateState::new())),
            candidate_window: Arc::new(Mutex::new(None)),
        }
    }

    pub fn initialized(&self, bufnr: &i32) -> bool {
        self.controller.is_some() && self.ctx.get(bufnr).is_some()
    }

    pub fn reset_im_ctx(&self, bufnr: &i32) -> Result<()> {
        if let Some(ctx) = self.ctx.get(bufnr) {
            ctx.reset()?;
        }
        Ok(())
    }

    pub fn get_im(&self, bufnr: &i32) -> oxi::Result<String> {
        if self.initialized(bufnr) {
            self.controller
                .as_ref()
                .unwrap()
                .current_input_method()
                .map_err(|e| as_api_error(e).into())
        } else {
            Err(oxi::api::Error::Other(format!(
                "{PLUGIN_NAME}: could not get current input method (not initialized)",
            ))
            .into())
        }
    }

    pub fn toggle_im(&self, bufnr: &i32) -> Result<()> {
        if let (Some(controller), Some(ctx)) =
            (self.controller.as_ref(), self.ctx.get(bufnr))
        {
            ctx.focus_in()?;
            controller.toggle()?;
        }
        Ok(())
    }

    pub fn activate_im(&self, bufnr: &i32) -> Result<()> {
        if let (Some(controller), Some(ctx), Some(config)) = (
            self.controller.as_ref(),
            self.ctx.get(bufnr),
            self.config.as_ref(),
        ) {
            ctx.focus_in()?;
            if controller.current_input_method()? != *config.im_active {
                controller.toggle()?;
            }
        }
        Ok(())
    }

    pub fn deactivate_im(&self, bufnr: &i32) -> Result<()> {
        if let (Some(controller), Some(ctx), Some(config)) = (
            self.controller.as_ref(),
            self.ctx.get(bufnr),
            self.config.as_ref(),
        ) {
            ctx.focus_in()?;
            if controller.current_input_method()? != *config.im_inactive {
                controller.toggle()?;
            }
        }
        Ok(())
    }
}

pub static PLUGIN_NAME: &'static str = "fcitx5-ui-rs.nvim";

// Use lazy_static for thread-safe initialization
lazy_static::lazy_static! {
    static ref PLUGIN_STATE: Arc<Mutex<Fcitx5Plugin>> = Arc::new(Mutex::new(Fcitx5Plugin::new()));
}

// Get a reference to the global state
pub fn get_state() -> Arc<Mutex<Fcitx5Plugin>> {
    PLUGIN_STATE.clone()
}

// Get a reference to just the candidate state
pub fn get_candidate_state() -> Arc<Mutex<CandidateState>> {
    let state = get_state();
    let state_guard = state.lock().unwrap();
    state_guard.candidate_state.clone()
}

// Get a reference to just the candidate Option<Window>
pub fn get_candidate_window() -> Arc<Mutex<Option<nvim_oxi::api::Window>>> {
    let state = get_state();
    let state_guard = state.lock().unwrap();
    state_guard.candidate_window.clone()
}
