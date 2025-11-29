//! Plugin state management
pub mod config;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use fcitx5_dbus::controller::ControllerProxyBlocking;
use fcitx5_dbus::input_context::InputContextProxyBlocking;
use fcitx5_dbus::utils::key_event::{
    KeyState as Fcitx5KeyState, KeyVal as Fcitx5KeyVal,
};
use fcitx5_dbus::zbus::Result;
use nvim_oxi::{
    self as oxi,
    api::{self, types::KeymapInfos, Buffer},
};

use crate::{
    fcitx5::candidates::IMWindowState,
    lock_logged,
    neovim::commands::process_im_window_updates,
    utils::{do_feedkeys_noremap, CURSOR_INDICATOR},
};
use crate::{ignore_dbus_no_interface_error, utils::as_api_error};

use config::PluginConfig;

type BufferOriginalKeymaps = HashMap<String, KeymapInfos>;

lazy_static::lazy_static! {
    pub(crate) static ref KEYMAPS: HashMap<String, Box<dyn Fn(Arc<Mutex<Fcitx5Plugin>>, &Buffer) -> oxi::Result<()> + Send + Sync>> = {
        let mut map: HashMap<String, Box<dyn Fn(Arc<Mutex<Fcitx5Plugin>>, &Buffer) -> oxi::Result<()> + Send + Sync + 'static>> = HashMap::new();

        map.insert(
             "<cr>".to_owned(),
             Box::new(move |state: Arc<Mutex<Fcitx5Plugin>>, buf: &Buffer| {
                 let state_guard = lock_logged!(state, "PLUGIN_STATE");
                 let im_window_state = state_guard.im_window_state.clone();
                 let mut im_window_guard = lock_logged!(im_window_state, "IMWindowState");

                 if im_window_guard.is_showing_current_im() {
                     do_feedkeys_noremap("<CR>")?;
                     return Ok(());
                 }
                 let insert_text = im_window_guard
                     .preedit_text
                     .replace([' ', CURSOR_INDICATOR], "")
                     .clone();
                 im_window_guard.mark_for_insert(insert_text);
                 ignore_dbus_no_interface_error!(state_guard.reset_im_ctx(buf));
                 drop(im_window_guard);
                 oxi::schedule(move |_| process_im_window_updates(im_window_state.clone()));
                 Ok(())
             })
         );

        map.insert(
            "<esc>".to_owned(),
            Box::new(move |state: Arc<Mutex<Fcitx5Plugin>>, _buf: &Buffer| {
                let state_guard = lock_logged!(state, "PLUGIN_STATE");
                ignore_dbus_no_interface_error!(state_guard.reset_im_ctx(_buf));
                let im_window_state = state_guard.im_window_state.clone();
                let im_window_guard = lock_logged!(im_window_state, "IMWindowState");

                if im_window_guard.is_showing_current_im() {
                    do_feedkeys_noremap("<Esc>")?;
                    return Ok(());
                }
                drop(im_window_guard);
                oxi::schedule(move |_| process_im_window_updates(im_window_state.clone()));
                Ok(())
            })
        );

        map
    };
    pub(crate) static ref PASSTHROUGH_KEYMAPS: HashMap<String, (Fcitx5KeyState, Fcitx5KeyVal)> = HashMap::from([
        ("<bs>".to_owned(), (Fcitx5KeyState::NoState, Fcitx5KeyVal::DELETE)),
        ("<c-w>".to_owned(), (Fcitx5KeyState::Ctrl, Fcitx5KeyVal::DELETE)),
        ("".to_owned(), (Fcitx5KeyState::Ctrl, Fcitx5KeyVal::DELETE)),
        ("<left>".to_owned(), (Fcitx5KeyState::NoState, Fcitx5KeyVal::LEFT)),
        ("<right>".to_owned(), (Fcitx5KeyState::NoState, Fcitx5KeyVal::RIGHT)),
        ("<c-left>".to_owned(), (Fcitx5KeyState::Ctrl, Fcitx5KeyVal::LEFT)),
        ("<c-right>".to_owned(), (Fcitx5KeyState::Ctrl, Fcitx5KeyVal::RIGHT)),
        ("<tab>".to_owned(), (Fcitx5KeyState::NoState, Fcitx5KeyVal::from_char('\u{FF09}'))),
        ("<s-tab>".to_owned(), (Fcitx5KeyState::Shift, Fcitx5KeyVal::from_char('\u{FF09}'))),
    ]);
}

