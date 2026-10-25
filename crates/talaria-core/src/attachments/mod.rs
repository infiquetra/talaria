//! Mentions, paste labels and attachment chips. Pure: no files, no clock, no terminal.

pub mod chips;
pub mod drop;
pub mod expand;
pub mod mention;
pub mod paste;
pub mod sniff;

pub use chips::{Chip, ChipError, ChipId, ChipKind, ChipStore, CollapseRule, ImageState};
pub use drop::{DropCandidate, drop_candidates};
pub use expand::{Attachment, ExpandError, Expanded};
pub use mention::{
    Accept, Accepted, CompletionRequest, MentionPhase, MentionQuery, MentionState, MentionView,
    accept, count_references, filter_items, format_ref_value, mention_at, reference_notice,
};
pub use paste::{PasteRoute, route_paste};
pub use sniff::{ImageType, sniff_image, unsupported_image};

// Keep this mapping identical to history's literal rendering without importing the terminal crate.
pub(crate) fn sanitize(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        let code = u32::from(ch);
        match code {
            0x09 => result.push_str("    "),
            0x00..=0x1f => {
                if let Some(picture) = char::from_u32(0x2400 + code) {
                    result.push(picture);
                }
            }
            0x7f => result.push('\u{2421}'),
            0x80..=0x9f
            | 0x061c
            | 0x200e
            | 0x200f
            | 0x2028..=0x202e
            | 0x2066..=0x2069
            | 0xe0000..=0xe007f => result.push_str(&format!("‹U+{code:04X}›")),
            _ => result.push(ch),
        }
    }
    result
}
