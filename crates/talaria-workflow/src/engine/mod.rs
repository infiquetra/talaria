//! Sandboxed JavaScript workflows with deterministic host-result delivery.

mod meta;
mod runtime;
mod scan;
mod syntax;
mod types;

pub use meta::parse_script;
pub use runtime::{run, spawn};
pub use syntax::{SyntaxIssue, check_syntax};
pub use types::*;
