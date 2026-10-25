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
