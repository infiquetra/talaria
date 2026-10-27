//! Executes configuration-session effects; the reducer owns all decisions.

use super::{ConfigureBackends, wire};
use hermes_client::{CallError, CallOptions, Client, ServerRequest};
use hermes_contract::schemas::CanonicalSnapshot;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Mutex, time::Duration};
use talaria_core::{
    configure::{ConfigureEffect, ConfigureInput, QUIT_DEADLINE},
    session::{Event, PromptKey},
};

#[derive(Default)]
struct Handles {
    session: Option<(String, String, i64)>,
    approval: Option<PromptKey>,
    running: bool,
    detached: bool,
    closing: bool,
    requests: HashMap<String, ServerRequest>,
}
pub struct ConfigureService<B: ConfigureBackends> {
    client: Option<Client>,
    backends: B,
    profile: String,
    handles: Mutex<Handles>,
}

/// Application-side detector injected into the pure planner and reducer.
pub fn detect_secret(text: &str) -> bool {
    !hermes_client::redact::scan_for_secrets(text).is_empty()
}
fn clean(text: &str) -> String {
    if detect_secret(text) {
        talaria_core::configure::WITHHELD.into()
    } else {
        text.chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect()
    }
}

impl<B: ConfigureBackends> ConfigureService<B> {
    pub fn new(client: Option<Client>, backends: B, profile: String) -> Self {
        Self {
            client,
            backends,
            profile,
            handles: Mutex::new(Handles::default()),
        }
    }
    /// Retain only protocol identities needed for bounded shutdown.
    pub fn observe(&self, event: &Event) {
        let Event::Session {
            session_id, raw, ..
        } = event
        else {
            return;
        };
        let mut handles = self
            .handles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some((id, _, generation)) = handles.session.as_mut() else {
            return;
        };
        if id != session_id.as_str() {
            return;
        }
        if let Some(current) = raw.get("execution_generation").and_then(Value::as_i64) {
            *generation = current;
        }
        match raw.get("type").and_then(Value::as_str) {
            Some("message.start") => handles.running = true,
            Some("message.complete") => {
                handles.running = false;
                handles.approval = None;
            }
            Some("approval.request") => {
                let payload = &raw["payload"];
                if let Some(prompt_id) = payload["prompt_id"].as_str() {
                    handles.approval = Some(PromptKey {
                        session_id: session_id.clone(),
                        prompt_id: prompt_id.into(),
                        execution_generation: payload["execution_generation"]
                            .as_i64()
                            .or_else(|| raw["execution_generation"].as_i64())
                            .unwrap_or(0),
                    });
                }
            }
            _ => {}
        }
    }
    pub fn hold_request(&self, request: ServerRequest) {
        self.handles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .requests
            .insert(request.id.clone(), request);
    }
    async fn call(&self, method: &str, params: Value, seconds: u64) -> Result<Value, CallError> {
        let client = self.client.as_ref().ok_or(CallError::NotConnected)?;
        client
            .call_raw(
                method,
                params,
                CallOptions {
                    timeout: Some(Duration::from_secs(seconds)),
                },
            )
            .await
    }
    pub async fn execute(&self, effect: ConfigureEffect) -> Vec<ConfigureInput> {
        match effect {
            ConfigureEffect::Gather => match self.backends.read_context(&self.profile).await {
                Ok(context) => vec![ConfigureInput::Gathered(Box::new(context))],
                Err(error) => vec![ConfigureInput::GatherFailed(clean(&error))],
            },
            ConfigureEffect::CreateSession(spec) => match self
                .call("session.create", wire::create_params(&spec), 30)
                .await
            {
                Ok(value) => match serde_json::from_value::<CanonicalSnapshot>(value) {
                    Ok(snapshot) => {
                        let closing = {
                            let mut handles = self
                                .handles
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            handles.session = Some((
                                snapshot.session_id.clone(),
                                snapshot.subscription_id.clone(),
                                snapshot.execution_generation,
                            ));
                            handles.closing
                        };
                        if closing {
                            self.detach(&snapshot.session_id, &snapshot.subscription_id)
                                .await;
                            vec![ConfigureInput::Detached]
                        } else {
                            vec![ConfigureInput::Created(Box::new(snapshot))]
                        }
                    }
                    Err(_) => vec![ConfigureInput::CreateRefused {
                        field: None,
                        detail: None,
                        message: "session reply could not be read".into(),
                    }],
                },
                Err(CallError::Rpc(error)) => vec![ConfigureInput::CreateRefused {
                    field: error
                        .data
                        .as_ref()
                        .and_then(|data| data["field"].as_str())
                        .map(clean),
                    detail: error
                        .data
                        .as_ref()
                        .and_then(|data| data["detail"].as_str())
                        .map(clean),
                    message: clean(&error.message),
                }],
                Err(error) => vec![ConfigureInput::CreateRefused {
                    field: None,
                    detail: None,
                    message: clean(&error.to_string()),
                }],
            },
            ConfigureEffect::Submit {
                session_id,
                text,
                submission_id,
            } => {
                if detect_secret(&text) {
                    return vec![ConfigureInput::SubmitFailed(
                        "text withheld because it looks like a secret".into(),
                    )];
                }
                self.handles
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .running = true;
                match self
                    .call(
                        "prompt.submit",
                        wire::submit_params(&session_id, &text, &submission_id),
                        30,
                    )
                    .await
                {
                    Ok(_) => vec![ConfigureInput::Submitted],
                    Err(error) => vec![ConfigureInput::SubmitFailed(clean(&error.to_string()))],
                }
            }
            ConfigureEffect::Apply { index, step } => vec![ConfigureInput::StepDone {
                index,
                result: self.backends.apply(&step).await,
            }],
            ConfigureEffect::Interrupt {
                session_id,
                execution_generation,
            } => {
                let _ = self
                    .call(
                        "session.interrupt",
                        wire::interrupt_params(&session_id, execution_generation),
                        10,
                    )
                    .await;
                self.handles
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .running = false;
                Vec::new()
            }
            ConfigureEffect::Detach {
                session_id,
                subscription_id,
            } => {
                self.detach(&session_id, &subscription_id).await;
                vec![ConfigureInput::Detached]
            }
            ConfigureEffect::DenyApproval { key } => {
                let _ = self.call("approval.respond", json!({"session_id":key.session_id.as_str(), "execution_generation":key.execution_generation, "prompt_id":key.prompt_id,"choice":"deny"}), 10).await;
                self.handles
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .approval = None;
                Vec::new()
            }
            ConfigureEffect::RespondClarify { key, answer } => {
                if !detect_secret(&answer) {
                    let _ = self.call("clarify.respond", json!({"session_id":key.session_id.as_str(), "execution_generation":key.execution_generation,"prompt_id":key.prompt_id,"answer":answer}), 30).await;
                }
                Vec::new()
            }
            ConfigureEffect::RefuseServerRequest { request_id } => {
                let request = self
                    .handles
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .requests
                    .remove(&request_id);
                if let Some(request) = request {
                    let _ = request
                        .responder
                        .fail(
                            -32601,
                            "configuration sessions cannot request secrets or commands",
                        )
                        .await;
                }
                Vec::new()
            }
            ConfigureEffect::ForgetHistoryLine(_)
            | ConfigureEffect::ClearEditor
            | ConfigureEffect::Open(_)
            | ConfigureEffect::Notice(_)
            | ConfigureEffect::CloseOverlay => Vec::new(),
        }
    }
    async fn detach(&self, session_id: &str, subscription_id: &str) {
        {
            let mut handles = self
                .handles
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if handles.detached {
                return;
            }
            handles.detached = true;
        }
        if let Some(client) = &self.client
            && client.stats().await.closed_at.is_none()
        {
            let _ = self
                .call(
                    "session.detach",
                    wire::detach_params(session_id, subscription_id),
                    2,
                )
                .await;
        }
    }
    pub async fn close(&self) {
        let (session, approval, running) = {
            let mut handles = self
                .handles
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            handles.closing = true;
            (
                handles.session.clone(),
                handles.approval.take(),
                handles.running,
            )
        };
        let _ = tokio::time::timeout(QUIT_DEADLINE, async {
            if let Some(key) = approval {
                self.execute(ConfigureEffect::DenyApproval { key }).await;
            }
            if let Some((session_id, subscription_id, execution_generation)) = session {
                if running {
                    self.execute(ConfigureEffect::Interrupt {
                        session_id: session_id.clone(),
                        execution_generation,
                    })
                    .await;
                }
                self.detach(&session_id, &subscription_id).await;
            }
        })
        .await;
    }
}
