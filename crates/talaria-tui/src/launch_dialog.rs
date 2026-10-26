//! The background-launch dialog. It speaks in its own types.
//!
//! Step 0 places the box and draws nothing, so a frame test sees an empty buffer.

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::keys::KeyChord;

/// Which launch the dialog is collecting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogMode {
    /// A session in the current folder.
    Plain,
    /// A session in a new worktree.
    Worktree,
}

/// Values the dialog opens with.
pub struct DialogDefaults {
    /// Profiles the link can launch in, in display order.
    pub profiles: Vec<String>,
    /// The profile the cursor starts on.
    pub profile: String,
    /// Shown when the link can reach only one profile.
    pub profile_note: Option<String>,
    /// The folder field's starting text.
    pub folder: PathBuf,
    /// A prompt kept from the last time the dialog closed.
    pub draft: Option<String>,
    /// Shown in the model field while it is empty.
    pub model_placeholder: String,
    /// Turns a prompt into the live worktree-name preview.
    pub slug: fn(&str) -> String,
}

/// What the operator submitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogSubmit {
    /// The prompt, as typed.
    pub prompt: String,
    /// The profile the cursor was on.
    pub profile: String,
    /// The folder field, as typed. `~` is not expanded here.
    pub folder: String,
    /// The model field, or `None` while it was left empty.
    pub model: Option<String>,
    /// Whether this was the worktree dialog.
    pub worktree: bool,
    /// The name field once the operator has edited it.
    pub worktree_name: Option<String>,
}

/// What a key did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogAction {
    /// The key changed nothing the caller has to act on.
    None,
    /// The operator asked to launch.
    Launch(DialogSubmit),
    /// The operator closed the dialog. `draft` is the prompt to keep.
    Close { draft: String },
}

/// The dialog. Step 0 remembers the mode so [`Self::area`] can size the box.
pub struct LaunchDialog {
    mode: DialogMode,
    defaults: DialogDefaults,
}

impl LaunchDialog {
    /// Open the dialog. Nothing is drawn until [`Self::draw`].
    #[must_use]
    pub fn open(mode: DialogMode, defaults: DialogDefaults) -> Self {
        Self { mode, defaults }
    }

    /// Where the box goes on a screen of this size.
    ///
    /// Width is `min(screen width - 4, 120)`, centred. The top edge is two rows down.
    /// Height is 4 plus the prompt lines (one, in this stub) plus the three fields
    /// below the prompt, plus one more row in worktree mode.
    #[must_use]
    pub fn area(&self, screen: Rect) -> Rect {
        let width = screen.width.saturating_sub(4).min(120);
        let x = screen
            .x
            .saturating_add(screen.width.saturating_sub(width) / 2);
        let extra = match self.mode {
            DialogMode::Worktree => 1,
            DialogMode::Plain => 0,
        };
        let height = 4 + 1 + 3 + extra;
        Rect {
            x,
            y: screen.y.saturating_add(2),
            width,
            height,
        }
    }

    /// Apply one key. Step 0 does nothing.
    pub fn handle_key(&mut self, key: KeyChord) -> DialogAction {
        let _ = (key, &self.defaults.profile);
        DialogAction::None
    }

    /// Insert pasted text. Step 0 drops it.
    pub fn handle_paste(&mut self, text: &str) {
        let _ = text;
    }

    /// Show a stage line. Step 0 keeps the fields as they were.
    pub fn show_progress(&mut self, text: &str) {
        let _ = text;
    }

    /// Show a launch failure. Step 0 keeps the fields as they were.
    pub fn show_error(&mut self, what: &str, next_step: &str) {
        let _ = (what, next_step);
    }

    /// Show a validation line. Step 0 keeps the fields as they were.
    pub fn show_invalid(&mut self, text: &str) {
        let _ = text;
    }

    /// Draw the box. Step 0 leaves `buf` untouched.
    pub fn draw(&self, area: Rect, buf: &mut Buffer) {
        let _ = (area, buf, self.mode, &self.defaults.folder);
    }
}
