use serde::Serialize;
use std::collections::BTreeSet;
use std::time::Duration;

pub const MIN_HERMES: (u64, u64, u64) = (0, 22, 3);
pub const RESTART_DEADLINE: Duration = Duration::from_secs(90);
pub const PROBE_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Skip,
    Info,
    Ok,
    Warn,
    Fail,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckId {
    Talaria,
    Gateway,
    HermesVersion,
    Capabilities,
    Profiles,
    Attach,
    RuntimeHealth,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub id: CheckId,
    pub severity: Severity,
    pub summary: String,
    pub details: Vec<String>,
    pub fix: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub profile: String,
    pub checks: Vec<Check>,
}
impl Report {
    pub fn overall(&self) -> Severity {
        Severity::Ok
    }
    pub fn counts(&self) -> (usize, usize, usize) {
        (0, 0, 0)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub tag: String,
    pub commit: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayFacts {
    Ready {
        instance_id: String,
        epoch: u64,
        launch: Option<String>,
    },
    RestartWait {
        instance_id: String,
        epoch: u64,
    },
    NotReady {
        state: String,
        drain_reason: Option<String>,
    },
    Incompatible,
    Unreachable {
        message: String,
        next_step: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityFacts {
    pub code_version: Option<String>,
    pub code_sha: Option<String>,
    pub capabilities: Vec<String>,
    pub served: Vec<String>,
    pub parked: Vec<(String, String)>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachFacts {
    NotTried,
    Attached {
        describe_capabilities: Vec<String>,
        identify_ms: u64,
        ticket_ms: u64,
        upgrade_ms: u64,
        ready_ms: u64,
        capabilities_ms: u64,
        describe_ms: u64,
    },
    Failed {
        message: String,
        next_step: String,
    },
}
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StatusPage {
    pub config_version: Option<i64>,
    pub latest_config_version: Option<i64>,
    pub heartbeat_stale_s: Option<f64>,
    pub active_agents: Option<u64>,
    pub storage_status: Option<String>,
    pub storage_reason: Option<String>,
    pub platforms_status: Option<String>,
    pub platforms_configured: Option<u64>,
    pub platforms_connected: Option<u64>,
    pub dashboard_status: Option<String>,
    pub dashboard_errors: Option<u64>,
    pub dashboard_selftest: Option<String>,
    pub standalone_reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum StatusFacts {
    NotTried,
    NotReady,
    Unreadable { reason: String },
    Read(StatusPage),
}
#[derive(Debug, Clone, PartialEq)]
pub struct DoctorFacts {
    pub talaria_version: String,
    pub pin: Pin,
    pub profile: String,
    pub gateway: GatewayFacts,
    pub identity: Option<IdentityFacts>,
    pub attach: AttachFacts,
    pub status: StatusFacts,
}

pub fn diagnose(facts: &DoctorFacts) -> Report {
    let ids = [
        CheckId::Talaria,
        CheckId::Gateway,
        CheckId::HermesVersion,
        CheckId::Capabilities,
        CheckId::Profiles,
        CheckId::Attach,
        CheckId::RuntimeHealth,
    ];
    Report {
        profile: facts.profile.clone(),
        checks: ids
            .into_iter()
            .map(|id| Check {
                id,
                severity: Severity::Skip,
                summary: String::new(),
                details: Vec::new(),
                fix: None,
            })
            .collect(),
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionVerdict {
    Pinned,
    SameReleaseOtherCommit,
    SameReleaseUnknownCommit,
    OlderThanPin,
    TooOld,
    NewerThanPin,
    Unknown,
}
pub fn compare_hermes_version(
    _code_version: Option<&str>,
    _code_sha: Option<&str>,
    _pin: &Pin,
) -> VersionVerdict {
    VersionVerdict::Unknown
}
pub fn scrub_text(text: &str, _home: Option<&str>) -> String {
    text.to_owned()
}
pub fn render_text(_report: &Report) -> String {
    String::new()
}
pub fn render_json(
    _report: &Report,
    _restart: Option<&RestartOutcome>,
    _pin: &Pin,
    _talaria_version: &str,
) -> String {
    "{}".to_owned()
}
pub fn exit_code(_report: &Report, _restart: Option<&RestartOutcome>) -> i32 {
    99
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkSummary {
    pub chat: u64,
    pub other: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartState {
    Idle,
    Asking {
        old_instance: String,
        profiles: Vec<String>,
    },
    Confirming {
        old_instance: String,
        profiles: Vec<String>,
        work: Option<WorkSummary>,
        restart_selected: bool,
    },
    Requesting {
        old_instance: String,
    },
    Waiting {
        old_instance: String,
        elapsed: Duration,
        unknown_request: bool,
    },
    Done(RestartOutcome),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartOutcome {
    Restarted {
        seconds: u64,
        old_instance: String,
        new_instance: String,
    },
    Refused {
        http_status: Option<u16>,
        detail: String,
    },
    Changed,
    DidNotRestart,
    NotBack,
    NotReady,
    Declined,
    NeedsConfirmation,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartInput {
    Ask {
        gateway_ready: bool,
        instance_id: String,
        profiles: Vec<String>,
    },
    ActiveWork(Result<WorkSummary, ()>),
    Toggle,
    Confirm,
    Cancel,
    Posted(PostResult),
    Tick(Duration),
    Probe(ProbeResult),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostResult {
    Accepted,
    Refused {
        http_status: Option<u16>,
        detail: String,
    },
    Unknown,
    Changed,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeResult {
    Ready { instance_id: String },
    NotAnswering,
    NotReady,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartEffect {
    QueryActiveWork,
    Post { old_instance: String },
    ProbeAfter(Duration),
    RerunChecks,
    Notice(String),
}
pub fn restart_update(
    state: RestartState,
    _input: RestartInput,
) -> (RestartState, Vec<RestartEffect>) {
    (state, Vec::new())
}
pub fn ask_input(_facts: &DoctorFacts) -> RestartInput {
    RestartInput::Ask {
        gateway_ready: false,
        instance_id: String::new(),
        profiles: Vec::new(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DoctorView {
    pub report: Option<Report>,
    pub facts: Option<DoctorFacts>,
    pub running: bool,
    pub selected: usize,
    pub expanded: BTreeSet<CheckId>,
}
impl DoctorView {
    pub fn new() -> Self {
        Self {
            report: None,
            facts: None,
            running: true,
            selected: 0,
            expanded: BTreeSet::new(),
        }
    }
}
impl Default for DoctorView {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorKey {
    Up,
    Down,
    Toggle,
    Rerun,
    Restart,
    Close,
    Yes,
    No,
    Tab,
    Left,
    Right,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoctorEffect {
    Gather,
    AskRestart,
    Restart(RestartInput),
    Close,
}
pub fn doctor_view_update(
    _view: &mut DoctorView,
    _restart: &RestartState,
    _key: DoctorKey,
) -> Vec<DoctorEffect> {
    Vec::new()
}
