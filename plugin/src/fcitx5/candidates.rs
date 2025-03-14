//! Candidate selection and UI management

use fcitx5_dbus::input_context::InputContextProxyBlocking;
use fcitx5_dbus::zbus::Result;
use nvim_oxi::{
    self as oxi,
    api::{
        self,
        types::{
            WindowBorder, WindowConfig, WindowRelativeTo, WindowStyle, WindowTitle,
            WindowTitlePosition,
        },
        Buffer, Window,
    },
    libuv::AsyncHandle,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

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
    /// Whether the window should be updated
    pub update_queue: VecDeque<UpdateType>,
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
            loop {
                match buffer.set_lines(0..buffer.line_count()?, true, lines.clone()) {
                    Err(e)
                        if e.to_string() == r#"Exception("Failed to save undo information")"# => {}
                    _ => break,
                }
            }
        }

        Ok(())
    }

    // Rather than directly showing/hiding, mark for update
    pub fn mark_for_show(&mut self) {
        if !self.candidates.is_empty() {
            self.update_queue.push_back(UpdateType::Show);
            self.is_visible = true;
        }
    }

    pub fn mark_for_hide(&mut self) {
        self.update_queue.push_back(UpdateType::Hide);
        self.is_visible = false;
    }

    pub fn mark_for_insert(&mut self, text: String) {
        self.update_queue.push_back(UpdateType::Insert(text));
    }

    pub fn mark_for_update(&mut self) {
        if self.is_visible {
            self.update_queue.push_back(UpdateType::UpdateContent);
        }
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
                                    let _ = trigger.send();
                                    // eprintln!("sending events: {:?}", &guard.update_queue);
                                }
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

    Ok(())
}
