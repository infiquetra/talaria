//! Wait a short time for one client frame. No voice handler is registered.

use std::time::Duration;

use serde_json::Value;

use super::peer::Peer;

/// The next JSON text frame, or nothing when the client stays silent.
pub async fn next_request(peer: &mut Peer) -> Option<Value> {
    match tokio::time::timeout(Duration::from_millis(200), peer.recv_json()).await {
        Ok(value) => Some(value),
        Err(_) => None,
    }
}
