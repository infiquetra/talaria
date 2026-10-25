//! Switching the foreground session, branching and writing exports (#231).

use talaria_core::directory::rows::RowKey;

use super::app::AppState;
use super::directory::ExportTarget;

/// How a switch ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitchOutcome {
    Switched { notice: String },
    AlreadyHere,
    Failed { text: String },
}

/// Open the target first; only after that lets go of the old session.
/// The one way to change the foreground session.
pub async fn switch_foreground(_app: &mut AppState, _target: RowKey) -> SwitchOutcome {
    unimplemented!("#231")
}

/// How a branch ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BranchOutcome {
    Branched { notice: String },
    Refused { text: String },
}

/// Branch the foreground session on the foreground link, then switch into
/// the child.
pub async fn branch_foreground(_app: &mut AppState, _title: Option<String>) -> BranchOutcome {
    unimplemented!("#231")
}

/// Write export content to its target without ever overwriting. Returns the
/// path written and its size.
pub fn write_export_file(
    _target: &ExportTarget,
    _name: &str,
    _content: &str,
) -> Result<(std::path::PathBuf, u64), String> {
    unimplemented!("#231")
}
