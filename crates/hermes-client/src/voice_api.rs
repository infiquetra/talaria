//! Hand-built voice requests. The bodies are stubs: nothing is sent.

use hermes_contract::schemas::{
    SlashExecResult, VoiceRecordResult, VoiceToggleResult, VoiceTtsResult,
};

use crate::Client;
use crate::rpc::CallError;

/// `config.get` of the session voice mode. `Some` is the raw `value`.
pub async fn voice_mode(c: &Client, session_id: &str) -> Result<Option<String>, CallError> {
    let _ = (c, session_id);
    Err(CallError::NotConnected)
}

/// `slash.exec` of `voice on` or `voice off` for one session.
pub async fn set_voice_mode(
    c: &Client,
    session_id: &str,
    on: bool,
) -> Result<SlashExecResult, CallError> {
    let _ = (c, session_id, on);
    Err(CallError::NotConnected)
}

/// `voice.toggle` with `action` `status` only.
pub async fn voice_requirements(c: &Client) -> Result<VoiceToggleResult, CallError> {
    let _ = c;
    Err(CallError::NotConnected)
}

/// `voice.record` start or stop for one session.
pub async fn voice_record(
    c: &Client,
    session_id: &str,
    start: bool,
) -> Result<VoiceRecordResult, CallError> {
    let _ = (c, session_id, start);
    Err(CallError::NotConnected)
}

/// `voice.tts` of the already spoken text. No profile is accepted.
pub async fn voice_speak(c: &Client, text: &str) -> Result<VoiceTtsResult, CallError> {
    let _ = (c, text);
    Err(CallError::NotConnected)
}

/// `config.voice.submit_mode` from `config.get` of `full`.
pub async fn voice_submit_mode(c: &Client) -> Result<Option<String>, CallError> {
    let _ = c;
    Err(CallError::NotConnected)
}
