//! Decode wire envelopes while keeping unknown event and request payloads intact.

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use std::{error::Error, fmt};

pub use crate::generated::{events::Notification, server_requests::ServerRequest};

/// A known payload or its envelope failed validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
    /// The event's wire type, or the server request's method; empty if unavailable.
    pub event_type: String,
    pub reason: String,
    pub missing_fields: Vec<String>,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.event_type, self.reason)
    }
}

impl Error for DecodeError {}

#[derive(Clone, Debug, PartialEq)]
pub struct EventFrame {
    pub session_id: Option<String>,
    pub seq: Option<u64>,
    pub replay_epoch: Option<String>,
    pub authority_epoch: Option<i64>,
    pub execution_generation: Option<i64>,
    pub admission_id: Option<String>,
    pub notification: Notification,
}

impl EventFrame {
    /// Decode the whole `params` object of an `event` notification.
    pub fn from_params(params: &Value) -> Result<Self, DecodeError> {
        let object = params
            .as_object()
            .ok_or_else(|| error("", "event params must be an object"))?;
        let name = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| error("", "event type must be a string"))?;
        let mut frame = Self {
            session_id: envelope_field(object, "session_id", name)?,
            seq: envelope_field(object, "seq", name)?,
            replay_epoch: envelope_field(object, "replay_epoch", name)?,
            authority_epoch: envelope_field(object, "authority_epoch", name)?,
            execution_generation: envelope_field(object, "execution_generation", name)?,
            admission_id: envelope_field(object, "admission_id", name)?,
            notification: Notification::Unknown(params.clone()),
        };
        if let Some(required) = required_fields(crate::generated::events::REQUIRED_FIELDS, name) {
            let payload = object_payload(object.get("payload"), name, required)?;
            frame.notification = decode_notification(name, &payload, params)
                .map_err(|failure| error(name, failure.to_string()))?;
        }
        Ok(frame)
    }

    /// The original wire name, including the name of an unknown notification.
    pub fn type_name(&self) -> Option<&str> {
        notification_type_name(&self.notification)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ServerRequestFrame {
    pub id: Value,
    pub request: ServerRequest,
}

impl ServerRequestFrame {
    /// Decode a whole JSON-RPC server request, preserving its identifier as JSON.
    pub fn from_request(request: &Value) -> Result<Self, DecodeError> {
        let object = request
            .as_object()
            .ok_or_else(|| error("", "server request must be an object"))?;
        let name = object
            .get("method")
            .and_then(Value::as_str)
            .ok_or_else(|| error("", "server request method must be a string"))?;
        let params = object.get("params").unwrap_or(&Value::Null);
        let decoded = if let Some(required) =
            required_fields(crate::generated::server_requests::REQUIRED_FIELDS, name)
        {
            let payload = object_payload(Some(params), name, required)?;
            decode_server_request(name, &payload)
                .map_err(|failure| error(name, failure.to_string()))?
        } else {
            ServerRequest::Unknown(params.clone())
        };
        Ok(Self {
            id: object.get("id").cloned().unwrap_or(Value::Null),
            request: decoded,
        })
    }
}

fn error(name: &str, reason: impl Into<String>) -> DecodeError {
    DecodeError {
        event_type: name.into(),
        reason: reason.into(),
        missing_fields: Vec::new(),
    }
}

fn required_fields<'a>(table: &'a [(&str, &'a [&str])], name: &str) -> Option<&'a [&'a str]> {
    table
        .iter()
        .find_map(|(entry, fields)| (*entry == name).then_some(*fields))
}

fn envelope_field<T: DeserializeOwned>(
    object: &Map<String, Value>,
    key: &str,
    name: &str,
) -> Result<Option<T>, DecodeError> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => serde_json::from_value(value.clone())
            .map(Some)
            .map_err(|failure| error(name, format!("invalid envelope field {key}: {failure}"))),
    }
}

fn object_payload(
    payload: Option<&Value>,
    name: &str,
    required: &[&str],
) -> Result<Value, DecodeError> {
    let payload = match payload {
        None | Some(Value::Null) => Value::Object(Map::new()),
        Some(value) if value.is_object() => value.clone(),
        Some(_) => return Err(error(name, "payload must be an object")),
    };
    let mut missing_fields: Vec<String> = required
        .iter()
        .filter(|field| payload.get(**field).is_none())
        .map(|field| (*field).to_owned())
        .collect();
    missing_fields.sort_unstable();
    if !missing_fields.is_empty() {
        return Err(DecodeError {
            event_type: name.into(),
            reason: format!(
                "missing required payload fields: {}",
                missing_fields.join(", ")
            ),
            missing_fields,
        });
    }
    Ok(payload)
}

// The payload type is inferred from each generated enum variant's constructor.
macro_rules! notifications {
    ($($name:literal => $variant:ident),* $(,)?) => {
        fn decode_notification(name: &str, payload: &Value, params: &Value) -> serde_json::Result<Notification> {
            match name {
                $($name => serde_json::from_value(payload.clone()).map(Notification::$variant),)*
                "voice.interrupted" => Ok(Notification::VoiceInterrupted),
                _ => Ok(Notification::Unknown(params.clone())),
            }
        }

        fn notification_type_name(notification: &Notification) -> Option<&str> {
            match notification {
                $(Notification::$variant(_) => Some($name),)*
                Notification::VoiceInterrupted => Some("voice.interrupted"),
                Notification::Unknown(params) => params.get("type").and_then(Value::as_str),
            }
        }
    };
}

notifications! {
    "agent.print" => AgentPrint,
    "agent.terminal.output" => AgentTerminalOutput,
    "approval.cancelled" => ApprovalCancelled,
    "approval.request" => ApprovalRequest,
    "approval.settled" => ApprovalSettled,
    "background.complete" => BackgroundComplete,
    "billing.step_up.verification" => BillingStepUpVerification,
    "bot_relay.outbox.pending" => BotRelayOutboxPending,
    "browser.controller.cancel" => BrowserControllerCancel,
    "browser.controller.command" => BrowserControllerCommand,
    "browser.progress" => BrowserProgress,
    "btw.complete" => BtwComplete,
    "clarify.request" => ClarifyRequest,
    "clarify.settled" => ClarifySettled,
    "connection.request" => ConnectionRequest,
    "connection.update" => ConnectionUpdate,
    "cron.changed" => CronChanged,
    "display.install.done" => DisplayInstallDone,
    "display.install.log" => DisplayInstallLog,
    "display.lease" => DisplayLease,
    "display.status" => DisplayStatus,
    "error" => Error,
    "free_tier.challenge" => FreeTierChallenge,
    "gateway.ready" => GatewayReady,
    "message.complete" => MessageComplete,
    "message.delta" => MessageDelta,
    "message.interim" => MessageInterim,
    "message.reaction" => MessageReaction,
    "message.start" => MessageStart,
    "moa.aggregating" => MoaAggregating,
    "moa.phase" => MoaPhase,
    "moa.progress" => MoaProgress,
    "moa.reference" => MoaReference,
    "notice" => Notice,
    "notification.clear" => NotificationClear,
    "notification.show" => NotificationShow,
    "onboarding.ended" => OnboardingEnded,
    "pairing.changed" => PairingChanged,
    "pane.reveal" => PaneReveal,
    "pet.changed" => PetChanged,
    "pet.generate.progress" => PetGenerateProgress,
    "pet.hatch.progress" => PetHatchProgress,
    "platforms.changed" => PlatformsChanged,
    "preview.close" => PreviewClose,
    "preview.open" => PreviewOpen,
    "preview.restart.complete" => PreviewRestartComplete,
    "preview.restart.progress" => PreviewRestartProgress,
    "projects.changed" => ProjectsChanged,
    "reaction" => Reaction,
    "reasoning.available" => ReasoningAvailable,
    "reasoning.delta" => ReasoningDelta,
    "request.cancel" => RequestCancel,
    "review.summary" => ReviewSummary,
    "session.control.update" => SessionControlUpdate,
    "session.info" => SessionInfo,
    "session.reclaimed" => SessionReclaimed,
    "session.replay_gap" => SessionReplayGap,
    "session.resume_progress" => SessionResumeProgress,
    "session.title" => SessionTitle,
    "session.usage" => SessionUsage,
    "sessions.changed" => SessionsChanged,
    "setup.ready" => SetupReady,
    "skills.changed" => SkillsChanged,
    "skin.changed" => SkinChanged,
    "status.update" => StatusUpdate,
    "subagent.complete" => SubagentComplete,
    "subagent.progress" => SubagentProgress,
    "subagent.spawn_requested" => SubagentSpawnRequested,
    "subagent.start" => SubagentStart,
    "subagent.thinking" => SubagentThinking,
    "subagent.tool" => SubagentTool,
    "terminal.close" => TerminalClose,
    "thinking.delta" => ThinkingDelta,
    "tip.show" => TipShow,
    "todo.updated" => TodoUpdated,
    "tool.complete" => ToolComplete,
    "tool.generating" => ToolGenerating,
    "tool.output_risk" => ToolOutputRisk,
    "tool.progress" => ToolProgress,
    "tool.start" => ToolStart,
    "voice.partial" => VoicePartial,
    "voice.status" => VoiceStatus,
    "voice.transcript" => VoiceTranscript,
    "wake.detected" => WakeDetected,
}

macro_rules! server_requests {
    ($($name:literal => $variant:ident),* $(,)?) => {
        fn decode_server_request(name: &str, params: &Value) -> serde_json::Result<ServerRequest> {
            match name {
                $($name => serde_json::from_value(params.clone()).map(ServerRequest::$variant),)*
                _ => Ok(ServerRequest::Unknown(params.clone())),
            }
        }
    };
}

server_requests! {
    "approval" => Approval,
    "clarify" => Clarify,
    "display.install.sudo" => DisplayInstallSudo,
    "preview.act" => PreviewAct,
    "preview.read" => PreviewRead,
    "secret" => Secret,
    "setup_beat" => SetupBeat,
    "sudo" => Sudo,
    "terminal.read" => TerminalRead,
    "theme.apply" => ThemeApply,
    "theme.pick" => ThemePick,
    "tour" => Tour,
    "vault.code" => VaultCode,
    "vault.save_login" => VaultSaveLogin,
    "vault.unlock_prompt" => VaultUnlockPrompt,
    "window.read" => WindowRead,
}
