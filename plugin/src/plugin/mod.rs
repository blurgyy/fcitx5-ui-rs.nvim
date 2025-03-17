//! Plugin state management
pub mod config;

use fcitx5_dbus::controller::ControllerProxyBlocking;
use fcitx5_dbus::input_context::InputContextProxyBlocking;
use fcitx5_dbus::zbus::Result;
use nvim_oxi::{
    self as oxi,
    api::{self, opts::SetKeymapOpts, types::KeymapInfos, Buffer},
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::fcitx5::candidates::CandidateState;
use crate::utils::as_api_error;

use config::PluginConfig;

type BufferOriginalKeymaps = HashMap<String, KeymapInfos>;

// Structure to hold the plugin state
pub struct Fcitx5Plugin {
    pub config: Option<PluginConfig>,
    pub controller: HashMap<i32, ControllerProxyBlocking<'static>>,
    /// Per-buffer input context
    pub ctx: HashMap<i32, InputContextProxyBlocking<'static>>,
    /// Per-buffer augroup_id
    pub augroup_id: HashMap<i32, u32>,
    pub candidate_state: Arc<Mutex<CandidateState>>,
    pub candidate_window: Arc<Mutex<Option<nvim_oxi::api::Window>>>,
    pub existing_keymaps_insert: HashMap<i32, BufferOriginalKeymaps>,
}

impl Fcitx5Plugin {
    pub fn new() -> Self {
        Self {
            config: None,
            controller: HashMap::new(),
            ctx: HashMap::new(),
            augroup_id: HashMap::new(),
            candidate_state: Arc::new(Mutex::new(CandidateState::new())),
            candidate_window: Arc::new(Mutex::new(None)),
            existing_keymaps_insert: HashMap::new(),
        }
    }

    pub fn initialized(&self, buf: &Buffer) -> bool {
        self.controller.get(&buf.handle()).is_some()
            && self.ctx.get(&buf.handle()).is_some()
    }

    pub fn reset_im_ctx(&self, buf: &Buffer) -> Result<()> {
        if let Some(ctx) = self.ctx.get(&buf.handle()) {
            ctx.reset()?;
        }
        Ok(())
    }

    pub fn get_im(&self, buf: &Buffer) -> oxi::Result<String> {
        if self.initialized(buf) {
            self.controller
                .get(&buf.handle())
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

    pub fn toggle_im(&self, buf: &Buffer) -> Result<()> {
        if let (Some(controller), Some(ctx)) = (
            self.controller.get(&buf.handle()),
            self.ctx.get(&buf.handle()),
        ) {
            ctx.focus_in()?;
            controller.toggle()?;
        }
        Ok(())
    }

    pub fn activate_im(&self, buf: &Buffer) -> Result<()> {
        if let (Some(controller), Some(ctx), Some(config)) = (
            self.controller.get(&buf.handle()),
            self.ctx.get(&buf.handle()),
            self.config.as_ref(),
        ) {
            ctx.focus_in()?;
            if controller.current_input_method()? != *config.im_active {
                controller.toggle()?;
            }
        }
        Ok(())
    }

    pub fn deactivate_im(&self, buf: &Buffer) -> Result<()> {
        if let (Some(controller), Some(ctx), Some(config)) = (
            self.controller.get(&buf.handle()),
            self.ctx.get(&buf.handle()),
            self.config.as_ref(),
        ) {
            ctx.focus_in()?;
            if controller.current_input_method()? != *config.im_inactive {
                controller.toggle()?;
            }
        }
        Ok(())
    }

    pub fn store_existing_keymaps(&mut self, buf: &Buffer) -> oxi::Result<()> {
        for km in buf.get_keymap(api::types::Mode::Insert)? {
            match km.lhs.to_lowercase().as_ref() {
                key @ ("<esc>" | "<cr>" | "<bs>" | "<left>" | "<right>") => {
                    let new_buf_keymaps = if let Some(mut buf_keymaps) =
                        self.existing_keymaps_insert.remove(&buf.handle())
                    {
                        buf_keymaps.insert(key.to_owned(), km);
                        buf_keymaps
                    } else {
                        HashMap::from_iter([(key.to_owned(), km)])
                    };
                    self.existing_keymaps_insert
                        .insert(buf.handle(), new_buf_keymaps);
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn restore_existing_keymaps(&mut self, buf: &Buffer) -> oxi::Result<()> {
        if let Some(mut buf_keymaps) =
            self.existing_keymaps_insert.remove(&buf.handle())
        {
            for km in buf_keymaps.values_mut() {
                let mut buf = buf.clone();
                buf.set_keymap(
                    api::types::Mode::Insert,
                    &km.lhs,
                    &km.rhs.as_ref().unwrap_or(&"".to_string()),
                    {
                        let mut builder = SetKeymapOpts::builder();
                        let mut builder = builder
                            .expr(km.expr)
                            .nowait(km.nowait)
                            .noremap(km.noremap)
                            .silent(km.silent);
                        if let Some(callback) = km.callback.take() {
                            builder = builder.callback(callback);
                        }
                        &builder.build()
                    },
                )?;
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
