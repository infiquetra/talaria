//! Slash-command service. Issue #223. The methods are stubs.

use hermes_client::Client;
use talaria_core::commands::{CommandOutcome, CommandsInput, CommandsState, PaletteInput};
use talaria_core::session::Event;

use crate::app::UserAction;

/// What a slash outcome asks the application loop to apply.
///
/// These are the session events of issue #223, fed through the existing
/// [`crate::app::AppMessage`] path. This is not a second event channel.
#[allow(clippy::large_enum_variant)]
pub enum AppAction {
    /// Apply one session event.
    Event(Event),
    /// Load the composer.
    LoadDraft(String),
    /// Remember a bare name that was refused.
    Refused { name: String },
    /// A foreground key already turned into a person action.
    Key(UserAction),
}

/// Runs catalogue fetches, completion, and `slash.exec`. Issue #223.
pub struct CommandService {
    client: Client,
    state: CommandsState,
}

impl CommandService {
    /// A service on this client. The catalogue starts unloaded.
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: CommandsState::default(),
        }
    }

    /// The catalogue store.
    #[must_use]
    pub fn state(&self) -> &CommandsState {
        &self.state
    }

    /// Drive the catalogue state machine.
    #[must_use]
    pub fn handle_input(&mut self, input: CommandsInput) -> Vec<AppAction> {
        let _ = (&self.client, &self.state, input);
        todo!()
    }

    /// Fetch the catalogue. The timeout is 15 seconds.
    pub async fn fetch_catalog(
        &self,
        request_id: u64,
        session_id: Option<String>,
    ) -> CommandsInput {
        let _ = (&self.client, request_id, session_id);
        todo!()
    }

    /// Complete one draft. The timeout is 5 seconds.
    pub async fn complete(
        &self,
        request_id: u64,
        text: String,
        session_id: Option<String>,
    ) -> PaletteInput {
        let _ = (&self.client, request_id, text, session_id);
        todo!()
    }

    /// Run one typed line. An alias is followed once inside this call.
    pub async fn execute(&mut self, session_id: &str, typed: &str) -> CommandOutcome {
        let _ = (&self.client, &self.state, session_id, typed);
        todo!()
    }

    /// The draft changed. At most one completion leaves, 60 ms after the last change.
    pub fn draft_changed(&mut self, text: String, caret: usize) {
        let _ = (&self.client, &self.state, text, caret);
        todo!()
    }
}

/// Turn an outcome into session events for the application loop.
#[must_use]
pub fn outcome_actions(outcome: CommandOutcome, typed: &str) -> Vec<AppAction> {
    let _ = (outcome, typed);
    todo!()
}
