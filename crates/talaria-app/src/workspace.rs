//! The workspace service for #237: the local folder check and the move.
//!
//! Moves send through the shared mutation helper.

use std::io::ErrorKind;
use std::time::Duration;

use hermes_client::CallOptions;
use talaria_core::workspace::{CdEffect, CdInput, FolderCheck};

use crate::mutation::MutationClient;

/// How long the local folder check may take; a hung network mount ends here.
const FOLDER_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a move may take; Hermes runs up to three `git` commands.
const MOVE_TIMEOUT: Duration = Duration::from_secs(60);

/// Runs the `/cd` reducer's check and send effects.
pub struct WorkspaceService {
    mutations: MutationClient,
}

impl WorkspaceService {
    /// A service that sends moves through the given mutation sender.
    pub fn new(mutations: MutationClient) -> WorkspaceService {
        WorkspaceService { mutations }
    }

    /// Performs one effect and returns the input its reply feeds back, if any.
    pub async fn execute(&self, effect: CdEffect) -> Option<CdInput> {
        match effect {
            CdEffect::CheckFolder { session_id, target } => {
                let check = check_folder(&target).await;
                Some(CdInput::Checked {
                    session_id,
                    target,
                    check,
                })
            }
            CdEffect::Send(request) => {
                let options = CallOptions {
                    timeout: Some(MOVE_TIMEOUT),
                };
                let outcome = self.mutations.send_with(&request, options).await;
                Some(CdInput::Outcome { request, outcome })
            }
            CdEffect::UpdateFolder { .. } | CdEffect::Notice { .. } => None,
        }
    }
}

/// Looks at the target on the local disk, following symbolic links.
async fn check_folder(target: &std::path::Path) -> FolderCheck {
    match tokio::time::timeout(FOLDER_CHECK_TIMEOUT, tokio::fs::metadata(target)).await {
        Ok(Ok(metadata)) if metadata.is_dir() => FolderCheck::Directory,
        Ok(Ok(_)) => FolderCheck::NotADirectory,
        Ok(Err(error)) if error.kind() == ErrorKind::NotFound => FolderCheck::Missing,
        Ok(Err(_)) | Err(_) => FolderCheck::Unreadable,
    }
}
