use super::{AdmissionState, AttentionCause, JournalError, Resolution, ResolvedLaunch, corrupt};
use crate::engine::{AgentOutcome, CallId, CutShort, EngineEvent, RunOutcomeKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlobRef {
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecordLine {
    pub v: u32,
    pub seq: u64,
    pub at_ms: i64,
    #[serde(flatten)]
    pub record: Record,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CallOutcome {
    Completed {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "present_value"
        )]
        value: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value_ref: Option<BlobRef>,
        /// Present when Hermes stopped the agent at its step cap and its reply was kept.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cut_short: Option<CutShort>,
    },
    Failed {
        reason: String,
    },
    FailedWithReport {
        reason: String,
        path: String,
        report_ref: BlobRef,
    },
    Skipped,
    /// The run's budget ended the call; a resume runs it again.
    Stopped {
        reason: String,
    },
}
fn present_value<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Ok(Some(Value::deserialize(d)?))
}
impl CallOutcome {
    pub fn from_agent(outcome: AgentOutcome) -> Self {
        match outcome {
            AgentOutcome::Completed(value) => Self::Completed {
                value: Some(value),
                value_ref: None,
                cut_short: None,
            },
            AgentOutcome::CutShort { value, cut_short } => Self::Completed {
                value: Some(value),
                value_ref: None,
                cut_short: Some(cut_short),
            },
            AgentOutcome::Failed { reason } | AgentOutcome::Abort { reason } => Self::Failed {
                reason: super::cut(&reason, 4096),
            },
            AgentOutcome::FailedWithReport { reason, report, .. } => Self::FailedWithReport {
                reason: super::cut(&reason, 4096),
                path: report.path,
                report_ref: BlobRef {
                    sha256: report.sha256,
                    bytes: report.bytes,
                },
            },
            AgentOutcome::Skipped => Self::Skipped,
            AgentOutcome::Stopped { reason } => Self::Stopped {
                reason: super::cut(&reason, 4096),
            },
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Completed { .. } => "completed",
            Self::Failed { .. } | Self::FailedWithReport { .. } => "failed",
            Self::Skipped => "skipped",
            Self::Stopped { .. } => "stopped",
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FinishSummary {
    pub completed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub parked: usize,
    pub unfinished: usize,
    /// Calls the budget refused before they started, in the latest execution.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub refused: usize,
    /// Calls the budget ended after they started.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub stopped: usize,
}
fn is_zero(n: &usize) -> bool {
    *n == 0
}
// Keep public constructors compatible while the wire format grows optional fields.
macro_rules! records {
    ($($variant:ident { $($(#[$attribute:meta])* $field:ident: $ty:ty),* $(,)? }),* $(,)?) => {
        #[derive(Clone, Debug, PartialEq)]
        pub enum Record { $($variant { $($field: $ty),* }),* }
        #[derive(Serialize, Deserialize)]
        #[serde(tag = "t", rename_all = "snake_case")]
        enum RecordWire { $($variant { $($(#[$attribute])* $field: $ty),* }),* }
        impl From<Record> for RecordWire {
            fn from(record: Record) -> Self { match record {
                $(Record::$variant { $($field),* } => Self::$variant { $($field),* }),*
            } }
        }
        impl From<RecordWire> for Record {
            fn from(record: RecordWire) -> Self { match record {
                $(RecordWire::$variant { $($field),* } => Self::$variant { $($field),* }),*
            } }
        }
    };
}
records! {
    RunStarted {
        run_id: String,
        script_sha256: String,
    },
    RunResumed {
        script_sha256: String,
        resume_count: u32,
    },
    Engine {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        event: Option<EngineEvent>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        event_ref: Option<BlobRef>,
    },
    CallIntent {
        key: String,
        attempt: u32,
        call_id: CallId,
        label: String,
        phase: Option<String>,
        workflow_path: Vec<String>,
        launch: ResolvedLaunch,
        request_id: String,
        // The starting file hash, or "absent", for a call that declared a report.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        report_baseline: Option<String>,
    },
    SessionBound {
        key: String,
        attempt: u32,
        session_id: String,
        stored_session_id: String,
    },
    TurnIntent {
        key: String,
        attempt: u32,
        turn: u32,
        submission_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text_ref: Option<BlobRef>,
    },
    TurnAdmitted {
        key: String,
        attempt: u32,
        turn: u32,
        admission_id: String,
        status: AdmissionState,
        outcome: Option<String>,
        execution_generation: Option<i64>,
        authority_epoch: i64,
    },
    TurnSettled {
        key: String,
        attempt: u32,
        turn: u32,
        admission_id: String,
        outcome: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        final_text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        final_text_ref: Option<BlobRef>,
        result_meta: Value,
        usage: Option<Value>,
    },
    CallAttention {
        key: String,
        attempt: u32,
        episode: u32,
        cause: AttentionCause,
        turn: Option<u32>,
        admission_id: Option<String>,
        execution_generation: Option<i64>,
        detail: String,
        /// The admissions of other clients the port had already seen on the session when a
        /// stopped call parked. Its next reply must be a later turn than any of them.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        seen_admissions: Vec<String>,
    },
    CallResolved {
        key: String,
        attempt: u32,
        episode: u32,
        resolution: Resolution,
        source: ResolutionSource,
    },
    CallFinished {
        key: String,
        attempt: u32,
        outcome: CallOutcome,
        turns: u32,
    },
    // An operator explicitly authorized another attempt despite the kept report.
    CallReportRestartRequested { key: String, attempt: u32 },
    RunFinished {
        outcome: RunOutcomeKind,
        summary: FinishSummary,
    },
    CallRetryFinished { key: String, attempt: u32, outcome: CallOutcome, turns: u32 },
}
impl Serialize for Record {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Self::CallRetryFinished {
            key,
            attempt,
            outcome,
            turns,
        } = self
        {
            #[derive(Serialize)]
            struct RetryFinish<'a> {
                t: &'static str,
                key: &'a str,
                attempt: u32,
                outcome: &'a CallOutcome,
                turns: u32,
                retry: bool,
            }
            return RetryFinish {
                t: "call_finished",
                key,
                attempt: *attempt,
                outcome,
                turns: *turns,
                retry: true,
            }
            .serialize(serializer);
        }
        RecordWire::from(self.clone()).serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Record {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut value = Value::deserialize(deserializer)?;
        if value["t"] == "call_retry_finished" {
            return Err(serde::de::Error::custom("unknown record type"));
        }
        if value["t"] == "call_finished" && value.get("retry").is_some() {
            let retry = value["retry"]
                .as_bool()
                .ok_or_else(|| serde::de::Error::custom("retry must be a boolean"))?;
            if retry {
                value["t"] = Value::String("call_retry_finished".into());
            }
        }
        if value["t"] == "call_retry_finished"
            && !matches!(value["outcome"]["kind"].as_str(), Some("failed"))
        {
            return Err(serde::de::Error::custom("a retry finish must fail"));
        }
        serde_json::from_value::<RecordWire>(value)
            .map(Into::into)
            .map_err(serde::de::Error::custom)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionSource {
    Operator,
    Policy,
}
impl Record {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::RunStarted { .. } => "run_started",
            Self::RunResumed { .. } => "run_resumed",
            Self::Engine { .. } => "engine",
            Self::CallIntent { .. } => "call_intent",
            Self::SessionBound { .. } => "session_bound",
            Self::TurnIntent { .. } => "turn_intent",
            Self::TurnAdmitted { .. } => "turn_admitted",
            Self::TurnSettled { .. } => "turn_settled",
            Self::CallAttention { .. } => "call_attention",
            Self::CallResolved { .. } => "call_resolved",
            Self::CallFinished { .. } | Self::CallRetryFinished { .. } => "call_finished",
            Self::CallReportRestartRequested { .. } => "call_report_restart_requested",
            Self::RunFinished { .. } => "run_finished",
        }
    }
    pub fn identity(&self) -> Option<(&str, u32)> {
        match self {
            Self::CallIntent { key, attempt, .. }
            | Self::SessionBound { key, attempt, .. }
            | Self::TurnIntent { key, attempt, .. }
            | Self::TurnAdmitted { key, attempt, .. }
            | Self::TurnSettled { key, attempt, .. }
            | Self::CallAttention { key, attempt, .. }
            | Self::CallResolved { key, attempt, .. }
            | Self::CallFinished { key, attempt, .. }
            | Self::CallReportRestartRequested { key, attempt, .. }
            | Self::CallRetryFinished { key, attempt, .. } => Some((key, *attempt)),
            _ => None,
        }
    }
    pub(crate) fn refs(&self) -> Vec<&BlobRef> {
        match self {
            Self::Engine { event_ref, .. } => event_ref.iter().collect(),
            Self::TurnIntent { text_ref, .. } => text_ref.iter().collect(),
            Self::TurnSettled { final_text_ref, .. } => final_text_ref.iter().collect(),
            Self::CallFinished {
                outcome: CallOutcome::Completed { value_ref, .. },
                ..
            } => value_ref.iter().collect(),
            Self::CallFinished {
                outcome: CallOutcome::FailedWithReport { report_ref, .. },
                ..
            } => vec![report_ref],
            _ => Vec::new(),
        }
    }
    pub(crate) fn validate(&self, line: u64) -> Result<(), JournalError> {
        let valid = match self {
            Self::RunStarted { script_sha256, .. } | Self::RunResumed { script_sha256, .. } => {
                super::valid_sha256(script_sha256)
            }
            Self::Engine { event, event_ref } => event.is_some() != event_ref.is_some(),
            Self::TurnIntent {
                text,
                text_ref,
                turn,
                ..
            } => *turn < super::MAX_TURNS_PER_CALL && text.is_some() != text_ref.is_some(),
            Self::TurnSettled {
                final_text,
                final_text_ref,
                turn,
                ..
            } => {
                *turn < super::MAX_TURNS_PER_CALL
                    && !(final_text.is_some() && final_text_ref.is_some())
            }
            Self::CallFinished {
                outcome:
                    CallOutcome::Completed {
                        value, value_ref, ..
                    },
                turns,
                ..
            } => *turns <= 2 && value.is_some() != value_ref.is_some(),
            Self::CallFinished {
                outcome: CallOutcome::FailedWithReport { .. },
                turns,
                ..
            } => (1..=2).contains(turns),
            Self::CallFinished { turns, .. } => *turns <= 2,
            Self::CallRetryFinished { outcome, turns, .. } => {
                *turns <= 2 && matches!(outcome, CallOutcome::Failed { .. })
            }
            _ => true,
        };
        if !valid {
            return Err(corrupt(line, "invalid payload or turn count"));
        }
        if let Some((key, _)) = self.identity()
            && !super::run_dir::valid_key(key)
        {
            return Err(corrupt(line, "invalid key"));
        }
        Ok(())
    }
}
