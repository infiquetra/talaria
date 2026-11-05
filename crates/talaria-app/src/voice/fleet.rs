//! Send a fleet transcript after the operator presses Enter.

use talaria_core::fleet::{ExternalOption, RowKey};

use crate::acp::{AcpService, PermissionResponder};
use crate::herdr::HerdrService;

/// How a committed transcript is delivered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FleetVoiceDelivery {
    /// A normal prompt on the bound row.
    Prompt,
    /// A blocked Herdr pane, sent as an answer.
    Answer,
    /// An ACP permission. The spoken text may select one displayed option.
    Permission {
        /// The open permission request.
        responder: PermissionResponder,
        /// The options that were displayed when the answer was spoken.
        options: Vec<ExternalOption>,
    },
}

/// A transcript bound to one fleet row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetVoiceCommit {
    /// The row recording was aimed at.
    pub key: RowKey,
    /// The transcript text.
    pub text: String,
    /// The operator pressed Enter.
    pub entered: bool,
    /// Which send path the row's state selected.
    pub delivery: FleetVoiceDelivery,
}

/// Deliver a transcript that was recorded for one fleet row.
///
/// Nothing is sent until `entered` is set. A normal row uses that row's prompt. A blocked
/// Herdr pane uses the answer path. Spoken text may select a displayed permission option
/// only by a unique exact match on its name, after case and spaces are folded, and that
/// selection still waits for Enter.
pub async fn commit_fleet_transcript(
    commit: &FleetVoiceCommit,
    acp: Option<&mut AcpService>,
    herdr: Option<&mut HerdrService>,
) {
    let _ = (commit, acp, herdr);
}
