//! Hand-built requests for gateway-side voice capture and speech.

use hermes_contract::schemas::{
    CanonicalConfigGetResult, SlashExecResult, VoiceRecordResult, VoiceToggleResult, VoiceTtsResult,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::Client;
use crate::rpc::{CallError, CallOptions};

async fn call<T: DeserializeOwned>(
    c: &Client,
    method: &str,
    params: Value,
) -> Result<T, CallError> {
    let result = c.call_raw(method, params, CallOptions::default()).await?;
    serde_json::from_value(result).map_err(|_| CallError::BadResult {
        method: method.to_owned(),
        reason: "result did not decode",
    })
}

/// `config.get` of the session voice mode. `Some` is the raw `value`.
pub async fn voice_mode(c: &Client, session_id: &str) -> Result<Option<String>, CallError> {
    let result: CanonicalConfigGetResult = call(
        c,
        "config.get",
        json!({"key": "voice", "session_id": session_id}),
    )
    .await?;
    Ok(result.value)
}

/// `slash.exec` of `voice on` or `voice off` for one session.
pub async fn set_voice_mode(
    c: &Client,
    session_id: &str,
    on: bool,
) -> Result<SlashExecResult, CallError> {
    let command = if on { "voice on" } else { "voice off" };
    call(
        c,
        "slash.exec",
        json!({"session_id": session_id, "command": command}),
    )
    .await
}

/// `voice.toggle` with `action` `status` only.
pub async fn voice_requirements(c: &Client) -> Result<VoiceToggleResult, CallError> {
    call(c, "voice.toggle", json!({"action": "status"})).await
}

/// `voice.record` start or stop for one session.
pub async fn voice_record(
    c: &Client,
    session_id: &str,
    start: bool,
) -> Result<VoiceRecordResult, CallError> {
    let action = if start { "start" } else { "stop" };
    call(
        c,
        "voice.record",
        json!({"action": action, "session_id": session_id}),
    )
    .await
}

/// `voice.tts` of the already spoken text. No profile is accepted.
pub async fn voice_speak(c: &Client, text: &str) -> Result<VoiceTtsResult, CallError> {
    call(c, "voice.tts", json!({"text": text})).await
}

/// `config.voice.submit_mode` from `config.get` of `full`.
pub async fn voice_submit_mode(c: &Client) -> Result<Option<String>, CallError> {
    let result: CanonicalConfigGetResult = call(c, "config.get", json!({"key": "full"})).await?;
    Ok(result
        .config
        .as_ref()
        .and_then(|config| config.get("voice"))
        .and_then(|voice| voice.get("submit_mode"))
        .and_then(Value::as_str)
        .map(str::to_owned))
}
