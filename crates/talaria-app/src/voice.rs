//! Gateway-side voice capture and per-session state.

use std::collections::BTreeMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hermes_client::replay_source::ReplaySource;
use hermes_client::{Client, rpc::CallOptions};
use serde_json::{Value, json};
use talaria_core::voice::{
    RecordStatus, TurnOutcome, VoiceEffect, VoiceEventKind, VoiceInput, VoicePhase, VoiceState,
};
use tokio::sync::watch;

const STOP_BUDGET: Duration = Duration::from_millis(500);
pub const MICROPHONE_WARNING: &str =
    "The gateway microphone may still be recording; it stops on silence or at the recording limit.";

/// Settlement of an in-flight recording call, kept apart from the visible phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordSettlement {
    Started,
    Rejected,
    StopConfirmed,
    Unknown,
}

/// Voice capture is owned by the gateway; Talaria owns only one reducer per session.
#[derive(Clone, Debug)]
pub struct VoiceService {
    sessions: BTreeMap<String, VoiceState>,
    warnings: Arc<Mutex<Vec<String>>>,
    pending: BTreeMap<String, watch::Receiver<Option<RecordSettlement>>>,
    read_aloud: bool,
    speak_max_chars: usize,
    unavailable: bool,
}

impl Default for VoiceService {
    fn default() -> Self {
        Self {
            sessions: BTreeMap::new(),
            warnings: Arc::default(),
            pending: BTreeMap::new(),
            read_aloud: false,
            speak_max_chars: 1500,
            unavailable: false,
        }
    }
}

impl VoiceService {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn configure(&mut self, read_aloud: bool, speak_max_chars: usize) {
        self.read_aloud = read_aloud;
        self.speak_max_chars = speak_max_chars;
    }

    pub fn open(&mut self, session_id: &str) -> bool {
        if self.sessions.contains_key(session_id) {
            return false;
        }
        let mut state = VoiceState::new(session_id, self.read_aloud, self.speak_max_chars);
        if self.unavailable {
            let _ = state.update(VoiceInput::Unavailable, true);
        }
        self.sessions.insert(session_id.to_owned(), state);
        true
    }

    #[must_use]
    pub fn get(&self, session_id: &str) -> Option<&VoiceState> {
        self.sessions.get(session_id)
    }

    pub fn update(
        &mut self,
        session_id: &str,
        input: VoiceInput,
        composer_empty: bool,
    ) -> Vec<VoiceEffect> {
        if !self.sessions.contains_key(session_id) {
            return Vec::new();
        }
        if matches!(input, VoiceInput::ConnectionOpened) {
            self.unavailable = false;
        }
        let unavailable = matches!(
            &input,
            VoiceInput::Unavailable
                | VoiceInput::RecordFailed {
                    code: Some(-32601),
                    ..
                }
                | VoiceInput::SpeakFailed {
                    code: Some(-32601),
                    ..
                }
        );
        let first = !self.unavailable;
        if unavailable {
            self.unavailable = true;
        }
        let mut effects = self
            .sessions
            .get_mut(session_id)
            .map_or_else(Vec::new, |state| state.update(input, composer_empty));
        if unavailable {
            if !first {
                effects.retain(|effect| !matches!(effect, VoiceEffect::Notice(text) if text == "Voice is not available on this gateway."));
            }
            for (id, state) in &mut self.sessions {
                if id != session_id {
                    let _ = state.update(VoiceInput::Unavailable, true);
                }
            }
        }
        effects
    }

    pub fn session_ids(&self) -> Vec<String> {
        self.sessions.keys().cloned().collect()
    }

    /// Preserve switch-time warnings for the application's injected error output.
    pub fn keep_warning(&self, warning: String) {
        if let Ok(mut warnings) = self.warnings.lock() {
            warnings.push(warning);
        }
    }

    pub fn take_warnings(&self) -> Vec<String> {
        self.warnings
            .lock()
            .map(|mut warnings| std::mem::take(&mut *warnings))
            .unwrap_or_default()
    }

    /// Track the reply independently from the reducer so quit can await an in-flight request.
    pub fn track_record(&mut self, session_id: &str) -> watch::Sender<Option<RecordSettlement>> {
        let (sender, receiver) = watch::channel(None);
        self.pending.insert(session_id.to_owned(), receiver);
        sender
    }

