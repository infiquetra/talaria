//! Approval-mode and session-bypass state for the permissions work of #237.
//!
//! The reducer and its helpers are stubs: every behaviour panics with
//! `unimplemented!()` until the green implementation lands.

use std::time::Instant;

use crate::commands::CommandOutcome;
use crate::session::NoticeKind;

/// The profile's approval mode as Hermes reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalMode {
    /// Hermes asks before running commands it would normally ask about.
    Manual,
    /// Hermes decides which commands need approval.
    Smart,
    /// Approvals are off for every session of the profile.
    Off,
}

impl ApprovalMode {
    /// Parses the Hermes wire words `manual`, `smart` and `off` only.
    pub fn parse(word: &str) -> Option<ApprovalMode> {
        let _ = word;
        unimplemented!("not yet implemented")
    }

    /// The wire word for this mode.
    pub fn as_wire(&self) -> &'static str {
        unimplemented!("not yet implemented")
    }
}

/// How much Talaria knows about one Hermes setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading<T> {
    /// Nothing has been asked yet.
    NotRead,
    /// A read is in flight.
    Pending,
    /// Hermes confirmed this value.
    Known(T),
    /// The last read failed.
    Failed,
}

/// The status-line tone of the approvals segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalTone {
    /// The quiet tone for the on states.
    Muted,
    /// The warning tone for off and unknown.
    Warning,
}

/// The approvals segment text and tone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalSegmentText {
    /// The full segment text, for example `approvals: smart`.
    pub text: String,
    /// The tone the segment is drawn with.
    pub tone: ApprovalTone,
}

/// One parsed `/permissions` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PermissionsCommand {
    /// Show both settings.
    Show,
    /// Set the profile's mode for every session of the profile.
    Profile(ApprovalMode),
    /// Approvals off for this session only.
    SessionOff,
    /// This session follows the profile again.
    SessionDefault,
}

/// Parses the text after `/permissions`; the error is the usage line.
pub fn parse_permissions_command(args: &str) -> Result<PermissionsCommand, String> {
    let _ = args;
    unimplemented!("not yet implemented")
}

/// Why a permissions call did not produce a usable value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallFailure {
    /// Hermes refused the call.
    Refused {
        /// The refusal reason.
        reason: String,
        /// The JSON-RPC error code.
        rpc_code: i64,
        /// The JSON-RPC error message.
        message: String,
    },
    /// Hermes never answered, or the connection dropped first.
    Unknown,
    /// Hermes answered something Talaria could not read.
    Unreadable,
    /// The call was never sent.
    NotSent,
}

/// How a Hermes-side slash command finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HermesCommandResult {
    /// The command ran on the gateway.
    Ran,
    /// The command did not run.
    NotRun,
    /// Talaria could not tell whether the command ran.
    Unknown,
}

/// Maps a finished Hermes command to the reread input, when the command can change
/// approvals: `/yolo` and `/approvals` reread on `Output`, `Submit`, `OutcomeUnknown`
/// and `Failed`; anything else maps to nothing.
pub fn hermes_command_finished(
    session_id: &str,
    name: &str,
    outcome: &CommandOutcome,
) -> Option<PermissionsInput> {
    let _ = (session_id, name, outcome);
    unimplemented!("not yet implemented")
}

