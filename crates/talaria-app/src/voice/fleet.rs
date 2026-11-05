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
    if !commit.entered {
        return;
    }
    match &commit.delivery {
        FleetVoiceDelivery::Prompt => {
            if let Some(agent_id) = commit.key.profile.strip_prefix("acp:") {
                if let Some(acp) = acp {
                    let key = talaria_core::fleet::acp::AcpSessionKey {
                        agent_id: agent_id.to_owned(),
                        session_id: commit.key.session_id.clone(),
                    };
                    let _ = acp.prompt(&key, &commit.text).await;
                }
            } else if let Some(herdr) = herdr {
                let _ = herdr.prompt(&commit.key, &commit.text).await;
            }
        }
        FleetVoiceDelivery::Answer => {
            if let Some(herdr) = herdr {
                let _ = herdr.answer(&commit.key, &commit.text).await;
            }
        }
        FleetVoiceDelivery::Permission { responder, options } => {
            if commit.key.profile != format!("acp:{}", responder.key.agent_id)
                || commit.key.session_id != responder.key.session_id
            {
                return;
            }
            let normalize = |text: &str| {
                text.split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase()
            };
            let spoken = normalize(&commit.text);
            let mut matches = options
                .iter()
                .filter(|option| normalize(&option.name) == spoken);
            if let (Some(option), None, Some(acp)) = (matches.next(), matches.next(), acp) {
                let _ = acp.answer_permission(responder, &option.id).await;
            }
        }
    }
}
