//! Candidate selection and UI management

use fcitx5_dbus::zbus::Result;
use fcitx5_dbus::{
    input_context::InputContextProxyBlocking,
    utils::key_event::KeyState as Fcitx5KeyState,
};
use nvim_oxi::{
    self as oxi,
    api::{
        self,
        types::{
            WindowBorder, WindowConfig, WindowRelativeTo, WindowStyle, WindowTitle,
            WindowTitlePosition,
        },
        Buffer,
    },
    libuv::AsyncHandle,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use unicode_width::UnicodeWidthStr;

use crate::plugin::get_candidate_window;

/// Structure for an input method candidate
#[derive(Debug, Clone)]
pub struct Candidate {
    pub display: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub enum UpdateType {
    Show,
    Hide,
    Insert(String),
    UpdateContent,
}

/// State for candidate selection UI
#[derive(Clone, Debug)]
pub struct CandidateState {
    /// Current input method candidates
    pub candidates: Vec<Candidate>,
    /// Index of the currently selected candidate
    pub selected_index: usize,
    /// Buffer ID for the candidate window
    pub buffer_id: Option<Buffer>,
    /// Current preedit text
    pub preedit_text: String,
    /// Has previous page
    pub has_prev: bool,
    /// Has next page
    pub has_next: bool,
    /// Whether candidate window is currently visible
    pub is_visible: bool,
    /// Whether the window should be updated
    pub update_queue: VecDeque<UpdateType>,
}

impl CandidateState {
    pub fn new() -> Self {
        Self {
            candidates: Vec::new(),
            selected_index: 0,
            buffer_id: None,
            preedit_text: String::new(),
            has_prev: false,
            has_next: false,
            is_visible: false,
            update_queue: VecDeque::new(),
        }
    }

    /// Update candidates list
    pub fn update_candidates(&mut self, candidates: &[Candidate]) {
        self.candidates = candidates.to_owned();
        if !self.candidates.is_empty() && self.selected_index >= self.candidates.len() {
            self.selected_index = 0;
        }
    }

    /// Reset the candidate state
    pub fn reset(&mut self) {
        self.candidates.clear();
        self.selected_index = 0;
        self.preedit_text.clear();
        self.is_visible = false;
    }

    /// Calculate the optimal width for the window based on content
    fn calculate_window_dimensions(&self) -> (u32, u32) {
        // Calculate width based on content
        let mut width = 30; // Start with reasonable default

        if !self.candidates.is_empty() {
            // Find the longest candidate text
            let max_candidate_len = self
                .candidates
                .iter()
                .map(|c| c.display.len() + c.text.len() + 3) // +3 for marker and space
                .max()
                .unwrap_or(0);

            // Find longest preedit text
            let preedit_len = if !self.preedit_text.is_empty() {
                self.preedit_text.len() + 4 // "⌨  " prefix
            } else {
                0
            };

            // Find max length needed
            let needed_len = std::cmp::max(max_candidate_len, preedit_len);

            // Use gradual growth function instead of direct 1:1 mapping
            // For small changes, grow slowly; for larger changes, grow more
            if needed_len > 0 {
                if needed_len <= 20 {
                    // For small text, use fixed size
                    width = 30;
                } else if needed_len <= 40 {
                    // For medium text, grow at about 80% of text growth
                    width = 30 + ((needed_len - 20) as f32 * 0.8) as u32;
                } else {
                    // For larger text, grow at about 90% rate
                    width = 46 + ((needed_len - 40) as f32 * 0.9) as u32;
                }

                // Add minimal padding for aesthetics (reduced from 4 to 2)
                width += 2;

                // Apply hysteresis to prevent small oscillations
                // Only change size if it would be at least 4 chars different
                if let Some(window) = get_candidate_window().lock().unwrap().as_ref() {
                    if window.is_valid() {
                        if let Ok(config) = window.get_config() {
                            let current_width = config.width.unwrap_or(0);
                            if (width as i32 - current_width as i32).abs() < 4 {
                                width = current_width;
                            }
                        }
                    }
                }
            }

            // Ensure width is at least 20 and at most 60 characters
            width = width.clamp(20, 60);
        }

        // Calculate height based on number of items plus headers/footers
        let base_height = if !self.preedit_text.is_empty() { 2 } else { 0 }; // Header + separator (or nothing)
        let paging_height = if self.has_prev || self.has_next { 2 } else { 0 }; // Separator + paging line
        let content_height = self.candidates.len() as u32;

        // Calculate total height - remove the extra line
        let height = (base_height + content_height + paging_height).clamp(3, 15);

        (width, height)
    }

    /// Setup the candidate window
    pub fn setup_window(&mut self) -> oxi::Result<()> {
        // Check if we already have a buffer
        if self.buffer_id.is_none() {
            // Create a new scratch buffer for candidates
            self.buffer_id = Some(api::create_buf(false, true)?);

            // Set buffer options for better appearance
            if let Some(buf) = self.buffer_id.as_mut() {
                let _ = buf.set_option("bufhidden", "hide");
                let _ = buf.set_option("filetype", "fcitx5candidates");
            }
        }

        // Calculate window dimensions once
        let (width, height) = self.calculate_window_dimensions();

        // Create the floating window for candidates if needed
        let candidate_window = get_candidate_window();
        let window_guard = candidate_window.lock().unwrap();
        if window_guard.is_none() {
            // Get buffer reference - avoid cloning repeatedly
            let buffer = match &self.buffer_id {
                Some(b) => b,
                None => return Ok(()),
            };

            // Create window options with improved appearance
            let opts = WindowConfig::builder()
                .relative(WindowRelativeTo::Cursor)
                .row(1)
                .col(0)
                .width(width)
                .height(height)
                .focusable(false)
                .border(WindowBorder::Rounded)
                .title(WindowTitle::SimpleString(
                    " Fcitx5 ".to_owned().into(), // More compact title with spacing
                ))
                .title_pos(WindowTitlePosition::Center)
                .style(WindowStyle::Minimal)
                .zindex(50) // Ensure it stays on top
                .build();

            // Drop lock before scheduling
            drop(window_guard);

            // Schedule window creation on main thread
            oxi::schedule({
                let candidate_window = candidate_window.clone();
                let buffer = buffer.clone();
                move |_| {
                    match api::open_win(&buffer, false, &opts) {
                        Ok(mut window) => {
                            // Set window options
                            let _ = window.set_option("winblend", 15);
                            let _ = window.set_option("wrap", true);
                            let _ = window.set_option("scrolloff", 0);
                            let _ = window.set_option("sidescrolloff", 0);

                            // Store window safely
                            if let Ok(mut window_guard) = candidate_window.lock() {
                                *window_guard = Some(window);
                            }
                        }
                        Err(e) => eprintln!("Failed to open window: {}", e),
                    }
                }
            });
        } else {
            // If window exists, only resize if needed
            drop(window_guard); // Release lock before scheduling

            // Schedule safe window update
            oxi::schedule({
                let candidate_window = candidate_window.clone();
                move |_| {
                    if let Ok(mut window_guard) = candidate_window.lock() {
                        if let Some(window) = window_guard.as_mut() {
                            if window.is_valid() {
                                let config = WindowConfig::builder()
                                    .width(width)
                                    .height(height)
                                    .build();
                                let _ = window.set_config(&config);
                            }
                        }
                    }
                }
            });
        }

        Ok(())
    }

    /// Update the candidate window display
    pub fn update_display(&mut self) -> oxi::Result<()> {
        let buffer = match &self.buffer_id {
            Some(buffer) => buffer.clone(),
            None => return Ok(()),
        };

        // Calculate dimensions once
        let (width, _) = self.calculate_window_dimensions();

        // Generate content for the candidate window
        let mut lines = Vec::new();

        // Add preedit text at the top with better formatting
        if !self.preedit_text.is_empty() {
            lines.push(format!("   {}", self.preedit_text)); // keyboard icon
            lines.push("─".repeat(width as usize));
        }

        // Add candidates with improved formatting
        for (idx, candidate) in self.candidates.iter().enumerate() {
            let marker = if idx == self.selected_index {
                "►"
            } else {
                " "
            };

            lines.push(format!(
                "{} {} {}",
                marker, candidate.display, candidate.text
            ));
        }

        // Add paging info at the bottom with better styling
        if self.has_prev || self.has_next {
            lines.push("─".repeat(width as usize)); // Clean separator

            // Define indicators
            let prev_part = if self.has_prev { "◄ Prev" } else { "      " };
            let next_part = if self.has_next { "Next ►" } else { "      " };

            // Calculate visual widths properly
            let prev_width = UnicodeWidthStr::width(prev_part);
            let next_width = UnicodeWidthStr::width(next_part);

            // Calculate the total width available
            let total_width = width as usize;

            // Create a line with prev on far left and next on far right
            let mut paging_line = String::with_capacity(total_width);

            // Add the prev indicator (or its placeholder)
            paging_line.push_str(prev_part);

            // Fill the middle with spaces to push "Next" to the right edge
            let spaces_needed = total_width.saturating_sub(prev_width + next_width);
            paging_line.push_str(&" ".repeat(spaces_needed));

            // Add the next indicator
            paging_line.push_str(next_part);

            // Ensure the line is exactly the right width by truncating if needed
            if UnicodeWidthStr::width(paging_line.as_str()) > total_width {
                // Truncate based on visual width, not character count
                while UnicodeWidthStr::width(paging_line.as_str()) > total_width
                    && !paging_line.is_empty()
                {
                    paging_line.pop();
                }
            }

            // Add the properly formatted paging line
            lines.push(paging_line);
        }

        // Remove the buffer update code that might be adding an extra empty line

        // Update buffer content safely
        oxi::schedule({
            let mut buffer = buffer.clone();
            let lines = lines.clone();
            move |_| {
                // Only update buffer if it's valid
                if !buffer.is_valid() {
                    return;
                }

                // Get buffer line count safely
                let line_count = match buffer.line_count() {
                    Ok(count) => count,
                    Err(_) => return,
                };

                // Update the lines, with retry on undo info failure
                let mut success = false;
                for _ in 0..3 {
                    // Try up to 3 times
                    match buffer.set_lines(0..line_count, true, lines.clone()) {
                        Ok(_) => {
                            success = true;
                            break;
                        }
                        Err(e)
                            if e.to_string()
                                == r#"Exception("Failed to save undo information")"# =>
                        {
                            // Retry after small delay
                            std::thread::sleep(std::time::Duration::from_millis(1));
                        }
                        Err(_) => break, // Different error, don't retry
                    }
                }

                if !success {
                    eprintln!(
                        "Failed to update buffer content after multiple attempts"
                    );
                }
            }
        });

        // Update buffer content safely
        oxi::schedule({
            let mut buffer = buffer.clone();
            let lines = lines.clone();
            move |_| {
                // Only update buffer if it's valid
                if !buffer.is_valid() {
                    return;
                }

                // Get buffer line count safely
                let line_count = match buffer.line_count() {
                    Ok(count) => count,
                    Err(_) => return,
                };

                // Update the lines, with retry on undo info failure
                let mut success = false;
                for _ in 0..3 {
                    // Try up to 3 times
                    match buffer.set_lines(0..line_count, true, lines.clone()) {
                        Ok(_) => {
                            success = true;
                            break;
                        }
                        Err(e)
                            if e.to_string()
                                == r#"Exception("Failed to save undo information")"# =>
                        {
                            // Retry after small delay
                            std::thread::sleep(std::time::Duration::from_millis(1));
                        }
                        Err(_) => break, // Different error, don't retry
                    }
                }

                if !success {
                    eprintln!(
                        "Failed to update buffer content after multiple attempts"
                    );
                }
            }
        });

        Ok(())
    }

    // Rather than directly showing/hiding, mark for update
    pub fn mark_for_show(&mut self) {
        if !self.candidates.is_empty() {
            self.update_queue.push_back(UpdateType::Show);
        }
    }

    pub fn mark_for_hide(&mut self) {
        self.update_queue.push_back(UpdateType::Hide);
    }

    pub fn mark_for_insert(&mut self, text: String) {
        self.update_queue.push_back(UpdateType::Insert(text));
    }

    pub fn mark_for_update(&mut self) {
        self.update_queue.push_back(UpdateType::UpdateContent);
    }

    pub fn pop_update(&mut self) -> Option<UpdateType> {
        self.update_queue.pop_front()
    }
}

/// Setup message receivers to listen for Fcitx5 candidate updates
pub fn setup_candidate_receivers(
    ctx: &InputContextProxyBlocking<'static>,
    candidate_state: Arc<Mutex<CandidateState>>,
    trigger: AsyncHandle,
) -> Result<()> {
    // Spawn thread to handle update signals
    std::thread::spawn({
        let trigger = trigger.clone();
        let update_ctx = ctx.clone();
        let candidate_state = candidate_state.clone();
        move || {
            match update_ctx.receive_update_client_side_ui() {
                Ok(update_signal) => {
                    for signal in update_signal {
                        match signal.args() {
                            Ok(args) => {
                                // Convert candidate data from Fcitx5 format
                                let mut candidates = Vec::new();
                                for (display, text) in args.candidates {
                                    candidates.push(Candidate {
                                        display: display.to_owned(),
                                        text: text.to_owned(),
                                    });
                                }

                                // Extract preedit text
                                let mut preedit_text = String::new();
                                for (text, _) in &args.preedit_strs {
                                    preedit_text.push_str(text);
                                }

                                // Update our candidate state
                                if let Ok(mut guard) = candidate_state.lock() {
                                    guard.update_candidates(&candidates);
                                    guard.preedit_text = preedit_text;
                                    guard.has_prev = args.has_prev;
                                    guard.has_next = args.has_next;

                                    // Mark for update based on whether we have candidates
                                    if !guard.candidates.is_empty() {
                                        guard.mark_for_show();
                                    } else {
                                        guard.mark_for_hide();
                                    }
                                }
                                let _ = trigger.send();
                            }
                            Err(_) => {
                                eprintln!("Error processing update signal");
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to receive update signals: {}", e);
                }
            }
        }
    });

    // Spawn thread to handle commit signals
    std::thread::spawn({
        let trigger = trigger.clone();
        let commit_ctx = ctx.clone();
        let candidate_state = candidate_state.clone();

        move || {
            match commit_ctx.receive_commit_string() {
                Ok(commit_signal) => {
                    for signal in commit_signal {
                        if let Ok(args) = signal.args() {
                            let text_to_insert = args.text.to_owned();

                            // When a string is committed, mark for hiding
                            if let Ok(mut guard) = candidate_state.lock() {
                                guard.reset();
                                guard.mark_for_hide();
                                // Insert, if anything
                                if !text_to_insert.is_empty() {
                                    guard.mark_for_insert(args.text.to_owned());
                                }
                            }
                            let _ = trigger.send();
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to receive commit signals: {}", e);
                }
            }
        }
    });

    // FIXME: this thread does not seem to do shit
    std::thread::spawn({
        let forward_ctx = ctx.clone();
        move || {
            match forward_ctx.receive_forward_key() {
                Ok(forward_signal) => {
                    for signal in forward_signal {
                        if let Ok(args) = signal.args() {
                            if args.is_release {
                                return;
                            }
                            let mut key = String::new();
                            let modifier_prefix =
                                match Fcitx5KeyState::from_bits(args.states) {
                                    Some(Fcitx5KeyState::Ctrl) => "<C-",
                                    Some(Fcitx5KeyState::Alt) => "<M-",
                                    Some(Fcitx5KeyState::Shift) => "<S-",
                                    _ => {
                                        "" // no modifier
                                    }
                                };
                            key.push_str(modifier_prefix);
                            key.push(args.sym as u8 as char);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to receive commit signals: {}", e);
                }
            }
        }
    });

    Ok(())
}
