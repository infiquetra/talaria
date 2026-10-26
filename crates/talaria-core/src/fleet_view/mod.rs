//! Pure fleet overlay state: rows, filters, peek, reply and stop. No input, no clock, no screen.

pub mod detail;
pub mod state;

pub use detail::{glyph, matches, row_detail, title_or_untitled};
pub use state::{
    FleetAction, FleetViewEffect, FleetViewInput, FleetViewState, Focus, PeekStatus, ProfileFilter,
    ReplyOutcome, RowFlags, StopOutcome, VisibleGroup, update,
};
