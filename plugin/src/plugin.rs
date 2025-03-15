//! Plugin state management

use fcitx5_dbus::controller::ControllerProxyBlocking;
use fcitx5_dbus::input_context::InputContextProxyBlocking;
use fcitx5_dbus::zbus::Result;
use nvim_oxi as oxi;
use std::sync::{Arc, Mutex};

use crate::fcitx5::candidates::CandidateState;
use crate::utils::as_api_error;

// Structure to hold the plugin state
pub struct Fcitx5Plugin {
    pub controller: Option<ControllerProxyBlocking<'static>>,
    pub ctx: Option<InputContextProxyBlocking<'static>>,
    pub augroup_id: Option<u32>,
    pub candidate_state: Arc<Mutex<CandidateState>>,
    pub candidate_window: Arc<Mutex<Option<nvim_oxi::api::Window>>>,
    pub im_activated: Option<String>,
    pub im_deactivated: Option<String>,
}

impl Fcitx5Plugin {
    pub fn new() -> Self {
        Self {
            controller: None,
            ctx: None,
            augroup_id: None,
            candidate_state: Arc::new(Mutex::new(CandidateState::new())),
            candidate_window: Arc::new(Mutex::new(None)),
            im_activated: None,
            im_deactivated: None,
        }
    }

    pub fn initialized(&self) -> bool {
        self.controller.is_some() && self.ctx.is_some()
    }

    pub fn reset_im_ctx(&self) -> Result<()> {
        if let Some(ctx) = self.ctx.as_ref() {
            ctx.reset()?;
        }
        Ok(())
    }

    pub fn get_im(&self) -> oxi::Result<String> {
        if self.initialized() {
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

    pub fn toggle_im(&self) -> Result<()> {
        if let (Some(controller), Some(ctx)) =
            (self.controller.as_ref(), self.ctx.as_ref())
        {
            ctx.focus_in()?;
            controller.toggle()?;
        }
        Ok(())
    }

    pub fn activate_im(&self) -> Result<()> {
        if let (Some(controller), Some(ctx), Some(im_activated)) = (
            self.controller.as_ref(),
            self.ctx.as_ref(),
            self.im_activated.as_ref(),
        ) {
            ctx.focus_in()?;
            if controller.current_input_method()? != *im_activated {
                controller.toggle()?;
            }
        }
        Ok(())
    }

    pub fn deactivate_im(&self) -> Result<()> {
        if let (Some(controller), Some(ctx), Some(im_deactivated)) = (
            self.controller.as_ref(),
            self.ctx.as_ref(),
            self.im_deactivated.as_ref(),
        ) {
            ctx.focus_in()?;
            if controller.current_input_method()? != *im_deactivated {
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
