//! Approval-mode and session-bypass state for the permissions work of #237.
//!
//! Hermes holds both the profile's approval mode and each session's bypass.
//! The reducer here only shows what Hermes last confirmed: it never guesses a
//! value, and it says `unknown` when a read or a write cannot be confirmed.
//! It is pure: it has no clock and does no I/O. Time arrives inside inputs,
//! and the app runs the effects it returns.

use std::time::{Duration, Instant};

use crate::commands::CommandOutcome;
use crate::session::NoticeKind;

/// The usage line shown for a `/permissions` line that does not parse.
const USAGE: &str = "Usage: /permissions [manual | smart | off | session off | session default]";

/// How long a first `/permissions off` stays open for its confirming repeat.
const OFF_CONFIRM_WINDOW: Duration = Duration::from_secs(30);

/// The longest unrecognised refusal reason shown to the developer.
const MAX_REASON_CHARS: usize = 80;

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
        match word {
            "manual" => Some(ApprovalMode::Manual),
            "smart" => Some(ApprovalMode::Smart),
            "off" => Some(ApprovalMode::Off),
            _ => None,
        }
    }

    /// The wire word for this mode.
    pub fn as_wire(&self) -> &'static str {
        match self {
            ApprovalMode::Manual => "manual",
            ApprovalMode::Smart => "smart",
            ApprovalMode::Off => "off",
        }
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
    let lowered = args.to_lowercase();
    let words: Vec<&str> = lowered.split_whitespace().collect();
    match words.as_slice() {
        [] => Ok(PermissionsCommand::Show),
        ["session", "off"] => Ok(PermissionsCommand::SessionOff),
        ["session", "default"] => Ok(PermissionsCommand::SessionDefault),
        [word] => ApprovalMode::parse(word)
            .map(PermissionsCommand::Profile)
            .ok_or_else(|| USAGE.to_owned()),
        _ => Err(USAGE.to_owned()),
    }
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
    let name = name.trim().trim_start_matches('/').to_lowercase();
    if !matches!(name.as_str(), "yolo" | "approvals") {
        return None;
    }
    let result = match outcome {
        CommandOutcome::Output { .. } | CommandOutcome::Submit { .. } => HermesCommandResult::Ran,
        CommandOutcome::OutcomeUnknown { .. } | CommandOutcome::Failed { .. } => {
            HermesCommandResult::Unknown
        }
        _ => return None,
    };
    Some(PermissionsInput::HermesCommandFinished {
        session_id: session_id.to_string(),
        name,
        result,
    })
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
pub struct PermissionsState {
    profile_name: String,
    session_id: Option<String>,
    profile: Reading<ApprovalMode>,
    session_off: Reading<bool>,
    /// One counter for every request; it never resets, not even on a session change.
    next_generation: u64,
    /// The generation of the read in flight of each kind, if any.
    profile_read: Option<u64>,
    session_read: Option<u64>,
    /// A reply whose generation is below the floor of its kind is stale.
    profile_floor: u64,
    session_floor: u64,
    /// A trigger arrived while a read of that kind was in flight.
    reread_profile: bool,
    reread_session: bool,
    /// The bypass write in flight: its generation and the value it carries.
    session_set: Option<(u64, bool)>,
    /// The latest intended bypass while a write is in flight.
    wanted: Option<bool>,
    /// The generation of the profile write in flight.
    profile_set: Option<u64>,
    off_confirm_until: Option<Instant>,
    /// The failed-read notice of each kind has already been shown.
    profile_failure_noticed: bool,
    session_failure_noticed: bool,
    /// An outcome that may have changed the setting could not be confirmed;
    /// cleared only by the next successful read of that kind.
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
            profile_read: None,
            session_read: None,
            profile_floor: 0,
            session_floor: 0,
            reread_profile: false,
            reread_session: false,
            session_set: None,
            wanted: None,
            profile_set: None,
            off_confirm_until: None,
            profile_failure_noticed: false,
            session_failure_noticed: false,
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
        let before = self.segment();
        let mut effects = Vec::new();
        self.apply(input, toggle_label, &mut effects);
        if self.segment() != before {
            effects.push(PermissionsEffect::Redraw);
        }
        effects
    }

    /// The approvals segment, or nothing when it stays hidden.
    pub fn segment(&self) -> Option<ApprovalSegmentText> {
        self.session_id.as_ref()?;
        let warning = |text: &str| {
            Some(ApprovalSegmentText {
                text: text.to_owned(),
                tone: ApprovalTone::Warning,
            })
        };
        let muted = |text: &str| {
            Some(ApprovalSegmentText {
                text: text.to_owned(),
                tone: ApprovalTone::Muted,
            })
        };
        if self.profile == Reading::Known(ApprovalMode::Off) {
            return warning("approvals: off");
        }
        if self.session_off == Reading::Known(true) {
            return warning("approvals: off (this session)");
        }
        if self.profile == Reading::Failed
            || self.session_off == Reading::Failed
            || self.profile_unknown
            || self.session_unknown
        {
            return warning("approvals: unknown");
        }
        match (self.profile, self.session_off) {
            (Reading::Known(ApprovalMode::Manual), Reading::Known(false)) => {
                muted("approvals: manual")
            }
            (Reading::Known(ApprovalMode::Smart), Reading::Known(false)) => {
                muted("approvals: smart")
            }
            _ => None,
        }
    }

    /// The session-bypass reading.
    pub fn session_off(&self) -> &Reading<bool> {
        &self.session_off
    }

    fn apply(
        &mut self,
        input: PermissionsInput,
        key: Option<&str>,
        effects: &mut Vec<PermissionsEffect>,
    ) {
        match input {
            PermissionsInput::ConnectionOpened => {
                self.off_confirm_until = None;
                self.request_profile_read(effects);
                self.request_session_read(effects);
            }
            PermissionsInput::SessionOpened { session_id } => {
                self.open_session(session_id, effects);
            }
            PermissionsInput::SessionClosed => self.close_session(),
            PermissionsInput::TurnCompleted { session_id } => {
                if self.is_current(&session_id) {
                    self.request_profile_read(effects);
                    self.request_session_read(effects);
                }
            }
            PermissionsInput::HermesCommandFinished {
                session_id,
                name,
                result,
            } => {
                if self.is_current(&session_id) && (name == "yolo" || name == "approvals") {
                    self.hermes_command_finished(result, effects);
                }
            }
            PermissionsInput::ToggleKey => self.toggle(effects),
            PermissionsInput::Command { command, now } => self.command(command, now, key, effects),
            PermissionsInput::ProfileModeRead { generation, result } => {
                self.profile_read_reply(generation, result, effects);
            }
            PermissionsInput::SessionOffRead {
                session_id,
                generation,
                result,
            } => self.session_read_reply(&session_id, generation, result, effects),
            PermissionsInput::SessionOffSet {
                session_id,
                generation,
                result,
                ..
            } => self.session_set_reply(&session_id, generation, result, key, effects),
            PermissionsInput::ProfileModeSet {
                generation, result, ..
            } => self.profile_set_reply(generation, result, key, effects),
        }
    }

    fn is_current(&self, session_id: &str) -> bool {
        self.session_id.as_deref() == Some(session_id)
    }

    fn take_generation(&mut self) -> u64 {
        let generation = self.next_generation;
        self.next_generation += 1;
        generation
    }

    fn open_session(&mut self, session_id: String, effects: &mut Vec<PermissionsEffect>) {
        self.session_id = Some(session_id);
        self.session_off = Reading::Pending;
        self.session_set = None;
        self.wanted = None;
        self.off_confirm_until = None;
        self.profile_failure_noticed = false;
        self.session_failure_noticed = false;
        self.session_unknown = false;
        // Every reply sent for the previous session is now stale, and so is the
        // profile read that was in flight: the new session asks again.
        self.profile_floor = self.next_generation;
        self.session_floor = self.next_generation;
        self.profile_read = None;
        self.session_read = None;
        self.reread_profile = false;
        self.reread_session = false;
        self.request_profile_read(effects);
        self.request_session_read(effects);
    }

    fn close_session(&mut self) {
        self.session_id = None;
        self.session_off = Reading::NotRead;
        self.session_set = None;
        self.session_read = None;
        self.reread_session = false;
        self.wanted = None;
        self.off_confirm_until = None;
        self.session_failure_noticed = false;
        self.session_unknown = false;
        self.session_floor = self.next_generation;
    }

    fn hermes_command_finished(
        &mut self,
        result: HermesCommandResult,
        effects: &mut Vec<PermissionsEffect>,
    ) {
        match result {
            HermesCommandResult::NotRun => {}
            HermesCommandResult::Ran => {
                self.request_profile_read(effects);
                self.request_session_read(effects);
            }
            HermesCommandResult::Unknown => {
                self.profile = Reading::Pending;
                self.session_off = Reading::Pending;
                self.profile_unknown = true;
                self.session_unknown = true;
                self.request_profile_read(effects);
                self.request_session_read(effects);
            }
        }
    }

    /// Asks for the profile mode. A read already in flight turns the request
    /// into a single re-read after its reply; a write in flight makes it
    /// unnecessary, because the write's reply is newer.
    fn request_profile_read(&mut self, effects: &mut Vec<PermissionsEffect>) {
        if self.profile_read.is_some() {
            self.reread_profile = true;
            return;
        }
        if self.profile_set.is_some() {
            return;
        }
        let generation = self.take_generation();
        self.profile_read = Some(generation);
        effects.push(PermissionsEffect::ReadProfileMode { generation });
    }

    /// Asks for the session bypass, with the same rules as the profile read.
    fn request_session_read(&mut self, effects: &mut Vec<PermissionsEffect>) {
        let Some(session_id) = self.session_id.clone() else {
            return;
        };
        if self.session_read.is_some() {
            self.reread_session = true;
            return;
        }
        if self.session_set.is_some() {
            return;
        }
        let generation = self.take_generation();
        self.session_read = Some(generation);
        effects.push(PermissionsEffect::ReadSessionOff {
            session_id,
            generation,
        });
    }

    /// Sends the re-reads that were requested while reads were in flight, once
    /// both kinds have answered.
    fn run_rereads(&mut self, effects: &mut Vec<PermissionsEffect>) {
        if self.profile_read.is_some() || self.session_read.is_some() {
            return;
        }
        if std::mem::take(&mut self.reread_profile) {
            self.request_profile_read(effects);
        }
        if std::mem::take(&mut self.reread_session) {
            self.request_session_read(effects);
        }
    }

    fn profile_read_reply(
        &mut self,
        generation: u64,
        result: Result<ApprovalMode, CallFailure>,
        effects: &mut Vec<PermissionsEffect>,
    ) {
        if self.profile_read != Some(generation) || generation < self.profile_floor {
            return;
        }
        self.profile_read = None;
        match result {
            Ok(mode) => {
                self.profile = Reading::Known(mode);
                self.profile_unknown = false;
                self.profile_failure_noticed = false;
            }
            Err(failure) => {
                self.profile = Reading::Failed;
                if self.session_id.is_some() && !self.profile_failure_noticed {
                    self.profile_failure_noticed = true;
                    read_failed_notice(&failure, effects);
                }
            }
        }
        self.run_rereads(effects);
    }

    fn session_read_reply(
        &mut self,
        session_id: &str,
        generation: u64,
        result: Result<bool, CallFailure>,
        effects: &mut Vec<PermissionsEffect>,
    ) {
        if !self.is_current(session_id)
            || self.session_read != Some(generation)
            || generation < self.session_floor
        {
            return;
        }
        self.session_read = None;
        match result {
            Ok(off) => {
                self.session_off = Reading::Known(off);
                self.session_unknown = false;
                self.session_failure_noticed = false;
            }
            Err(failure) => {
                self.session_off = Reading::Failed;
                if !self.session_failure_noticed {
                    self.session_failure_noticed = true;
                    read_failed_notice(&failure, effects);
                }
            }
        }
        self.run_rereads(effects);
    }

    /// Sends a bypass write. Any read in flight is now older than the write.
    fn send_session_write(&mut self, off: bool, effects: &mut Vec<PermissionsEffect>) {
        let Some(session_id) = self.session_id.clone() else {
            return;
        };
        let generation = self.take_generation();
        self.session_set = Some((generation, off));
        self.session_floor = generation;
        self.session_read = None;
        self.reread_session = false;
        effects.push(PermissionsEffect::SetSessionOff {
            session_id,
            generation,
            off,
        });
    }

    /// Sends a profile write. Any read in flight is now older than the write.
    fn send_profile_write(&mut self, mode: ApprovalMode, effects: &mut Vec<PermissionsEffect>) {
        let generation = self.take_generation();
        self.profile_set = Some(generation);
        self.profile_floor = generation;
        self.profile_read = None;
        self.reread_profile = false;
        effects.push(PermissionsEffect::SetProfileMode { generation, mode });
    }

    fn toggle(&mut self, effects: &mut Vec<PermissionsEffect>) {
        self.off_confirm_until = None;
        if self.session_id.is_none() {
            notice(effects, NoticeKind::System, no_session_text());
            return;
        }
        if self.profile == Reading::Known(ApprovalMode::Off) {
            let text = format!(
                "Profile {} has approvals off for every session, so this session already \
                 runs without asking. Use /permissions manual or /permissions smart to change \
                 the profile.",
                self.profile_name
            );
            notice(effects, NoticeKind::System, text);
            return;
        }
        let Reading::Known(current) = self.session_off else {
            notice(effects, NoticeKind::System, still_reading_text());
            self.request_session_read(effects);
            return;
        };
        if let Some((_, sent)) = self.session_set {
            // Each press flips the latest intended value; nothing is sent yet.
            self.wanted = Some(!self.wanted.unwrap_or(sent));
            return;
        }
        self.send_session_write(!current, effects);
    }

    fn session_command(&mut self, target: bool, effects: &mut Vec<PermissionsEffect>) {
        if self.session_id.is_none() {
            notice(effects, NoticeKind::System, no_session_text());
            return;
        }
        if !matches!(self.session_off, Reading::Known(_)) {
            notice(effects, NoticeKind::System, still_reading_text());
            self.request_session_read(effects);
            return;
        }
        if self.session_set.is_some() {
            self.wanted = Some(target);
            return;
        }
        // Sent even when the reading already equals the target: the reading may
        // be stale, and writing the same value is harmless.
        self.send_session_write(target, effects);
    }

    fn command(
        &mut self,
        command: PermissionsCommand,
        now: Instant,
        key: Option<&str>,
        effects: &mut Vec<PermissionsEffect>,
    ) {
        // Only a second `off` inside the window keeps the confirmation open.
        if command != PermissionsCommand::Profile(ApprovalMode::Off) {
            self.off_confirm_until = None;
        }
        match command {
            PermissionsCommand::Show => {
                let text = self.show_text(key);
                notice(effects, NoticeKind::System, text);
            }
            PermissionsCommand::SessionOff => self.session_command(true, effects),
            PermissionsCommand::SessionDefault => self.session_command(false, effects),
            PermissionsCommand::Profile(mode) => self.profile_command(mode, now, effects),
        }
    }

    fn profile_command(
        &mut self,
        mode: ApprovalMode,
        now: Instant,
        effects: &mut Vec<PermissionsEffect>,
    ) {
        if self.profile_set.is_some() {
            let text = "A change to the profile's approval mode is still being applied. Try \
                        again in a moment.";
            notice(effects, NoticeKind::System, text);
            return;
        }
        if mode == ApprovalMode::Off {
            let confirmed = self.off_confirm_until.is_some_and(|until| now < until);
            if !confirmed {
                self.off_confirm_until = now.checked_add(OFF_CONFIRM_WINDOW);
                let text = format!(
                    "This turns approvals off for every session of profile {}, background \
                     sessions included. Run /permissions off again within 30 seconds to \
                     confirm.",
                    self.profile_name
                );
                notice(effects, NoticeKind::Warning, text);
                return;
            }
            self.off_confirm_until = None;
        }
        // Sent even when the reading already equals the mode, for the same
        // reason as the bypass write.
        self.send_profile_write(mode, effects);
    }

    fn session_set_reply(
        &mut self,
        session_id: &str,
        generation: u64,
        result: Result<bool, CallFailure>,
        key: Option<&str>,
        effects: &mut Vec<PermissionsEffect>,
    ) {
        if !self.is_current(session_id)
            || self.session_set.map(|(sent, _)| sent) != Some(generation)
        {
            return;
        }
        self.session_set = None;
        let wanted = self.wanted.take();
        match result {
            Ok(off) => {
                self.session_off = Reading::Known(off);
                self.session_unknown = false;
                self.session_failure_noticed = false;
                match wanted {
                    Some(target) if target != off => self.send_session_write(target, effects),
                    _ => {
                        let (kind, text) = self.session_changed_text(off, key);
                        notice(effects, kind, text);
                    }
                }
            }
            Err(CallFailure::Unknown | CallFailure::Unreadable) => {
                self.session_off = Reading::Pending;
                self.session_unknown = true;
                notice(effects, NoticeKind::Warning, unconfirmed_text());
                self.request_session_read(effects);
            }
            Err(failure) => write_failed_notice(&failure, effects),
        }
    }

    fn profile_set_reply(
        &mut self,
        generation: u64,
        result: Result<ApprovalMode, CallFailure>,
        key: Option<&str>,
        effects: &mut Vec<PermissionsEffect>,
    ) {
        if self.profile_set != Some(generation) {
            return;
        }
        self.profile_set = None;
        match result {
            Ok(mode) => {
                self.profile = Reading::Known(mode);
                self.profile_unknown = false;
                self.profile_failure_noticed = false;
                let text = self.profile_changed_text(mode, key);
                notice(effects, NoticeKind::System, text);
            }
            Err(CallFailure::Unknown | CallFailure::Unreadable) => {
                self.profile = Reading::Pending;
                self.profile_unknown = true;
                notice(effects, NoticeKind::Warning, unconfirmed_text());
                self.request_profile_read(effects);
            }
            Err(failure) => write_failed_notice(&failure, effects),
        }
    }

    /// The notice for a confirmed bypass write, worded by what the profile does.
    fn session_changed_text(&self, off: bool, key: Option<&str>) -> (NoticeKind, String) {
        let profile = &self.profile_name;
        match (off, self.profile) {
            (true, Reading::Known(ApprovalMode::Off)) => (
                NoticeKind::Warning,
                format!(
                    "This session now has approvals off for itself. Profile {profile} also \
                     has approvals off for every session; /permissions manual or \
                     /permissions smart changes the profile."
                ),
            ),
            (true, Reading::Known(_)) => (
                NoticeKind::Warning,
                format!(
                    "Approvals off for this session only: Hermes will run commands it would \
                     normally ask about. Rules that always block a command still apply. {}",
                    turn_back_on(key)
                ),
            ),
            (true, _) => (
                NoticeKind::Warning,
                format!(
                    "This session now has approvals off for itself: Hermes will run commands \
                     it would normally ask about. Rules that always block a command still \
                     apply. /permissions session default makes it follow profile {profile} \
                     again."
                ),
            ),
            (false, Reading::Known(ApprovalMode::Off)) => (
                NoticeKind::Warning,
                format!(
                    "This session follows profile {profile} again, which has approvals off \
                     for every session."
                ),
            ),
            (false, Reading::Known(mode)) => (
                NoticeKind::System,
                format!(
                    "Approvals back on for this session: it follows profile {profile} ({}).",
                    mode.as_wire()
                ),
            ),
            (false, _) => (
                NoticeKind::Warning,
                format!(
                    "This session follows profile {profile} again. Talaria has not read the \
                     profile's approval mode, so it cannot say whether approvals are on."
                ),
            ),
        }
    }

    /// The notice for a confirmed profile write, with a suffix when this
    /// session has its own bypass on.
    fn profile_changed_text(&self, mode: ApprovalMode, key: Option<&str>) -> String {
        let mut text = format!(
            "Approvals for every session of profile {}: {}.",
            self.profile_name,
            mode.as_wire()
        );
        if self.session_off == Reading::Known(true) {
            if mode == ApprovalMode::Off {
                text.push_str(" This session also has approvals off for itself.");
            } else {
                let how = key.unwrap_or("/permissions session default");
                text.push_str(&format!(
                    " This session still has approvals off for itself; {how} turns them back \
                     on."
                ));
            }
        }
        text
    }

    /// The three lines `/permissions` shows.
    fn show_text(&self, key: Option<&str>) -> String {
        let profile = &self.profile_name;
        let segment_words = match (&self.session_id, self.segment()) {
            (None, _) => "no session is open".to_owned(),
            (Some(_), None) => "not read yet".to_owned(),
            (Some(_), Some(segment)) => segment
                .text
                .strip_prefix("approvals: ")
                .unwrap_or(&segment.text)
                .to_owned(),
        };
        let profile_words = match self.profile {
            Reading::Known(mode) => mode.as_wire(),
            Reading::Failed => "unknown",
            _ if self.profile_unknown => "unknown",
            _ => "not read yet",
        };
        let line_three = if self.session_id.is_none() {
            "No session is open. /permissions manual, smart or off changes the profile.".to_owned()
        } else {
            let state = match self.session_off {
                Reading::Known(false) => "follows the profile",
                Reading::Known(true) => "has approvals off for itself only",
                Reading::Failed => "unknown",
                _ if self.session_unknown => "unknown",
                _ => "not read yet",
            };
            let profile_is_on = matches!(
                self.profile,
                Reading::Known(ApprovalMode::Manual | ApprovalMode::Smart)
            );
            let how = match (profile_is_on, key) {
                (true, Some(label)) => format!(
                    "{label} switches this session; /permissions manual, smart or off changes \
                     the profile."
                ),
                (true, None) => "/permissions session off or /permissions session default \
                                 switches this session; /permissions manual, smart or off \
                                 changes the profile."
                    .to_owned(),
                (false, _) => "/permissions session off or /permissions session default sets \
                               this session's own setting; /permissions manual, smart or off \
                               changes the profile."
                    .to_owned(),
            };
            format!("This session: {state}. {how}")
        };
        format!(
            "Approvals in this session: {segment_words}.\nProfile {profile}: {profile_words}. \
             This applies to every session of the profile.\n{line_three}"
        )
    }
}

