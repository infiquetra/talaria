//! Slash commands: parse a line, hold the catalogue, and decide what a reply means.
//!
//! Issue #223. The functions below are stubs. They compile so the tests can call them.

pub mod catalog;
pub mod local;
pub mod outcome;
pub mod palette;
pub mod parse;
pub mod state;

pub use catalog::{
    ArgumentMode, CatalogEntry, CatalogResult, CommandCatalog, CommandSource, SkillOrigin,
};
pub use local::{LocalAction, LocalCommand, LocalCommandTable};
pub use outcome::{CommandOutcome, ErrorView, describe_error, interpret_result};
pub use palette::{
    CompleteResult, PaletteContext, PaletteEffect, PaletteInput, PaletteRow, PaletteStage,
    PaletteState, RowKind, update as palette_update,
};
pub use parse::{SlashLine, looks_like_slash_command, parse_slash_line};
pub use state::{
    CatalogStatus, CommandsEffect, CommandsInput, CommandsState, update as state_update,
};
