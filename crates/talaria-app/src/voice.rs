//! Session voice service. The bodies are stubs: they emit no frames.

use std::io::Write;

use hermes_client::Client;
use hermes_client::replay_source::ReplaySource;
use serde_json::Value;
use talaria_core::voice::{VoiceEffect, VoiceInput};

/// One voice state per session. The stub does not open the microphone.
#[derive(Debug, Default)]
pub struct VoiceService;

impl VoiceService {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    pub fn open(&mut self, session_id: &str) -> bool {
        let _ = session_id;
        false
    }

    pub fn update(
        &mut self,
        session_id: &str,
        input: VoiceInput,
        composer_empty: bool,
    ) -> Vec<VoiceEffect> {
        let _ = (session_id, input, composer_empty);
        Vec::new()
    }

    pub async fn stop_before_detach(
        &mut self,
        client: &Client,
        session_id: &str,
        err: &mut (dyn Write + Send),
    ) {
        let _ = (client, session_id, err);
    }

    /// Frames the client would send while replaying a script. The stub sends none.
    #[must_use]
    pub fn submit_from_script(&self, source: &ReplaySource) -> Vec<Value> {
        let _ = source;
        Vec::new()
    }

    /// `voice.tts` frames for one finished turn. The stub sends none.
    #[must_use]
    pub fn read_aloud_frames(&self, cells: &[String]) -> Vec<Value> {
        let _ = cells;
        Vec::new()
    }

    /// The read-aloud flag after a speak error. The stub leaves it on.
    #[must_use]
    pub fn read_aloud_after_error(&self, code: i64) -> bool {
        let _ = code;
        true
    }

    /// Frames after a typed stop phrase for `session_id`. The stub sends none.
    #[must_use]
    pub fn after_voice_stopped(&self, session_id: &str) -> Vec<Value> {
        let _ = session_id;
        Vec::new()
    }

    /// Quit while a capture is open. The stub writes nothing and sends nothing.
    pub async fn quit_while_recording(&self, err: &mut dyn Write) -> Vec<Value> {
        let _ = err;
        Vec::new()
    }
}