    /// Stop a capture before detaching. An error reply confirms a stop; a lost reply does not.
    pub async fn stop_before_detach(
        &mut self,
        client: &Client,
        session_id: &str,
        err: &mut (dyn Write + Send),
    ) {
        let Some(state) = self.sessions.get_mut(session_id) else {
            return;
        };
        let phase = state.phase();
        let pending = state.pending_record();
        state.update(VoiceInput::DetachBegins, true);
        let receiver = self.pending.get_mut(session_id);
        let stopping = phase == VoicePhase::Stopping;
        let should_wait = stopping || matches!(phase, VoicePhase::Starting | VoicePhase::Recording);
        if !should_wait {
            return;
        }
        let operation = async {
            if let Some(pending_start) = pending {
                let Some(receiver) = receiver else {
                    return false;
                };
                while receiver.borrow().is_none() {
                    if receiver.changed().await.is_err() {
                        return false;
                    }
                }
                let reply = *receiver.borrow();
                if !pending_start {
                    return reply == Some(RecordSettlement::StopConfirmed);
                }
                match reply {
                    Some(RecordSettlement::Started) => {}
                    Some(RecordSettlement::Rejected) => return true,
                    _ => return false,
                }
            } else if stopping {
                return false;
            }
            match client
                .call_raw(
                    "voice.record",
                    json!({"action":"stop", "session_id":session_id}),
                    CallOptions::default(),
                )
                .await
            {
                Ok(_) | Err(hermes_client::rpc::CallError::Rpc(_)) => true,
                Err(_) => false,
            }
        };
        let confirmed = tokio::time::timeout(STOP_BUDGET, operation)
            .await
            .unwrap_or(false);
        if !confirmed {
            let _ = writeln!(err, "{MICROPHONE_WARNING}");
        }
        self.pending.remove(session_id);
    }

    /// Feed a contract frame log through the same reducer used for live notifications.
    #[must_use]
    pub fn submit_from_script(&self, source: &ReplaySource) -> Vec<Value> {
        let mut sessions = BTreeMap::<String, VoiceState>::new();
        let mut frames = Vec::new();
        for record in source.inbound() {
            let Some((session, event)) = voice_event(&record.frame["params"], true) else {
                continue;
            };
            let state = sessions.entry(session.clone()).or_insert_with(|| {
                let mut voice = VoiceState::new(&session, false, 1500);
                voice.update(VoiceInput::ModeRead(Some("voice_only".into())), true);
                voice.update(VoiceInput::Press, true);
                voice.update(
                    VoiceInput::RecordReply {
                        start: true,
                        status: RecordStatus::Recording,
                        reason: None,
                    },
                    true,
                );
                voice
            });
            for effect in state.update(VoiceInput::Event(event), true) {
                if let VoiceEffect::Submit(text) = effect {
                    frames.push(json!({"method":"prompt.submit", "params":crate::sessions::submit_params(&session, &text, "voice-replay-1")}));
                }
            }
        }
        frames
    }

    #[must_use]
    pub fn read_aloud_frames(&self, cells: &[String]) -> Vec<Value> {
        let mut state = VoiceState::new("", true, 1500);
        state
            .update(
                VoiceInput::TurnCompleted {
                    assistant_markdown: cells.to_vec(),
                    outcome: TurnOutcome::Completed,
                    focused: true,
                },
                true,
            )
            .into_iter()
            .filter_map(|effect| match effect {
                VoiceEffect::Speak(text) => {
                    Some(json!({"method":"voice.tts", "params":{"text":text}}))
                }
                _ => None,
            })
            .collect()
    }

    #[must_use]
    pub fn read_aloud_after_error(&self, code: i64) -> bool {
        let mut state = VoiceState::new("", true, 1500);
        state.update(
            VoiceInput::SpeakFailed {
                code: Some(code),
                message: String::new(),
                dropped: false,
            },
            true,
        );
        state.read_aloud()
    }

    #[must_use]
    pub fn after_voice_stopped(&self, session_id: &str) -> Vec<Value> {
        let mut state = VoiceState::new(session_id, false, 1500);
        state.update(VoiceInput::VoiceStopped, true).into_iter().filter_map(|effect| match effect {
            VoiceEffect::ReadMode => Some(json!({"method":"config.get", "params":{"key":"voice", "session_id":session_id}})),
            _ => None,
        }).collect()
    }
}

/// Decode the event payload without retaining gateway text in logs.
#[must_use]
pub fn voice_event(params: &Value, focused: bool) -> Option<(String, VoiceEventKind)> {
    let session_id = params.get("session_id")?.as_str()?.to_owned();
    let payload = &params["payload"];
    let event = match params.get("type")?.as_str()? {
        "voice.status" => VoiceEventKind::Status {
            state: payload.get("state")?.as_str()?.to_owned(),
        },
        "voice.partial" => VoiceEventKind::Partial {
            text: payload.get("text")?.as_str()?.to_owned(),
        },
        "voice.transcript" => VoiceEventKind::Transcript {
            text: payload
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            stop_phrase: payload
                .get("stop_phrase")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            typed: payload
                .get("typed")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            no_speech: payload
                .get("no_speech_limit")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            focused,
            session_id: session_id.clone(),
        },
        "voice.interrupted" => VoiceEventKind::Interrupted,
        _ => return None,
    };
    Some((session_id, event))
}