/// One input to the permissions reducer.
#[derive(Clone, Debug, PartialEq)]
pub enum PermissionsInput {
    /// The connection opened; the profile mode is read again.
    ConnectionOpened,
    /// A session is now in front.
    SessionOpened {
        /// The session now in front.
        session_id: String,
    },
    /// The foreground session closed.
    SessionClosed,
    /// A turn finished on a session.
    TurnCompleted {
        /// The session whose turn finished.
        session_id: String,
    },
    /// A Hermes-side slash command finished.
    HermesCommandFinished {
        /// The session the command ran for.
        session_id: String,
        /// The command name.
        name: String,
        /// How the command finished.
        result: HermesCommandResult,
    },
    /// The approvals toggle key was pressed.
    ToggleKey,
    /// A parsed `/permissions` command line.
    Command {
        /// The parsed command.
        command: PermissionsCommand,
        /// When the command was entered.
        now: Instant,
    },
    /// The reply of a profile-mode read.
    ProfileModeRead {
        /// The generation the read was sent with.
        generation: u64,
        /// The confirmed mode or the failure.
        result: Result<ApprovalMode, CallFailure>,
    },
    /// The reply of a session-bypass read.
    SessionOffRead {
        /// The session the read was for.
        session_id: String,
        /// The generation the read was sent with.
        generation: u64,
        /// The confirmed bypass or the failure.
        result: Result<bool, CallFailure>,
    },
    /// The reply of a session-bypass write.
    SessionOffSet {
        /// The session the write was for.
        session_id: String,
        /// The generation the write was sent with.
        generation: u64,
        /// Whether the write asked for the bypass on.
        sent: bool,
        /// The confirmed bypass or the failure.
        result: Result<bool, CallFailure>,
    },
    /// The reply of a profile-mode write.
    ProfileModeSet {
        /// The generation the write was sent with.
        generation: u64,
        /// The mode the write asked for.
        sent: ApprovalMode,
        /// The confirmed mode or the failure.
        result: Result<ApprovalMode, CallFailure>,
    },
}

/// One effect the permissions reducer emits.
#[derive(Clone, Debug, PartialEq)]
pub enum PermissionsEffect {
    /// Read the profile's approval mode.
    ReadProfileMode {
        /// The generation of this read.
        generation: u64,
    },
    /// Read the session's bypass.
    ReadSessionOff {
        /// The session to read.
        session_id: String,
        /// The generation of this read.
        generation: u64,
    },
    /// Write the session's bypass.
    SetSessionOff {
        /// The session to change.
        session_id: String,
        /// The generation of this write.
        generation: u64,
        /// Whether the bypass should be on.
        off: bool,
    },
    /// Write the profile's approval mode.
    SetProfileMode {
        /// The generation of this write.
        generation: u64,
        /// The mode to set.
        mode: ApprovalMode,
    },
    /// Show a notice to the developer.
    Notice {
        /// The notice kind.
        kind: NoticeKind,
        /// The notice text.
        text: String,
    },
    /// The screen must be redrawn.
    Redraw,
}

/// The permissions reducer state.
#[allow(dead_code)]
pub struct PermissionsState {
    profile_name: String,
    session_id: Option<String>,
    profile: Reading<ApprovalMode>,
    session_off: Reading<bool>,
    next_generation: u64,
    profile_read_generation: u64,
    session_read_generation: u64,
    profile_floor: u64,
    session_floor: u64,
    reread_profile: bool,
    reread_session: bool,
    set_in_flight: bool,
    wanted: Option<bool>,
    profile_set_in_flight: bool,
    off_confirm_until: Option<Instant>,
    read_failure_noticed: bool,
    profile_unknown: bool,
    session_unknown: bool,
}

impl PermissionsState {
    /// A fresh state for the named profile: nothing read, no session.
    pub fn new(profile_name: String) -> PermissionsState {
        PermissionsState {
            profile_name,
            session_id: None,
            profile: Reading::NotRead,
            session_off: Reading::NotRead,
            next_generation: 0,
            profile_read_generation: 0,
            session_read_generation: 0,
            profile_floor: 0,
            session_floor: 0,
            reread_profile: false,
            reread_session: false,
            set_in_flight: false,
            wanted: None,
            profile_set_in_flight: false,
            off_confirm_until: None,
            read_failure_noticed: false,
            profile_unknown: false,
            session_unknown: false,
        }
    }

    /// Reduces one input into effects. `toggle_label` names the toggle key.
    pub fn update(
        &mut self,
        input: PermissionsInput,
        toggle_label: Option<&str>,
    ) -> Vec<PermissionsEffect> {
        let _ = (&input, toggle_label);
        unimplemented!("not yet implemented")
    }

    /// The approvals segment, or nothing when it stays hidden.
    pub fn segment(&self) -> Option<ApprovalSegmentText> {
        unimplemented!("not yet implemented")
    }

    /// The session-bypass reading.
    pub fn session_off(&self) -> &Reading<bool> {
        unimplemented!("not yet implemented")
    }
}