fn notice(effects: &mut Vec<PermissionsEffect>, kind: NoticeKind, text: impl Into<String>) {
    effects.push(PermissionsEffect::Notice {
        kind,
        text: text.into(),
    });
}

fn no_session_text() -> &'static str {
    "No session is open, so this session's approvals cannot change. /permissions manual, smart \
     or off changes the profile."
}

fn still_reading_text() -> &'static str {
    "Talaria is still reading this session's approval setting. Try again in a moment."
}

fn unconfirmed_text() -> &'static str {
    "Talaria could not confirm the change. Reading the setting from Hermes again."
}

/// The closing sentence of the bypass-on notice.
fn turn_back_on(key: Option<&str>) -> String {
    match key {
        Some(label) => format!("{label} turns approvals back on."),
        None => "/permissions session default turns approvals back on.".to_owned(),
    }
}

/// The notice for a read that did not produce a value.
fn read_failed_notice(failure: &CallFailure, effects: &mut Vec<PermissionsEffect>) {
    let words = failure_words(failure, "config.get", false);
    notice(
        effects,
        NoticeKind::Warning,
        format!(
            "Talaria could not read the approval setting: {words}. The status line shows \
             \"approvals: unknown\" until a read succeeds."
        ),
    );
}

/// The notice for a write that was refused or never sent.
fn write_failed_notice(failure: &CallFailure, effects: &mut Vec<PermissionsEffect>) {
    let words = failure_words(failure, "config.set", true);
    if matches!(failure, CallFailure::NotSent) {
        notice(
            effects,
            NoticeKind::Error,
            format!("The change was not sent: {words}. Nothing changed."),
        );
    } else {
        notice(
            effects,
            NoticeKind::Error,
            format!("Hermes refused the change: {words}."),
        );
    }
}

