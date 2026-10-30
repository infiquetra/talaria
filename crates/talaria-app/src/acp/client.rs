//! Protocol framing and decoding.

use super::AcpError;
use talaria_core::fleet::acp::AcpMapping;

pub fn replay_recording(_recording: &str) -> Result<AcpMapping, AcpError> {
    todo!("add agent protocol client")
}
