//! Per-session pick state. Issue 230.

use super::picker::{ModelsEffect, ModelsInput, PickerView};
use super::saved::SavedModels;
use crate::mutation::{Confirmation, Fences};

/// The picker, the per-session picks, and the saved file for this launch.
pub struct ModelsState {
    launch_id: String,
    saved: Result<SavedModels, String>,
    list_height: usize,
    /// The model named on the confirmation card's keys line.
    confirm_keep: Option<String>,
}

impl ModelsState {
    pub fn new(launch_id: &str, saved: Result<SavedModels, String>) -> Self {
        Self {
            launch_id: launch_id.to_string(),
            saved,
            list_height: 10,
            confirm_keep: None,
        }
    }

    /// The model the confirmation card offers to keep.
    pub fn confirm_keep(&self) -> Option<&str> {
        self.confirm_keep.as_deref()
    }

    pub fn launch_id(&self) -> &str {
        &self.launch_id
    }

    /// The reason the saved file was refused, when this launch is not using it.
    pub fn saved_error(&self) -> Option<&str> {
        self.saved.as_ref().err().map(String::as_str)
    }

    /// How many list rows PageUp and PageDown move. The default is 10.
    pub fn set_list_height(&mut self, rows: usize) {
        self.list_height = rows.max(3);
    }

    pub fn list_height(&self) -> usize {
        self.list_height
    }

    pub fn update(&mut self, input: ModelsInput) -> Vec<ModelsEffect> {
        let _ = input;
        todo!()
    }

    pub fn view(&self) -> Option<PickerView<'_>> {
        let _ = self;
        todo!()
    }

    /// The foreground session's confirmation, when one is waiting to be drawn.
    pub fn confirmation(&self) -> Option<&Confirmation> {
        let _ = self;
        todo!()
    }

    /// Whether this session is waiting for Hermes to apply a stored model pick.
    pub fn is_deferred(&self, session_id: &str) -> bool {
        let _ = (self, session_id);
        todo!()
    }

    pub fn fences(&self, session_id: &str) -> Fences {
        let _ = (self, session_id);
        todo!()
    }
}
