//! Application messages for a fleet backend owned outside the Hermes gateway.

use std::path::PathBuf;
use std::time::Duration;

use talaria_core::fleet::RowKey;
use tokio::sync::{mpsc, oneshot};

/// Operations sent to the owner of an external fleet session.
pub enum ExternalCommand {
    Launch {
        agent_id: String,
        cwd: PathBuf,
        prompt: String,
    },
    Prompt {
        key: RowKey,
        text: String,
    },
    Cancel {
        key: RowKey,
    },
    Answer {
        key: RowKey,
        responder: String,
        tool_call_id: String,
        option_id: String,
    },
    Acknowledge {
        key: RowKey,
    },
    Shutdown(oneshot::Sender<()>),
}

/// Result of a fleet action handled by an external owner.
pub enum ExternalResult {
    Launch(Result<(), String>),
    Prompt {
        key: RowKey,
        result: Result<(), String>,
    },
    Cancel {
        key: RowKey,
        result: Result<(), String>,
    },
    Answer {
        result: Result<(), String>,
    },
}

/// A message channel and task owned by an external fleet backend.
pub struct ExternalHandle {
    sender: mpsc::UnboundedSender<ExternalCommand>,
    join: tokio::task::JoinHandle<()>,
}

impl ExternalHandle {
    pub fn new(
        sender: mpsc::UnboundedSender<ExternalCommand>,
        join: tokio::task::JoinHandle<()>,
    ) -> Self {
        Self { sender, join }
    }
    pub fn send(&self, command: ExternalCommand) -> bool {
        self.sender.send(command).is_ok()
    }
    pub async fn close(self) {
        let (reply, receive) = oneshot::channel();
        let _ = self.sender.send(ExternalCommand::Shutdown(reply));
        let mut join = self.join;
        if tokio::time::timeout(Duration::from_secs(15), receive)
            .await
            .is_err()
        {
            join.abort();
        }
        if tokio::time::timeout(Duration::from_secs(2), &mut join)
            .await
            .is_err()
        {
            join.abort();
        }
    }
}