/// Plain words for a failed call; `method` names the call, `write` picks the
/// wording for writes.
fn failure_words(failure: &CallFailure, method: &str, write: bool) -> String {
    match failure {
        CallFailure::Unknown => "Hermes did not answer".to_owned(),
        CallFailure::Unreadable => "Hermes gave an answer Talaria could not read".to_owned(),
        CallFailure::NotSent => "Talaria is not connected to Hermes".to_owned(),
        CallFailure::Refused {
            reason,
            rpc_code,
            message,
        } => match reason.as_str() {
            "permission_denied" if write => "this connection may not change settings".to_owned(),
            "permission_denied" => "this connection may not read settings".to_owned(),
            "invalid_params" => "Hermes did not accept the request".to_owned(),
            "profile_mismatch" => "the request named a different profile".to_owned(),
            "storage_unavailable" if write => "Hermes could not save the profile's settings \
                                               (an administrator may manage them, or the file \
                                               may be unreadable)"
                .to_owned(),
            "storage_unavailable" => "Hermes could not read the profile's settings".to_owned(),
            "not_found" => "the session no longer exists".to_owned(),
            _ if *rpc_code == -32601 => {
                format!("this Hermes gateway does not offer {method}")
            }
            "" => format!("error {rpc_code}: {message}")
                .chars()
                .take(MAX_REASON_CHARS)
                .collect(),
            other => other.chars().take(MAX_REASON_CHARS).collect(),
        },
    }
}
