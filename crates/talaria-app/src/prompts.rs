//! The protocol side of prompt cards: answering server requests and respond calls.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

use hermes_client::{CallError, Client, RespondError, Responder, ServerRequest};
use hermes_contract::methods::canonical::{approval_respond, clarify_respond};
use hermes_contract::schemas::{PromptResponseResult, PromptResponseResultStatus};
use hermes_contract::server_requests::ServerRequest as ContractRequest;
use talaria_core::session::{
    AnswerRefusal, Effect, Event, PromptKey, PromptKind, PromptState, SessionState,
};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskedKind {
    Sudo,
    Secret,
    VaultCode,
    VaultUnlockPrompt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerRequestPolicy {
    Masked(MaskedKind),
    Refuse,
}

/// `Refuse` for every name that is not one of the four masked kinds.
pub fn policy_for(method: &str) -> ServerRequestPolicy {
    match method {
        "sudo" => ServerRequestPolicy::Masked(MaskedKind::Sudo),
        "secret" => ServerRequestPolicy::Masked(MaskedKind::Secret),
        "vault.code" => ServerRequestPolicy::Masked(MaskedKind::VaultCode),
        "vault.unlock_prompt" => ServerRequestPolicy::Masked(MaskedKind::VaultUnlockPrompt),
        _ => ServerRequestPolicy::Refuse,
    }
}

#[derive(Clone, Debug)]
pub struct MaskedRequestInfo {
    pub id: String,
    pub session_id: Option<String>,
    pub kind: MaskedKind,
    pub fields: Vec<(&'static str, String)>,
}

#[derive(Clone, Debug)]
pub struct PromptNotice {
    pub session_id: Option<String>,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Executed {
    pub events: Vec<Event>,
    pub notices: Vec<PromptNotice>,
}

pub struct PromptService {
    next_submission_id: Box<dyn FnMut() -> String + Send>,
    generation: Option<u64>,
    masked: Vec<(MaskedRequestInfo, Responder)>,
    answered_requests: HashMap<String, (Option<String>, &'static str)>,
    notified_kinds: HashSet<&'static str>,
    open_prompts: Vec<PromptKey>,
    answered_prompts: HashSet<PromptKey>,
    notes: HashMap<PromptKey, String>,
}

impl PromptService {
    pub fn new(next_submission_id: Box<dyn FnMut() -> String + Send>) -> Self {
        Self {
            next_submission_id,
            generation: None,
            masked: Vec::new(),
            answered_requests: HashMap::new(),
            notified_kinds: HashSet::new(),
            open_prompts: Vec::new(),
            answered_prompts: HashSet::new(),
            notes: HashMap::new(),
        }
    }

    pub async fn on_server_request(&mut self, req: ServerRequest) -> Vec<PromptNotice> {
        if self
            .generation
            .is_some_and(|generation| req.generation.0 < generation)
        {
            req.responder.ignore();
            return Vec::new();
        }
        let mut notices = self.on_connection_replaced(req.generation.0);
        let method = request_method(&req.request);
        let session_id = req.session_id.filter(|id| !id.is_empty());
        let (kind, fields) = match req.request {
            ContractRequest::Sudo(params) => (MaskedKind::Sudo, vec![("command", params.command)]),
            ContractRequest::Secret(params) => (
                MaskedKind::Secret,
                vec![("env_var", params.env_var), ("prompt", params.prompt)],
            ),
            ContractRequest::VaultCode(params) => (
                MaskedKind::VaultCode,
                vec![
                    ("site", params.site.unwrap_or_default()),
                    ("hint", params.hint.unwrap_or_default()),
                ],
            ),
            ContractRequest::VaultUnlockPrompt(params) => (
                MaskedKind::VaultUnlockPrompt,
                vec![
                    ("backend", params.backend),
                    ("display_name", params.display_name),
                ],
            ),
            _ => {
                let message = format!("talaria does not support server request: {method}");
                if req.responder.fail(-32601, &message).await.is_ok() {
                    self.answered_requests
                        .insert(req.id, (session_id.clone(), method));
                } else {
                    req.responder.ignore();
                }
                if self.notified_kinds.insert(method) {
                    notices.push(PromptNotice {
                        session_id,
                        text: format!(
                            "Talaria does not support the {} request; Hermes continued without it",
                            sanitize(method)
                        ),
                    });
                }
                return notices;
            }
        };
        self.masked.push((
            MaskedRequestInfo {
                id: req.id,
                session_id,
                kind,
                fields,
            },
            req.responder,
        ));
        notices
    }

    pub fn on_request_cancel(&mut self, id: &str, method: &str, reason: &str) -> Vec<PromptNotice> {
        if let Some(index) = self.masked.iter().position(|(info, _)| info.id == id) {
            let (info, responder) = self.masked.remove(index);
            responder.ignore();
            let text = match reason {
                "timeout" => "timed out; the agent continued without it".to_owned(),
                "interrupted" => "withdrawn: the turn was interrupted".to_owned(),
                "resolved" => "answered in another window".to_owned(),
                _ => format!("withdrawn by Hermes ({})", sanitize(reason)),
            };
            return vec![PromptNotice {
                session_id: info.session_id,
                text,
            }];
        }
        if let Some((session_id, _)) = self.answered_requests.remove(id)
            && matches!(reason, "timeout" | "interrupted")
        {
            return vec![PromptNotice {
                session_id,
                text: format!(
                    "Hermes did not accept the answer to {}; the agent continued without it",
                    sanitize(method)
                ),
            }];
        }
        Vec::new()
    }

    pub fn on_connection_replaced(&mut self, generation: u64) -> Vec<PromptNotice> {
        if self.generation.is_some_and(|current| generation <= current) {
            return Vec::new();
        }
        self.generation = Some(generation);
        self.answered_requests.clear();
        self.notified_kinds.clear();
        self.masked
            .drain(..)
            .map(|(info, responder)| {
                responder.ignore();
                PromptNotice {
                    session_id: info.session_id,
                    text: "lost when the connection dropped".to_owned(),
                }
            })
            .collect()
    }

    pub async fn answer_masked(
        &mut self,
        request_id: &str,
        value: Zeroizing<String>,
    ) -> Result<(), RespondError> {
        let Some(index) = self
            .masked
            .iter()
            .position(|(info, _)| info.id == request_id)
        else {
            return Err(RespondError::Cancelled);
        };
        // Borrow the zeroing buffer; the transport alone serializes the response value.
        let result = self.masked[index]
            .1
            .respond(&BTreeMap::from([("value", value.as_str())]))
            .await;
        if result.is_ok() {
            let (info, responder) = self.masked.remove(index);
            self.answered_requests
                .insert(info.id, (info.session_id, masked_method(info.kind)));
            drop(responder);
        } else if matches!(
            result,
            Err(RespondError::Cancelled | RespondError::AlreadyAnswered)
        ) {
            let (_, responder) = self.masked.remove(index);
            responder.ignore();
        }
        result
    }

    pub fn masked_requests(&self) -> Vec<MaskedRequestInfo> {
        self.masked.iter().map(|(info, _)| info.clone()).collect()
    }

    pub fn set_note(&mut self, key: &PromptKey, note: String) {
        if note.trim().is_empty() {
            self.notes.remove(key);
        } else {
            self.notes.insert(key.clone(), note);
        }
    }

    pub async fn execute(&mut self, client: &Client, effect: Effect) -> Executed {
        let (key, result) = match effect {
            Effect::RespondApproval { key, choice } => {
                let result = client
                    .call::<approval_respond::Marker>(approval_params(&key, choice))
                    .await;
                (key, result)
            }
            Effect::RespondClarify { key, answer } => {
                let result = client
                    .call::<clarify_respond::Marker>(clarify_respond::Params {
                        session_id: key.session_id.as_str().to_owned(),
                        execution_generation: key.execution_generation,
                        prompt_id: key.prompt_id.clone(),
                        answer,
                    })
                    .await;
                (key, result)
            }
            Effect::RefuseAnswer { key, reason } => {
                let text = match reason {
                    AnswerRefusal::AlreadyAnswered => return Executed::default(),
                    AnswerRefusal::NoSuchPrompt => "no such prompt",
                    AnswerRefusal::WrongKind => "wrong prompt kind",
                    AnswerRefusal::ChoiceNotOffered => "choice not offered",
                };
                return Executed {
                    events: Vec::new(),
                    notices: vec![prompt_notice(&key, format!("could not answer: {text}"))],
                };
            }
            _ => return Executed::default(),
        };
        let note = self.notes.remove(&key);
        match result {
            Ok(PromptResponseResult {
                status: PromptResponseResultStatus::Resolved,
                ..
            }) => {
                self.answered_prompts.insert(key);
                Executed {
                    events: note
                        .into_iter()
                        .map(|text| Event::Submit {
                            display: text.clone(),
                            text,
                            submission_id: (self.next_submission_id)(),
                            attachments: Vec::new(),
                        })
                        .collect(),
                    notices: Vec::new(),
                }
            }
            Ok(_) => {
                self.answered_prompts.insert(key.clone());
                Executed {
                    notices: vec![prompt_notice(
                        &key,
                        with_note("answered elsewhere or timed out".to_owned(), note),
                    )],
                    events: vec![Event::AnswerAlreadyResolved { key }],
                }
            }
            Err(error) => Executed {
                notices: vec![prompt_notice(
                    &key,
                    with_note(
                        format!("could not answer: {}", sanitize(&call_reason(&error))),
                        note,
                    ),
                )],
                events: vec![Event::AnswerFailed { key }],
            },
        }
    }

    pub fn on_prompts_changed(&mut self, open: &[PromptKey]) -> Vec<PromptNotice> {
        let notices = self
            .open_prompts
            .iter()
            .filter(|key| !open.contains(key) && !self.answered_prompts.contains(key))
            .map(|key| prompt_notice(key, "answered elsewhere or timed out".to_owned()))
            .collect();
        self.notes.retain(|key, _| open.contains(key));
        self.answered_prompts.retain(|key| open.contains(key));
        self.open_prompts = open.to_vec();
        notices
    }

    pub async fn deny_before_interrupt(
        &mut self,
        client: &Client,
        state: &SessionState,
        session_id: &str,
        wait: std::time::Duration,
    ) -> Vec<PromptNotice> {
        let mut calls = tokio::task::JoinSet::new();
        for prompt in state.open_prompts().filter(|prompt| {
            prompt.key.session_id.as_str() == session_id
                && prompt.kind == PromptKind::Approval
                && prompt.state == PromptState::Open
                && prompt.choices.iter().any(|choice| choice == "deny")
        }) {
            self.answered_prompts.insert(prompt.key.clone());
            self.notes.remove(&prompt.key);
            let client = client.clone();
            let params = approval_params(&prompt.key, "deny".to_owned());
            calls.spawn(async move {
                client
                    .call::<approval_respond::Marker>(params)
                    .await
                    .is_ok()
            });
        }
        let confirmed = tokio::time::timeout(wait.min(Duration::from_secs(2)), async {
            let mut confirmed = true;
            while let Some(result) = calls.join_next().await {
                confirmed &= matches!(result, Ok(true));
            }
            confirmed
        })
        .await;
        if matches!(confirmed, Ok(true)) {
            Vec::new()
        } else {
            vec![PromptNotice {
                session_id: Some(session_id.to_owned()),
                text: "deny not confirmed; interrupting anyway".to_owned(),
            }]
        }
    }
}

fn approval_params(key: &PromptKey, choice: String) -> approval_respond::Params {
    approval_respond::Params {
        session_id: key.session_id.as_str().to_owned(),
        execution_generation: key.execution_generation,
        prompt_id: key.prompt_id.clone(),
        choice,
        profile: None,
    }
}

fn prompt_notice(key: &PromptKey, text: String) -> PromptNotice {
    PromptNotice {
        session_id: Some(key.session_id.as_str().to_owned()),
        text,
    }
}

fn with_note(mut text: String, note: Option<String>) -> String {
    if let Some(note) = note {
        text.push_str("; note not sent: ");
        text.push_str(&sanitize(&note));
    }
    text
}

fn call_reason(error: &CallError) -> String {
    match error {
        CallError::Unknown { .. } => "connection lost".to_owned(),
        CallError::TimedOut { .. } => "no reply in time".to_owned(),
        CallError::Rpc(error) => error.reason.clone().unwrap_or_else(|| error.to_string()),
        _ => error.to_string(),
    }
}

fn masked_method(kind: MaskedKind) -> &'static str {
    match kind {
        MaskedKind::Sudo => "sudo",
        MaskedKind::Secret => "secret",
        MaskedKind::VaultCode => "vault.code",
        MaskedKind::VaultUnlockPrompt => "vault.unlock_prompt",
    }
}

fn request_method(request: &ContractRequest) -> &'static str {
    match request {
        ContractRequest::Approval(_) => "approval",
        ContractRequest::Clarify(_) => "clarify",
        ContractRequest::DisplayInstallSudo(_) => "display.install.sudo",
        ContractRequest::PreviewAct(_) => "preview.act",
        ContractRequest::PreviewRead(_) => "preview.read",
        ContractRequest::Secret(_) => "secret",
        ContractRequest::SetupBeat(_) => "setup_beat",
        ContractRequest::Sudo(_) => "sudo",
        ContractRequest::TerminalRead(_) => "terminal.read",
        ContractRequest::ThemeApply(_) => "theme.apply",
        ContractRequest::ThemePick(_) => "theme.pick",
        ContractRequest::Tour(_) => "tour",
        ContractRequest::VaultCode(_) => "vault.code",
        ContractRequest::VaultSaveLogin(_) => "vault.save_login",
        ContractRequest::VaultUnlockPrompt(_) => "vault.unlock_prompt",
        ContractRequest::WindowRead(_) => "window.read",
        ContractRequest::Unknown(_) => "unknown",
    }
}

// Keep notice text safe without crossing the application's presentation dependency boundary.
// The visible replacements match the terminal history sanitizer.
fn sanitize(text: &str) -> String {
    let mut sanitized = String::with_capacity(text.len());
    for ch in text.chars() {
        let code = u32::from(ch);
        match code {
            0x09 => sanitized.push_str("    "),
            0x00..=0x1f => {
                if let Some(picture) = char::from_u32(0x2400 + code) {
                    sanitized.push(picture);
                }
            }
            0x7f => sanitized.push('\u{2421}'),
            0x80..=0x9f
            | 0x061c
            | 0x200e
            | 0x200f
            | 0x2028..=0x202e
            | 0x2066..=0x2069
            | 0xe0000..=0xe007f => {
                sanitized.push_str(&format!("‹U+{code:04X}›"));
            }
            _ => sanitized.push(ch),
        }
    }
    sanitized
}
