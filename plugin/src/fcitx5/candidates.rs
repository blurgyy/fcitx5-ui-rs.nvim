//! Candidate selection and UI management

use fcitx5_dbus::input_context::InputContextProxyBlocking;
use fcitx5_dbus::zbus::Result;
use nvim_oxi::{
    self as oxi,
    api::{
        self,
        opts::*,
        types::{
            WindowBorder, WindowConfig, WindowRelativeTo, WindowStyle, WindowTitle,
            WindowTitlePosition,
        },
        Buffer, Window,
    },
    Error as OxiError,
};
use std::{
    ops::RangeBounds,
    sync::{Arc, Mutex},
};

/// Structure for an input method candidate
#[derive(Debug, Clone)]
pub struct Candidate {
    pub display: String,
    pub text: String,
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
    /// Window ID for the candidate window
    pub window_id: Option<Window>,
    /// Current preedit text
    pub preedit_text: String,
    /// Has previous page
    pub has_prev: bool,
    /// Has next page
    pub has_next: bool,
    /// Whether candidate window is currently visible
    pub is_visible: bool,
}

impl CandidateState {
    pub fn new() -> Self {
        Self {
            candidates: Vec::new(),
            selected_index: 0,
            buffer_id: None,
            window_id: None,
            preedit_text: String::new(),
            has_prev: false,
            has_next: false,
            is_visible: false,
        }
    }

    /// Update candidates list
    pub fn update_candidates(&mut self, candidates: Vec<Candidate>) {
        self.candidates = candidates;
        if !self.candidates.is_empty() && self.selected_index >= self.candidates.len() {
            self.selected_index = 0;
        }
    }

    /// Select next candidate
    pub fn select_next(&mut self) {
        if !self.candidates.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.candidates.len();
        }
    }

    /// Select previous candidate
    pub fn select_previous(&mut self) {
        if !self.candidates.is_empty() {
            self.selected_index = if self.selected_index > 0 {
                self.selected_index - 1
            } else {
                self.candidates.len() - 1
            };
        }
    }

    /// Get currently selected candidate if any
    pub fn get_selected_candidate(&self) -> Option<&Candidate> {
        self.candidates.get(self.selected_index)
    }

    /// Reset the candidate state
    pub fn reset(&mut self) {
        self.candidates.clear();
        self.selected_index = 0;
        self.preedit_text.clear();
        self.is_visible = false;
    }

    /// Setup the candidate window
    pub fn setup_window(&mut self) -> oxi::Result<()> {
        // Check if we already have a buffer
        if self.buffer_id.is_none() {
            // Create a new scratch buffer for candidates
            self.buffer_id = Some(api::create_buf(false, true)?);
        }

        // Make sure the buffer exists
        let buffer = self.buffer_id.as_ref().unwrap();

        // Create the floating window for candidates if needed
        if self.window_id.is_none() {
            // Create window options
            let opts = WindowConfig::builder()
                .relative(WindowRelativeTo::Cursor)
                .row(1)
                .col(0)
                .width(30)
                .height(10)
                .focusable(false)
                .border(WindowBorder::Single)
                .title(WindowTitle::SimpleString(
                    "Fcitx5 Candidates".to_owned().into(),
                ))
                .title_pos(WindowTitlePosition::Center)
                .style(WindowStyle::Minimal)
                .build();

            // Open the window with our buffer
            let mut window = api::open_win(buffer, false, &opts)?;

            // Set window options
            window.set_option("winblend", 10)?;
            window.set_option("wrap", true)?;

            self.window_id = Some(window);
        }

        Ok(())
    }

    /// Update the candidate window display
    pub fn update_display(&mut self) -> oxi::Result<()> {
        if let Some(ref mut buffer) = self.buffer_id {
            // Generate content for the candidate window
            let mut lines = Vec::new();

            // Add preedit text at the top
            if !self.preedit_text.is_empty() {
                lines.push(format!("Input: {}", self.preedit_text));
                lines.push("".to_string()); // Empty line separator
            }

            // Add candidates with proper selection indicator
            for (idx, candidate) in self.candidates.iter().enumerate() {
                let prefix = if idx == self.selected_index {
                    "> "
                } else {
                    "  "
                };
                lines.push(format!(
                    "{}{} {}",
                    prefix, candidate.display, candidate.text
                ));
            }

            // Add paging info at the bottom if needed
            if self.has_prev || self.has_next {
                lines.push("".to_string());

                let mut paging = String::new();
                if self.has_prev {
                    paging.push_str("< Prev ");
                }
                if self.has_next {
                    paging.push_str("Next >");
                }

                lines.push(paging);
            }

            // Update buffer content
            buffer.set_lines(0..buffer.line_count()?, true, lines)?;
        }

        Ok(())
    }

    /// Show the candidate window
    pub fn show(&mut self) -> oxi::Result<()> {
        if !self.is_visible && !self.candidates.is_empty() {
            self.setup_window()?;
            self.update_display()?;
            self.is_visible = true;
        }
        Ok(())
    }

    /// Hide the candidate window
    pub fn hide(&mut self) -> oxi::Result<()> {
        if let Some(window) = self.window_id.as_ref() {
            if window.is_valid() {
                window.clone().close(true)?
            }
            self.window_id = None;
            self.is_visible = false;
        }
        Ok(())
    }
}

/// Setup message receivers to listen for Fcitx5 candidate updates
pub fn setup_candidate_receivers(
    ctx: &InputContextProxyBlocking<'_>,
    candidate_state: Arc<Mutex<CandidateState>>,
) -> Result<()> {
    // Get the update signal
    let update_signal = ctx.receive_update_client_side_ui()?;

    // Spawn thread to handle update signals
    std::thread::spawn({
        let candidate_state = candidate_state.clone();
        move || {
            for signal in update_signal {
                // Try to get the signal arguments
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
                        let mut state = candidate_state.lock().unwrap();
                        state.update_candidates(candidates);
                        state.preedit_text = preedit_text;
                        state.has_prev = args.has_prev;
                        state.has_next = args.has_next;

                        // Show/hide candidate window based on whether we have candidates
                        if !state.candidates.is_empty() {
                            match state.show() {
                                Ok(_) => {}
                                Err(e) => eprintln!("Error showing candidate window: {}", e),
                            }
                            match state.update_display() {
                                Ok(_) => {}
                                Err(e) => eprintln!("Error updating candidate display: {}", e),
                            }
                        } else {
                            match state.hide() {
                                Ok(_) => {}
                                Err(e) => eprintln!("Error hiding candidate window: {}", e),
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error getting update args: {}", e);
                    }
                }
            }
        }
    });

    // Get the commit string signal
    let commit_signal = ctx.receive_commit_string()?;

    // Spawn thread to handle commit signals
    std::thread::spawn(move || {
        for signal in commit_signal {
            // Try to get the signal arguments
            match signal.args() {
                Ok(args) => {
                    // When a string is committed, hide the candidate window
                    let mut state = candidate_state.lock().unwrap();
                    state.hide().unwrap_or_else(|e| {
                        eprintln!("Error hiding candidate window: {}", e);
                    });
                    state.reset();
                }
                Err(e) => {
                    eprintln!("Error getting commit args: {}", e);
                }
            }
        }
    });

    Ok(())
}
