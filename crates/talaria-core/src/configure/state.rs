use super::{ApplyStep, ConfigureKey, ConfigureView, Context, Screen, SecretDetector, StepResult};
use crate::session::{Event, PromptKey, SessionState};
use hermes_contract::schemas::CanonicalSnapshot;
use std::time::{Duration, Instant};

pub const MAX_TURNS: u32 = 8;
pub const TURN_TIMEOUT: Duration = Duration::from_secs(300);
pub const QUIT_DEADLINE: Duration = Duration::from_secs(2);
pub const END_REASON: &str = "configure_closed";
pub const ALLOWED_TOOLS: &[&str] = &["clarify"];
pub const SESSION_TOOLSETS: &[&str] = &["clarify", "no_mcp"];
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateSpec {
    pub request_id: String,
    pub toolsets: Vec<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigureInput {
    Open {
        request: String,
        profile: String,
        connected: bool,
        model: Option<(String, String)>,
    },
    Gathered(Box<Context>),
    GatherFailed(String),
    Created(Box<CanonicalSnapshot>),
    CreateRefused {
        field: Option<String>,
        detail: Option<String>,
        message: String,
    },
    Session(Event),
    ClarifyAnswered {
        key: PromptKey,
        answer: String,
    },
    Submitted,
    SubmitFailed(String),
    ServerRequest {
        request_id: String,
        method: String,
    },
    Key(ConfigureKey),
    StepDone {
        index: usize,
        result: StepResult,
    },
    Detached,
    Tick,
    LinkLost,
    LinkBack,
    Quit,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigureEffect {
    Gather,
    CreateSession(CreateSpec),
    Submit {
        session_id: String,
        text: String,
        submission_id: String,
    },
    Interrupt {
        session_id: String,
        execution_generation: i64,
    },
    DenyApproval {
        key: PromptKey,
    },
    RespondClarify {
        key: PromptKey,
        answer: String,
    },
    RefuseServerRequest {
        request_id: String,
    },
    Apply {
        index: usize,
        step: ApplyStep,
    },
    Detach {
        session_id: String,
        subscription_id: String,
    },
    ForgetHistoryLine(String),
    ClearEditor,
    Open(crate::commands::LocalAction),
    Notice(String),
    CloseOverlay,
}
#[derive(Debug)]
pub struct ConfigureState;
impl ConfigureState {
    pub fn new(_detect: SecretDetector) -> Self {
        todo!("configure stub")
    }
    pub fn from_reply(_ctx: Context, _reply_text: &str, _detect: SecretDetector) -> Self {
        todo!("configure stub")
    }
    pub fn update(&mut self, _input: ConfigureInput, _now: Instant) -> Vec<ConfigureEffect> {
        todo!("configure stub")
    }
    pub fn view(&self, _now: Instant) -> ConfigureView {
        todo!("configure stub")
    }
    pub fn next_deadline(&self) -> Option<Instant> {
        todo!("configure stub")
    }
    pub fn session(&self) -> Option<&SessionState> {
        todo!("configure stub")
    }
}
pub fn parse_configure_args(_arg: &str) -> Result<String, &'static str> {
    todo!("configure stub")
}
pub fn screen_action(_screen: Screen, _target: Option<&str>) -> crate::commands::LocalAction {
    todo!("configure stub")
}
