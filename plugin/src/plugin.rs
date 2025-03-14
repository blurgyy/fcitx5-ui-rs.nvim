//! Plugin state management

use fcitx5_dbus::controller::ControllerProxyBlocking;
use fcitx5_dbus::input_context::InputContextProxyBlocking;
use std::sync::{Arc, Mutex};

use crate::fcitx5::candidates::CandidateState;

// Structure to hold the plugin state
pub struct Fcitx5Plugin {
    pub controller: Option<ControllerProxyBlocking<'static>>,
    pub ctx: Option<InputContextProxyBlocking<'static>>,
    pub augroup_id: Option<u32>,
    pub initialized: bool,
    pub candidate_state: Arc<Mutex<CandidateState>>,
    pub candidate_window: Arc<Mutex<Option<nvim_oxi::api::Window>>>,
}

impl Fcitx5Plugin {
    pub fn new() -> Self {
        Self {
            controller: None,
            ctx: None,
            augroup_id: None,
            initialized: false,
            candidate_state: Arc::new(Mutex::new(CandidateState::new())),
            candidate_window: Arc::new(Mutex::new(None)),
        }
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
