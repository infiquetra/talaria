//! The standard-input document sent to the external command.

use super::segments::StatusInputs;

/// The eight fields, in order.
pub const DOCUMENT_FIELDS: [&str; 8] = ["", "", "", "", "", "", "", ""];

#[derive(serde::Serialize)]
pub struct StatusDocument {
    _private: (),
}

impl StatusDocument {
    /// `None` without a session.
    pub fn from_inputs(inputs: &StatusInputs) -> Option<StatusDocument> {
        let _ = inputs;
        todo!("status")
    }

    /// Compact JSON and a newline.
    pub fn encode(&self) -> Vec<u8> {
        todo!("status")
    }
}
