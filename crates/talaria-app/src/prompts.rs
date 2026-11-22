//! The protocol side of prompt cards: answering server requests and respond calls.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use hermes_client::{CallError, Client, RespondError, Responder, ServerRequest};
use hermes_contract::methods::canonical::{approval_respond, clarify_respond};
use hermes_contract::methods::legacy::connection_respond;
use hermes_contract::schemas::{PromptResponseResult, PromptResponseResultStatus};
use hermes_contract::server_requests::ServerRequest as ContractRequest;
use talaria_core::session::{
    AnswerRefusal, Effect, Event, PromptKey, PromptKind, PromptState, SessionState,
};
use zeroize::Zeroizing;

use crate::app::AppEffect;

/// Cancelled request ids kept so a late register cannot restore the card.
const CANCELLED_REQUEST_LIMIT: usize = 64;

static NEXT_SERVICE: AtomicU64 = AtomicU64::new(1);

/// A live masked card, bound to the service and request that opened it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveMaskedTarget {
    pub token: String,
    pub session_id: Option<String>,
    pub generation: u64,
    pub instance_id: u64,
    pub service_id: u64,
}

/// A masked answer is either a live card or a recovered prompt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaskedTarget {
    Live(LiveMaskedTarget),
    Recovered { request_id: String },
}

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
    if !talaria_core::session::is_answerable(method) {
        return ServerRequestPolicy::Refuse;
    }
    match method {
        "sudo" => ServerRequestPolicy::Masked(MaskedKind::Sudo),
        "secret" => ServerRequestPolicy::Masked(MaskedKind::Secret),
        "vault.code" => ServerRequestPolicy::Masked(MaskedKind::VaultCode),
        "vault.unlock_prompt" => ServerRequestPolicy::Masked(MaskedKind::VaultUnlockPrompt),
        _ => ServerRequestPolicy::Refuse,
    }
}