// Structure to hold the plugin state
pub struct Fcitx5Plugin {
    pub config: Option<PluginConfig>,
    pub controller: HashMap<i32, ControllerProxyBlocking<'static>>,
    /// Whether a buffer has been registered with our keymaps, we will not register it multiple
    /// times.
    pub keymaps_registered: HashMap<i32, bool>,
    /// Per-buffer input context
    pub ctx: HashMap<i32, InputContextProxyBlocking<'static>>,
    /// Per-buffer augroup_id
    pub augroup_id: HashMap<i32, u32>,
    pub im_window_state: Arc<Mutex<IMWindowState>>,
    pub im_window: Arc<Mutex<Option<nvim_oxi::api::Window>>>,
    pub existing_keymaps_insert: HashMap<i32, BufferOriginalKeymaps>,
}

impl Fcitx5Plugin {
    pub fn new() -> Self {
        Self {
            config: None,
            controller: HashMap::new(),
            keymaps_registered: HashMap::new(),
            ctx: HashMap::new(),
            augroup_id: HashMap::new(),
            im_window_state: Arc::new(Mutex::new(IMWindowState::new())),
            im_window: Arc::new(Mutex::new(None)),
            existing_keymaps_insert: HashMap::new(),
        }
    }

    pub fn initialized(&self, buf: &Buffer) -> bool {
        self.controller.contains_key(&buf.handle())
            && self.ctx.contains_key(&buf.handle())
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
        if let (Some(controller), Some(ctx)) = (
            self.controller.get(&buf.handle()),
            self.ctx.get(&buf.handle()),
        ) {
            ctx.focus_in()?;
            controller.activate()?;
        }
        Ok(())
    }

    pub fn deactivate_im(&self, buf: &Buffer) -> Result<()> {
        if let (Some(controller), Some(ctx)) = (
            self.controller.get(&buf.handle()),
            self.ctx.get(&buf.handle()),
        ) {
            ctx.focus_in()?;
            controller.deactivate()?;
        }
        Ok(())
    }

    pub fn store_original_keymaps(&mut self, buf: &Buffer) -> oxi::Result<()> {
        for km in buf.get_keymap(api::types::Mode::Insert)? {
            let key = km.lhs.to_lowercase();
            if KEYMAPS
                .keys()
                .chain(PASSTHROUGH_KEYMAPS.keys())
                .any(|k| k.to_lowercase() == key)
            {
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
        }
        Ok(())
    }
}

pub static PLUGIN_NAME: &str = "fcitx5-ui-rs.nvim";

// Use lazy_static for thread-safe initialization
lazy_static::lazy_static! {
    static ref PLUGIN_STATE: Arc<Mutex<Fcitx5Plugin>> = Arc::new(Mutex::new(Fcitx5Plugin::new()));
}

// Get a reference to the global state
pub fn get_state() -> Arc<Mutex<Fcitx5Plugin>> {
    PLUGIN_STATE.clone()
}

// Get a reference to just the candidate state
pub fn get_im_window_state() -> Arc<Mutex<IMWindowState>> {
    let state = get_state();
    let state_guard = lock_logged!(state, "PLUGIN_STATE");
    state_guard.im_window_state.clone()
}

// Get a reference to just the candidate Option<Window>
pub fn get_im_window() -> Arc<Mutex<Option<nvim_oxi::api::Window>>> {
    let state = get_state();
    let state_guard = lock_logged!(state, "PLUGIN_STATE");
    state_guard.im_window.clone()
}
