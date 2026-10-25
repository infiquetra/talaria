//! One queue call at a time. The reducer decides the next step.
//!
//! Issue 232. Queue and steer.

use hermes_client::Client;
use talaria_core::queue::QueueIntent;
use talaria_core::session::{Effect, Event, SessionId};

/// Runs a single queue effect and returns the outcome events.
pub struct QueueService {
    client: Client,
    session_id: SessionId,
}

impl QueueService {
    /// Hold the client that will make the one call.
    #[must_use]
    pub fn new(client: Client, session_id: SessionId) -> Self {
        Self { client, session_id }
    }

    /// Make exactly one call. No loop, no retry, and no wait for frames.
    ///
    /// Issue 232. Queue and steer.
    pub async fn run(&mut self, effect: Effect) -> Vec<Event> {
        let _ = (&self.client, &self.session_id, effect);
        todo!("Issue 232. Queue and steer.")
    }
}

/// What `/queue` asks the session to do.
#[derive(Clone, Debug, PartialEq)]
pub enum QueueCommand {
    /// A queue intent.
    Intent(QueueIntent),
    /// Submit this text.
    Submit(String),
    /// Show this notice and make no call.
    Notice(&'static str),
}

/// Parse the text after `/queue`. Issue 232. Queue and steer.
#[must_use]
pub fn queue_command(arg: &str) -> QueueCommand {
    let _ = arg;
    todo!("Issue 232. Queue and steer.")
}

/// Parse the text after `/steer`. Issue 232. Queue and steer.
#[must_use]
pub fn steer_command(arg: &str) -> QueueCommand {
    let _ = arg;
    todo!("Issue 232. Queue and steer.")
}
