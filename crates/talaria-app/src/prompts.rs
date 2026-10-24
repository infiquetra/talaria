//! The protocol side of prompt cards: answering server requests and respond calls.

use hermes_client::{Client, RespondError, ServerRequest};
use talaria_core::session::{Effect, Event, PromptKey, SessionState};
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
    let _ = method;
    todo!("prompts")
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
    _next_submission_id: Box<dyn FnMut() -> String + Send>,
}

impl PromptService {
    pub fn new(next_submission_id: Box<dyn FnMut() -> String + Send>) -> Self {
        Self {
            _next_submission_id: next_submission_id,
        }
    }

    pub async fn on_server_request(&mut self, req: ServerRequest) -> Vec<PromptNotice> {
        let _ = req;
        todo!("prompts")
    }

    pub fn on_request_cancel(&mut self, id: &str, method: &str, reason: &str) -> Vec<PromptNotice> {
        let _ = (id, method, reason);
        todo!("prompts")
    }

    pub fn on_connection_replaced(&mut self, generation: u64) -> Vec<PromptNotice> {
        let _ = generation;
        todo!("prompts")
    }

    pub async fn answer_masked(
        &mut self,
        request_id: &str,
        value: Zeroizing<String>,
    ) -> Result<(), RespondError> {
        let _ = (request_id, value);
        todo!("prompts")
    }

    pub fn masked_requests(&self) -> Vec<MaskedRequestInfo> {
        todo!("prompts")
    }

    pub fn set_note(&mut self, key: &PromptKey, note: String) {
        let _ = (key, note);
        todo!("prompts")
    }

    pub async fn execute(&mut self, client: &Client, effect: Effect) -> Executed {
        let _ = (client, effect);
        todo!("prompts")
    }

    pub fn on_prompts_changed(&mut self, open: &[PromptKey]) -> Vec<PromptNotice> {
        let _ = open;
        todo!("prompts")
    }

    pub async fn deny_before_interrupt(
        &mut self,
        client: &Client,
        state: &SessionState,
        session_id: &str,
        wait: std::time::Duration,
    ) -> Vec<PromptNotice> {
        let _ = (client, state, session_id, wait);
        todo!("prompts")
    }
}
