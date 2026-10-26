//! The connection one launch uses, then closes.

use std::time::Duration;

use hermes_client::liveness::LinkEvent;
use hermes_client::{CallError, CallOptions, Client, Generation, Inbound};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::bg::link::{LaunchLink, LinkScope};
use crate::bg::machine::{CallFailure, DescribeSummary};
use crate::connection::OpenConnection;

/// Hermes's code for a storage error inside the handler: its writes may or may not be committed.
const STORAGE_UNAVAILABLE: i64 = 5001;

/// An already-open foreground connection, or a short-lived link opened for this launch.
// The open command-line connection is a large type. Tests match the variant, so it stays unboxed.
#[allow(clippy::large_enum_variant)]
pub enum LaunchConn {
    /// The command line's open connection.
    Opened(OpenConnection),
    /// A link opened for this launch alone.
    Link(LaunchLink),
}

impl LaunchConn {
    /// Wrap the command line's connection, draining its events so the link keeps running.
    ///
    /// Events are dropped and server requests are left alone until the connection closes.
    #[must_use]
    pub fn opened(mut open: OpenConnection) -> Self {
        let (_closed, empty) = mpsc::channel(1);
        let inbound = std::mem::replace(&mut open.inbound, empty);
        tokio::spawn(drain_link(inbound));
        Self::Opened(open)
    }

    /// Send one call and wait up to `timeout`.
    ///
    /// [`CallFailure::NotSent`] means the frame was never written; a frame that may have been
    /// written and got no readable answer is [`CallFailure::Unknown`].
    pub async fn call(
        &self,
        method: &'static str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, CallFailure> {
        let options = CallOptions {
            timeout: Some(timeout),
        };
        self.client()
            .call_raw(method, params, options)
            .await
            .map_err(failure_of)
    }

    /// `Some(target)` when this is a host link and `target` is not the link's own profile.
    #[must_use]
    pub fn routed_profile(&self, target: &str) -> Option<String> {
        match self {
            Self::Opened(_) => None,
            Self::Link(link) => match link.scope() {
                LinkScope::Host { .. } if link.profile() != target => Some(target.to_owned()),
                LinkScope::Host { .. } | LinkScope::ProfileOnly { .. } => None,
            },
        }
    }

    /// Whether a launch in `target` can go through this connection: a profile-only link and
    /// the command line's connection reach their own profile alone.
    #[must_use]
    pub fn reaches(&self, target: &str) -> bool {
        match self {
            Self::Opened(open) => open.profile_name == target.trim().to_lowercase(),
            Self::Link(link) => match link.scope() {
                LinkScope::Host { .. } => true,
                LinkScope::ProfileOnly { profile } => profile == target,
            },
        }
    }

    /// The summary the handshake already fetched.
    #[must_use]
    pub fn describe(&self) -> DescribeSummary {
        match self {
            Self::Opened(open) => DescribeSummary {
                sources: open.describe.session_create.sources.clone(),
                parameters: open.describe.session_create.parameters.clone(),
            },
            Self::Link(link) => link.describe(),
        }
    }

    /// Close the connection. Closing detaches a session this link still holds.
    pub async fn close(self) {
        match self {
            Self::Opened(open) => open.link.close().await,
            Self::Link(link) => link.close().await,
        }
    }

    /// The socket generation calls go out on. It changes when the command line's link
    /// reconnects, which is how the create's retry knows the connection dropped.
    pub(crate) fn generation(&self) -> Generation {
        self.client().generation()
    }

    fn client(&self) -> Client {
        match self {
            Self::Opened(open) => open.link.client(),
            Self::Link(link) => link.client().clone(),
        }
    }
}

/// Drop events and leave every server request alone.
async fn drain_link(mut inbound: mpsc::Receiver<LinkEvent>) {
    while let Some(event) = inbound.recv().await {
        if let LinkEvent::Inbound {
            item: Inbound::ServerRequest(request),
            ..
        } = event
        {
            request.responder.ignore();
        }
    }
}

fn failure_of(error: CallError) -> CallFailure {
    match error {
        CallError::NotConnected => CallFailure::NotSent("not connected".to_owned()),
        CallError::Encode => CallFailure::NotSent("the request could not be encoded".to_owned()),
        CallError::InvalidTimeout => CallFailure::NotSent("the timeout was zero".to_owned()),
        CallError::Rpc(error) if error.code == STORAGE_UNAVAILABLE => CallFailure::Unknown,
        CallError::Rpc(error) => {
            let fields = error.data.as_ref().map(fields_of).unwrap_or_default();
            let reason = error
                .reason
                .clone()
                .or_else(|| (!error.message.is_empty()).then(|| error.message.clone()));
            CallFailure::Refused {
                code: error.code,
                reason,
                fields,
            }
        }
        CallError::TimedOut { .. } | CallError::Unknown { .. } => CallFailure::Unknown,
        CallError::BadResult { reason, .. } => CallFailure::Malformed(reason.to_owned()),
    }
}

/// `data.field`, or the list in `data.fields` that a key-set refusal carries.
fn fields_of(data: &Value) -> Vec<String> {
    if let Some(field) = data.get("field").and_then(Value::as_str) {
        return vec![field.to_owned()];
    }
    data.get("fields")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