/// A blank id is not a session. Storing it would let a card look like the foreground one.
fn present_session_id(session_id: Option<String>) -> Option<String> {
    session_id.and_then(|id| {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

pub(crate) fn cancel_reason_notice(reason: &str) -> String {
    match reason {
        "timeout" => "timed out; the agent continued without it".to_owned(),
        "interrupted" => "withdrawn: the turn was interrupted".to_owned(),
        "resolved" => "answered in another window".to_owned(),
        _ => format!("withdrawn by Hermes ({})", sanitize(reason)),
    }
}

#[derive(Clone, Debug)]
pub struct MaskedRequestInfo {
    pub id: String,
    /// Client-generated identity for the card and for [`PromptService::answer_masked`].
    pub token: String,
    /// Connection generation that delivered the request.
    pub generation: u64,
    /// The responder instance on that connection.
    pub instance_id: u64,
    /// The [`PromptService`] that accepted the request.
    pub service_id: u64,
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

impl MaskedRequestInfo {
    pub fn target(&self) -> LiveMaskedTarget {
        LiveMaskedTarget {
            token: self.token.clone(),
            session_id: self.session_id.clone(),
            generation: self.generation,
            instance_id: self.instance_id,
            service_id: self.service_id,
        }
    }
}

pub struct PromptService {
    service_id: u64,
    next_submission_id: Box<dyn FnMut() -> String + Send>,
    next_token: u64,
    generation: Option<u64>,
    masked: Vec<(MaskedRequestInfo, Responder)>,
    /// Request ids whose cancel arrived, including ones that had no card yet.
    cancelled_requests: VecDeque<String>,
    answered_requests: HashMap<String, (Option<String>, &'static str)>,
    answered_order: VecDeque<String>,
    notified_kinds: HashSet<&'static str>,
    open_prompts: Vec<PromptKey>,
    answered_prompts: HashSet<PromptKey>,
    notes: HashMap<PromptKey, String>,
}

impl PromptService {
    pub fn new(next_submission_id: Box<dyn FnMut() -> String + Send>) -> Self {
        Self {
            service_id: NEXT_SERVICE.fetch_add(1, Ordering::Relaxed),
            next_submission_id,
            next_token: 0,
            generation: None,
            masked: Vec::new(),
            cancelled_requests: VecDeque::new(),
            answered_requests: HashMap::new(),
            answered_order: VecDeque::new(),
            notified_kinds: HashSet::new(),
            open_prompts: Vec::new(),
            answered_prompts: HashSet::new(),
            notes: HashMap::new(),
        }
    }

    pub async fn on_server_request(&mut self, req: ServerRequest) -> Vec<PromptNotice> {
        self.on_server_request_for_session(req, None).await
    }

    /// Answer `req` when it belongs to `owned_session`.
    ///
    /// A request that names a different session is left unanswered. `None` keeps the
    /// caller responsible for that check.
    pub async fn on_server_request_for_session(
        &mut self,
        req: ServerRequest,
        owned_session: Option<&str>,
    ) -> Vec<PromptNotice> {
        if let (Some(owned), Some(session)) = (
            owned_session.filter(|id| !id.is_empty()),
            req.session_id.as_deref().filter(|id| !id.is_empty()),
        ) && session != owned
        {
            req.responder.ignore();
            return Vec::new();
        }
        if self
            .generation
            .is_some_and(|generation| req.generation.0 < generation)
        {
            req.responder.ignore();
            return Vec::new();
        }
        let cancelled = self.was_cancelled(&req.id);
        let mut notices = self.on_connection_replaced(req.generation.0);
        if cancelled || self.was_cancelled(&req.id) {
            req.responder.ignore();
            return notices;
        }
        let method = request_method(&req.request);
        let session_id = present_session_id(req.session_id);
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
                    self.remember_answered(req.id, session_id.clone(), method);
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
        let fields = fields
            .into_iter()
            .map(|(name, value)| (name, bounded_field(value)))
            .collect();
        if self.masked.len() >= MAX_OPEN_MASKED {
            if req
                .responder
                .fail(-32000, "too many open masked requests")
                .await
                .is_err()
            {
                req.responder.ignore();
            }
            return notices;
        }
        self.next_token = self.next_token.saturating_add(1);
        let token = format!("masked-{}", self.next_token);
        let generation = req.generation.0;
        self.masked.push((
            MaskedRequestInfo {
                id: req.id,
                token,
                generation,
                instance_id: req.responder.instance_id(),
                service_id: self.service_id,
                session_id,
                kind,
                fields,
            },
            req.responder,
        ));
        notices
    }

    pub fn on_request_cancel(&mut self, id: &str, method: &str, reason: &str) -> Vec<PromptNotice> {
        self.remember_cancelled(id);
        if let Some(index) = self
            .masked
            .iter()
            .position(|(info, _)| info.id == id && masked_method(info.kind) == method)
        {
            let (info, responder) = self.masked.remove(index);
            responder.ignore();
            let text = cancel_reason_notice(reason);
            return vec![PromptNotice {
                session_id: info.session_id,
                text,
            }];
        }
        if let Some((session_id, _)) = self.answered_requests.remove(id) {
            self.answered_order.retain(|existing| existing != id);
            if matches!(reason, "timeout" | "interrupted") {
                return vec![PromptNotice {
                    session_id,
                    text: format!(
                        "Hermes did not accept the answer to {}; the agent continued without it",
                        sanitize(method)
                    ),
                }];
            }
        }
        Vec::new()
    }

    /// Remember a cancelled id so a register that arrives afterwards is not shown.
    fn remember_cancelled(&mut self, id: &str) {
        if id.is_empty() || self.was_cancelled(id) {
            return;
        }
        if self.cancelled_requests.len() == CANCELLED_REQUEST_LIMIT {
            self.cancelled_requests.pop_front();
        }
        self.cancelled_requests.push_back(id.to_owned());
    }

    fn was_cancelled(&self, id: &str) -> bool {
        self.cancelled_requests.iter().any(|kept| kept == id)
    }

    pub fn on_connection_replaced(&mut self, generation: u64) -> Vec<PromptNotice> {
        if self.generation.is_some_and(|current| generation <= current) {
            return Vec::new();
        }
        self.generation = Some(generation);
        self.answered_requests.clear();
        self.cancelled_requests.clear();
        self.answered_order.clear();
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

    /// Remove live masked duplicates without answering the recovered requests.
    pub fn install_recovered(&mut self, ids: &[String]) {
        for (_, responder) in self
            .masked
            .extract_if(.., |(info, _)| ids.contains(&info.id))
        {
            responder.ignore();
        }
    }

    /// Request an answer without retaining its value in session state.
    pub fn answer_recovered(request_id: &str, value: Zeroizing<String>) -> AppEffect {
        AppEffect::AnswerRecovered {
            request_id: request_id.to_owned(),
            value,
        }
    }

    pub async fn answer_masked(
        &mut self,
        target: &LiveMaskedTarget,
        value: Zeroizing<String>,
    ) -> Result<(), RespondError> {
        if target.service_id != self.service_id {
            return Err(RespondError::Cancelled);
        }
        let Some(index) = self.masked_index(target) else {
            return Err(RespondError::Cancelled);
        };
        if self
            .generation
            .is_some_and(|current| self.masked[index].0.generation != current)
            || target.generation != self.masked[index].0.generation
        {
            let (_, responder) = self.masked.remove(index);
            responder.ignore();
            return Err(RespondError::Cancelled);
        }
        // Borrow the zeroing buffer; the transport alone serializes the response value.
        let result = self.masked[index]
            .1
            .respond(&BTreeMap::from([("value", value.as_str())]))
            .await;
        if result.is_ok() {
            let (info, responder) = self.masked.remove(index);
            self.remember_answered(info.id, info.session_id, masked_method(info.kind));
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

    fn masked_index(&self, target: &LiveMaskedTarget) -> Option<usize> {
        self.masked.iter().position(|(info, _)| {
            info.service_id == target.service_id
                && info.token == target.token
                && info.instance_id == target.instance_id
                && info.generation == target.generation
                && info.session_id == target.session_id
        })
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
            Effect::RespondConnection {
                session_id,
                op_id,
                result,
            } => {
                use hermes_contract::schemas::{
                    ConnectionRespondParams, Owner, SessionOwner, SessionOwnerType,
                };
                let failed = || Executed {
                    events: Vec::new(),
                    notices: vec![PromptNotice {
                        session_id: Some(session_id.as_str().to_owned()),
                        text: "could not answer the connection request".to_owned(),
                    }],
                };
                let Ok(owner_id) = session_id.as_str().parse() else {
                    return failed();
                };
                let reply = client
                    .call::<connection_respond::Marker>(ConnectionRespondParams {
                        op_id,
                        owner: Owner::SessionOwner(SessionOwner {
                            session_id: owner_id,
                            type_: SessionOwnerType::Session,
                        }),
                        profile: None,
                        result,
                    })
                    .await;
                return match reply {
                    Ok(reply) if reply.status == "ok" => Executed::default(),
                    _ => failed(),
                };
            }
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
                self.answered_prompts.insert(key.clone());
                // The accepted answer comes first, so the session seals the prompt before a
                // note's follow-up prompt starts.
                Executed {
                    events: std::iter::once(Event::AnswerAccepted { key })
                        .chain(note.into_iter().map(|text| Event::Submit {
                            display: text.clone(),
                            text,
                            submission_id: (self.next_submission_id)(),
                            attachments: Vec::new(),
                        }))
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

const MAX_OPEN_MASKED: usize = 64;
const MAX_ANSWERED_REQUESTS: usize = 256;
const MAX_MASKED_FIELD_BYTES: usize = 4096;
const FIELD_SUFFIX: &str = "… (truncated)";

impl PromptService {
    fn remember_answered(&mut self, id: String, session_id: Option<String>, method: &'static str) {
        if let Some(stored) = self.answered_requests.get_mut(&id) {
            *stored = (session_id, method);
            return;
        }
        while self.answered_requests.len() >= MAX_ANSWERED_REQUESTS {
            let Some(old) = self.answered_order.pop_front() else {
                break;
            };
            self.answered_requests.remove(&old);
        }
        self.answered_order.push_back(id.clone());
        self.answered_requests.insert(id, (session_id, method));
    }
}

fn bounded_field(value: String) -> String {
    if value.len() <= MAX_MASKED_FIELD_BYTES {
        return value;
    }
    let end = floor_char_boundary(&value, MAX_MASKED_FIELD_BYTES - FIELD_SUFFIX.len());
    let mut out = String::with_capacity(end + FIELD_SUFFIX.len());
    out.push_str(&value[..end]);
    out.push_str(FIELD_SUFFIX);
    out
}

fn floor_char_boundary(value: &str, mut index: usize) -> usize {
    if index > value.len() {
        return value.len();
    }
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
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

#[cfg(test)]
mod tests {
    use talaria_core::session::{Cell, Event, PromptState, SessionState, Settlement, update};

    #[test]
    fn cancel_timeout_settles_a_recovered_sudo_card() {
        let snapshot: hermes_contract::schemas::CanonicalSnapshot =
            serde_json::from_value(serde_json::json!({
                "session_id": "sess-1",
                "stored_session_id": "stored-1",
                "messages": [],
                "message_count": 0,
                "running": false,
                "authority_epoch": 1,
                "replay_epoch": "epoch-1",
                "last_sequence": 0,
                "subscription_id": "sub-1",
                "revision": 1,
                "execution_generation": 1,
                "pending": [],
                "prompts": [],
                "open_requests": [
                    {"id": "req-1", "method": "sudo", "params": {}}
                ],
                "info": {"desktop_protocol": "1"},
            }))
            .expect("snapshot");
        let mut state = SessionState::from_snapshot(snapshot);
        let _effects = update(
            &mut state,
            Event::ServerRequestCancelled {
                id: "req-1".to_owned(),
            },
        );
        assert!(
            !state
                .open_prompts()
                .any(|cell| cell.key.prompt_id == "req-1"),
            "cancelled card must no longer be open"
        );
        let card = state
            .cells()
            .iter()
            .find_map(|cell| match cell {
                Cell::Prompt(prompt) if prompt.key.prompt_id == "req-1" => Some(prompt),
                _ => None,
            })
            .expect("cancelled card must remain in cells");
        assert_eq!(card.state, PromptState::Settled(Settlement::Elsewhere));
        assert_eq!(
            super::cancel_reason_notice("timeout"),
            "timed out; the agent continued without it"
        );
    }

    #[test]
    fn blank_session_id_is_not_stored_as_the_foreground_session() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            for session_id in [
                Some(String::new()),
                Some("   ".to_owned()),
                Some("\t".to_owned()),
                None,
            ] {
                let mut service =
                    super::PromptService::new(Box::new(|| "talaria-blank-1".to_owned()));
                service
                    .on_server_request(hermes_client::ServerRequest {
                        generation: hermes_client::Generation(1),
                        id: "srq-blank".into(),
                        session_id: session_id.clone(),
                        request: hermes_contract::server_requests::ServerRequest::Sudo(
                            hermes_contract::schemas::SudoRequestParams {
                                command: "id".into(),
                                session_id: "   ".into(),
                            },
                        ),
                        responder: hermes_client::Responder::detached(std::sync::Arc::new(
                            std::sync::atomic::AtomicU64::new(0),
                        )),
                    })
                    .await;
                let stored = service.masked_requests();
                assert_eq!(stored.len(), 1);
                assert_eq!(
                    stored[0].session_id, None,
                    "blank session id {:?} must not be stored as a named session",
                    session_id
                );
            }
        });
    }
}
