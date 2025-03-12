//! Autocommand setup for Neovim

use nvim_oxi::{
    self as oxi,
    api::{
        self,
        opts::{CreateAugroupOpts, CreateAutocmdOpts},
        Buffer,
    },
    Error as OxiError,
};

use crate::fcitx5::commands::{set_im_en, set_im_zh};
use crate::plugin::Fcitx5Plugin;
use crate::utils::as_api_error;
use std::sync::{Arc, Mutex};

/// Setup autocommands for input method switching
pub fn setup_autocommands(state: Arc<Mutex<Fcitx5Plugin>>) -> oxi::Result<()> {
    let mut state_guard = state.lock().unwrap();

    // If already initialized, clean up first
    if let Some(augroup_id) = state_guard.augroup_id {
        api::del_augroup_by_id(augroup_id)?;
    }

    // Create augroup for our autocommands
    let augroup_id = api::create_augroup(
        "fcitx5-ui-rs-nvim",
        &CreateAugroupOpts::builder().clear(true).build(),
    )?;
    state_guard.augroup_id = Some(augroup_id);

    // Ensure we have controller and ctx
    let controller = state_guard
        .controller
        .as_ref()
        .expect("Controller not initialized");
    let ctx = state_guard
        .ctx
        .as_ref()
        .expect("Input context not initialized");

    // Clone for use in callbacks
    let controller_clone = controller.clone();
    let ctx_clone = ctx.clone();

    let opts = CreateAutocmdOpts::builder()
        .buffer(Buffer::current())
        .group(augroup_id)
        .desc("Switch to Pinyin input method when entering insert mode")
        .callback({
            let state_ref = state.clone();
            move |_| {
                let state_guard = state_ref.lock().unwrap();
                if !state_guard.initialized {
                    return Ok(false);
                }
                set_im_zh(&controller_clone, &ctx_clone).map_err(as_api_error)?;
                Ok::<_, OxiError>(false) // NB: return false to keep this autocmd
            }
        })
        .build();
    api::create_autocmd(["InsertEnter"], &opts)?;

    let opts = CreateAutocmdOpts::builder()
        .buffer(Buffer::current())
        .group(augroup_id)
        .desc("Switch to English input method when leaving insert mode")
        .callback({
            let controller_clone = controller.clone();
            let ctx_clone = ctx.clone();
            let state_ref = state.clone();
            move |_| {
                let state_guard = state_ref.lock().unwrap();
                if !state_guard.initialized {
                    return Ok(false);
                }
                set_im_en(&controller_clone, &ctx_clone).map_err(as_api_error)?;
                Ok::<_, OxiError>(false) // NB: return false to keep this autocmd
            }
        })
        .build();
    api::create_autocmd(["InsertLeave"], &opts)?;

    let opts = CreateAutocmdOpts::builder()
        .buffer(Buffer::current())
        .group(augroup_id)
        .desc("Reset input context when leaving window or buffer")
        .callback({
            let ctx_clone = ctx.clone();
            let state_ref = state.clone();
            move |_| {
                let state_guard = state_ref.lock().unwrap();
                if !state_guard.initialized {
                    return Ok(false);
                }
                ctx_clone.reset().map_err(as_api_error)?;
                Ok::<_, OxiError>(false) // NB: return false to keep this autocmd
            }
        })
        .build();
    api::create_autocmd(["WinLeave", "BufLeave"], &opts)?;

    Ok(())
}

/// Placeholder for future InsertCharPre event setup
/// This will be implemented later for candidate selection UI
pub fn setup_insert_char_pre(_state: Arc<Mutex<Fcitx5Plugin>>) -> oxi::Result<()> {
    // This will be implemented in the future
    Ok(())
}
