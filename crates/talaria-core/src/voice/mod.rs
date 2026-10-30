//! Per-session voice state. No socket and no terminal types.

use std::time::Duration;

pub mod speakable;

/// Where one session is in a capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoicePhase {
    Unknown,
    Off,
    Ready,
    Starting,
    Recording,
    Stopping,
    Transcribing,
}

/// What a finished transcript does with the composer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitMode {
    Direct,
    Draft,
}

impl SubmitMode {
    /// `draft` selects the draft. Anything else, including absence, is direct.
    #[must_use]
    pub fn from_config(value: Option<&str>) -> Self {
        if value == Some("draft") {
            Self::Draft
        } else {
            Self::Direct
        }
    }
}

/// A `/voice` argument after it has been recognised.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceCommand {
    On,
    Off,
    Status,
    /// `None` toggles. `Some` sets the flag.
    Read(Option<bool>),
}

/// Recognise a `/voice` argument. The error is the unknown-option notice.
pub fn parse_voice_args(arg: &str) -> Result<VoiceCommand, String> {
    let collapsed = arg.split_whitespace().collect::<Vec<_>>().join(" ");
    let key = collapsed.to_ascii_lowercase();
    match key.as_str() {
        "" | "status" => Ok(VoiceCommand::Status),
        "on" => Ok(VoiceCommand::On),
        "off" => Ok(VoiceCommand::Off),
        "read" | "tts" => Ok(VoiceCommand::Read(None)),
        "read on" => Ok(VoiceCommand::Read(Some(true))),
        "read off" => Ok(VoiceCommand::Read(Some(false))),
        _ => {
            let shown = clean_option(&collapsed);
            Err(format!(
                "Unknown /voice option \"{shown}\". Use /voice on, off, status, read on or read off."
            ))
        }
    }
}

fn clean_option(arg: &str) -> String {
    arg.chars().filter(|ch| !ch.is_control()).take(40).collect()
}

/// Methods a command would send. Read-aloud sends none.
#[must_use]
pub fn command_outbound(command: &VoiceCommand) -> &'static [&'static str] {
    match command {
        VoiceCommand::On | VoiceCommand::Off => &["slash.exec", "config.get"],
        VoiceCommand::Status => &["config.get", "voice.toggle"],
        VoiceCommand::Read(_) => &[],
    }
}

/// `status` on a record reply. Local, so this crate does not borrow gateway types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordStatus {
    Recording,
    Stopped,
    Busy,
}

/// How a turn ended, for read-aloud.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnOutcome {
    Completed,
    Failed,
    Interrupted,
}

/// One gateway notification the machine understands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoiceEventKind {
    Status {
        state: String,
    },
    Partial {
        text: String,
    },
    Transcript {
        text: String,
        stop_phrase: bool,
        typed: bool,
        no_speech: bool,
        focused: bool,
        session_id: String,
    },
    Interrupted,
}

/// What the service feeds the machine. Time arrives only as [`VoiceInput::Tick`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoiceInput {
    ModeRead(Option<String>),
    SubmitModeRead(Option<String>),
    Press,
    RecordReply {
        start: bool,
        status: RecordStatus,
        reason: Option<String>,
    },
    RecordFailed {
        start: bool,
        code: Option<i64>,
        message: String,
        dropped: bool,
    },
    Event(VoiceEventKind),
    TurnCompleted {
        assistant_markdown: Vec<String>,
        outcome: TurnOutcome,
        focused: bool,
    },
    VoiceStopped,
    /// A voice method is unavailable on this connection.
    Unavailable,
    Command(VoiceCommand),
    ConnectionOpened,
    /// A session previously detached on this connection is attached again.
    SessionReopened,
    DetachBegins,
    Tick(Duration),
    SpeakFailed {
        code: Option<i64>,
        message: String,
        dropped: bool,
    },
}

/// What the service should do after [`VoiceState::update`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoiceEffect {
    SetMode { on: bool },
    ReadMode,
    ReadSubmitMode,
    ReadRequirements,
    Record { start: bool },
    Speak(String),
    Submit(String),
    AppendToDraft(String),
    NoticeIn { session_id: String, text: String },
    Notice(String),
    StopBeforeDetach,
}

/// What the status line and the line under the composer need.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceView {
    pub phase: VoicePhase,
    pub label: Option<String>,
    pub partial: Option<String>,
    pub clock: Option<Duration>,
    pub read_aloud: bool,
}

/// One session's voice machine.
#[derive(Clone, Debug)]
pub struct VoiceState {
    session_id: String,
    phase: VoicePhase,
    read_aloud: bool,
    speak_max_chars: usize,
    submit_mode: SubmitMode,
    focused: bool,
    pending: Option<bool>,
    capture_auto_submitted: bool,
    partial: Option<String>,
    label: Option<String>,
    clock: Option<Duration>,
    transcribing_for: Duration,
    detaching: bool,
    unavailable: bool,
}

impl VoiceState {
    #[must_use]
    pub fn new(session_id: impl Into<String>, read_aloud: bool, speak_max_chars: usize) -> Self {
        Self {
            session_id: session_id.into(),
            phase: VoicePhase::Unknown,
            read_aloud,
            speak_max_chars,
            submit_mode: SubmitMode::Direct,
            focused: true,
            pending: None,
            capture_auto_submitted: false,
            partial: None,
            label: None,
            clock: None,
            transcribing_for: Duration::ZERO,
            detaching: false,
            unavailable: false,
        }
    }

    /// Apply one input without performing I/O.
    pub fn update(&mut self, input: VoiceInput, composer_empty: bool) -> Vec<VoiceEffect> {
        use VoiceEffect as Effect;
        use VoiceInput as Input;
        let mut effects = Vec::new();
        match input {
            Input::ModeRead(mode) => {
                if matches!(mode.as_deref(), Some("voice_only" | "all")) && !self.unavailable {
                    if matches!(self.phase, VoicePhase::Unknown | VoicePhase::Off) {
                        self.set_phase(VoicePhase::Ready);
                    }
                } else {
                    self.off();
                }
            }
            Input::SubmitModeRead(mode) => {
                self.submit_mode = SubmitMode::from_config(mode.as_deref())
            }
            Input::Press if self.unavailable => effects.push(Effect::Notice(UNAVAILABLE.into())),
            Input::Press if self.pending.is_some() || self.detaching => {}
            Input::Press => match self.phase {
                VoicePhase::Unknown => effects.push(Effect::Notice("Checking voice mode…".into())),
                VoicePhase::Off => effects.push(Effect::Notice(
                    "Voice is off. Type /voice on to use the microphone.".into(),
                )),
                VoicePhase::Ready => {
                    self.capture_auto_submitted = false;
                    self.pending = Some(true);
                    self.set_phase(VoicePhase::Starting);
                    effects.push(Effect::Record { start: true });
                }
                VoicePhase::Recording => {
                    self.pending = Some(false);
                    self.set_phase(VoicePhase::Stopping);
                    effects.push(Effect::Record { start: false });
                }
                VoicePhase::Starting | VoicePhase::Stopping | VoicePhase::Transcribing => {}
            },
            Input::RecordReply {
                start,
                status,
                reason,
            } => {
                if self.pending == Some(start) {
                    self.pending = None;
                    let sent_phase = if start {
                        VoicePhase::Starting
                    } else {
                        VoicePhase::Stopping
                    };
                    if self.phase == sent_phase {
                        match (start, status) {
                            (true, RecordStatus::Recording) => {
                                self.set_phase(VoicePhase::Recording)
                            }
                            (false, RecordStatus::Stopped) => {
                                self.set_phase(VoicePhase::Transcribing)
                            }
                            (_, RecordStatus::Busy) => {
                                self.set_phase(VoicePhase::Ready);
                                let notice = if reason.as_deref() == Some("wake_owned") {
                                    "The wake word holds the microphone. Turn it off in Hermes, then try again."
                                } else {
                                    "The microphone is busy."
                                };
                                effects.push(Effect::Notice(notice.into()));
                            }
                            _ => self.set_phase(VoicePhase::Ready),
                        }
                    }
                }
            }
            Input::RecordFailed {
                start,
                code,
                message,
                dropped,
            } => {
                if code == Some(-32601) {
                    self.disable_unavailable(&mut effects);
                } else if self.pending == Some(start) {
                    self.pending = None;
                    let sent_phase = if start {
                        VoicePhase::Starting
                    } else {
                        VoicePhase::Stopping
                    };
                    if self.phase == sent_phase {
                        if dropped {
                            self.set_phase(VoicePhase::Unknown);
                            effects.push(Effect::Notice(DROPPED.into()));
                            effects.push(Effect::ReadMode);
                        } else {
                            if start && code == Some(4015) {
                                self.off();
                            } else {
                                self.set_phase(VoicePhase::Ready);
                            }
                            effects.push(Effect::Notice(record_error(code, &message)));
                        }
                    }
                }
            }
            Input::Event(event) => self.event(event, composer_empty, &mut effects),
            Input::TurnCompleted {
                assistant_markdown,
                outcome,
                focused,
            } if self.read_aloud
                && focused
                && outcome == TurnOutcome::Completed
                && !self.detaching =>
            {
                if let Some(text) = speakable::speakable_text(
                    &assistant_markdown.join("\n\n"),
                    self.speak_max_chars,
                ) {
                    effects.push(Effect::Speak(text));
                }
            }
            Input::TurnCompleted { .. } => {}
            Input::VoiceStopped => {
                self.off();
                effects.push(Effect::ReadMode);
            }
            Input::Unavailable => self.disable_unavailable(&mut effects),
            Input::Command(command) => match command {
                VoiceCommand::On => effects.push(Effect::SetMode { on: true }),
                VoiceCommand::Off => effects.push(Effect::SetMode { on: false }),
                VoiceCommand::Status => {
                    effects.extend([Effect::ReadMode, Effect::ReadRequirements])
                }
                VoiceCommand::Read(value) => {
                    let enabled = value.unwrap_or(!self.read_aloud);
                    if enabled && self.unavailable {
                        effects.push(Effect::Notice(UNAVAILABLE.into()));
                    } else {
                        self.read_aloud = enabled;
                        effects.push(Effect::Notice(
                            if enabled {
                                "Read-aloud on."
                            } else {
                                "Read-aloud off. A reply already speaking will finish."
                            }
                            .into(),
                        ));
                    }
                }
            },
            Input::ConnectionOpened => {
                if matches!(
                    self.phase,
                    VoicePhase::Starting
                        | VoicePhase::Recording
                        | VoicePhase::Stopping
                        | VoicePhase::Transcribing
                ) {
                    effects.push(Effect::Notice(DROPPED.into()));
                }
                self.pending = None;
                self.detaching = false;
                self.unavailable = false;
                self.partial = None;
                self.label = None;
                self.set_phase(VoicePhase::Unknown);
                effects.push(Effect::ReadMode);
            }
            Input::SessionReopened => {
                self.pending = None;
                self.detaching = false;
                self.partial = None;
                self.label = None;
                if self.unavailable {
                    self.off();
                } else {
                    self.set_phase(VoicePhase::Unknown);
                }
                effects.push(Effect::ReadMode);
            }
            Input::DetachBegins => {
                self.detaching = true;
                self.partial = None;
                if matches!(self.phase, VoicePhase::Starting | VoicePhase::Recording) {
                    effects.push(Effect::StopBeforeDetach);
                }
            }
            Input::Tick(delta) => {
                if self.phase == VoicePhase::Recording {
                    self.clock = Some(self.clock.unwrap_or_default().saturating_add(delta));
                } else if self.phase == VoicePhase::Transcribing {
                    self.transcribing_for = self.transcribing_for.saturating_add(delta);
                    if self.transcribing_for >= Duration::from_secs(120) {
                        self.set_phase(VoicePhase::Ready);
                    }
                }
            }
            Input::SpeakFailed {
                code,
                message,
                dropped,
            } => {
                if dropped {
                    effects.push(Effect::Notice(
                        "Read-aloud got no answer from the gateway.".into(),
                    ));
                } else if code == Some(4020) {
                    effects.push(Effect::Notice(
                        "Talaria sent an empty text to speak (4020). Please report this.".into(),
                    ));
                } else if matches!(code, Some(5026 | -32601)) {
                    if code == Some(-32601) {
                        self.disable_unavailable(&mut effects);
                    }
                    self.read_aloud = false;
                    effects.push(Effect::Notice(format!(
                        "Read-aloud is off: {}.",
                        clean_message(&message)
                    )));
                } else {
                    effects.push(Effect::Notice(
                        "Read-aloud got no answer from the gateway.".into(),
                    ));
                }
            }
        }
        effects
    }

    #[must_use]
    pub fn view(&self) -> VoiceView {
        VoiceView {
            phase: self.phase,
            label: self.label.clone(),
            partial: self.partial.as_ref().map(|text| format!("heard: {text}")),
            clock: if self.phase == VoicePhase::Starting {
                Some(Duration::ZERO)
            } else {
                self.clock
            },
            read_aloud: self.read_aloud,
        }
    }

    #[must_use]
    pub fn phase(&self) -> VoicePhase {
        self.phase
    }

    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    #[must_use]
    pub fn read_aloud(&self) -> bool {
        self.read_aloud
    }

    #[must_use]
    pub fn submit_mode(&self) -> SubmitMode {
        self.submit_mode
    }

    /// The service tells the machine whether this session is the focused one.
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// The in-flight record request, if any (`true` means start).
    #[must_use]
    pub fn pending_record(&self) -> Option<bool> {
        self.pending
    }

    fn set_phase(&mut self, phase: VoicePhase) {
        if phase != VoicePhase::Recording {
            self.clock = None;
        } else if self.phase != VoicePhase::Recording {
            self.clock = Some(Duration::ZERO);
        }
        if phase != VoicePhase::Transcribing || self.phase != VoicePhase::Transcribing {
            self.transcribing_for = Duration::ZERO;
        }
        self.phase = phase;
    }

    fn off(&mut self) {
        self.set_phase(VoicePhase::Off);
        self.partial = None;
        self.label = None;
    }

    fn disable_unavailable(&mut self, effects: &mut Vec<VoiceEffect>) {
        if !self.unavailable {
            effects.push(VoiceEffect::Notice(UNAVAILABLE.into()));
        }
        self.unavailable = true;
        self.pending = None;
        self.read_aloud = false;
        self.off();
    }

    fn event(
        &mut self,
        event: VoiceEventKind,
        composer_empty: bool,
        effects: &mut Vec<VoiceEffect>,
    ) {
        use VoiceEffect as Effect;
        match event {
            VoiceEventKind::Status { state } => match state.as_str() {
                "listening" => {
                    self.label = None;
                    if matches!(self.phase, VoicePhase::Starting | VoicePhase::Ready) {
                        self.set_phase(VoicePhase::Recording);
                    }
                }
                "transcribing" => {
                    self.label = None;
                    if matches!(self.phase, VoicePhase::Recording | VoicePhase::Stopping) {
                        self.set_phase(VoicePhase::Transcribing);
                    }
                }
                "idle" => {
                    self.label = None;
                    self.partial = None;
                    if matches!(
                        self.phase,
                        VoicePhase::Starting
                            | VoicePhase::Recording
                            | VoicePhase::Stopping
                            | VoicePhase::Transcribing
                    ) {
                        self.set_phase(VoicePhase::Ready);
                    }
                }
                _ if !matches!(self.phase, VoicePhase::Unknown | VoicePhase::Off) => {
                    let label: String = state
                        .chars()
                        .filter(|ch| !ch.is_control())
                        .take(24)
                        .collect();
                    if !label.is_empty() {
                        self.label = Some(label);
                    }
                }
                _ => {}
            },
            VoiceEventKind::Partial { text }
                if !self.detaching
                    && !matches!(self.phase, VoicePhase::Unknown | VoicePhase::Off) =>
            {
                self.partial = Some(clean_transcript(&text));
            }
            VoiceEventKind::Partial { .. } => {}
            VoiceEventKind::Transcript {
                text,
                stop_phrase,
                typed,
                no_speech,
                focused,
                session_id,
            } => {
                if self.detaching {
                    return;
                }
                self.partial = None;
                self.label = None;
                if stop_phrase || typed {
                    self.off();
                    effects.push(Effect::Notice("Voice chat ended (stop phrase).".into()));
                } else if no_speech {
                    self.off();
                    effects.push(Effect::Notice(
                        "Voice turned off after three recordings with no speech.".into(),
                    ));
                } else {
                    let text = clean_transcript(&text);
                    if text.is_empty() {
                        return;
                    }
                    if !focused {
                        effects.push(Effect::NoticeIn {
                            session_id,
                            text: format!("Voice transcript (not sent): {text}"),
                        });
                    } else if self.submit_mode == SubmitMode::Direct
                        && composer_empty
                        && !self.capture_auto_submitted
                    {
                        self.capture_auto_submitted = true;
                        effects.push(Effect::Submit(text));
                    } else {
                        effects.push(Effect::AppendToDraft(text));
                        if self.submit_mode == SubmitMode::Direct {
                            effects.push(Effect::Notice(
                                "Transcript added to your draft. Press Enter to send.".into(),
                            ));
                        }
                    }
                }
            }
            VoiceEventKind::Interrupted => {
                effects.push(Effect::Notice("Interrupted by voice.".into()))
            }
        }
    }
}

const UNAVAILABLE: &str = "Voice is not available on this gateway.";
const DROPPED: &str = "Recording state unknown: the connection dropped.";

/// Clean untrusted transcript text before insertion or display.
#[must_use]
pub fn clean_transcript(text: &str) -> String {
    text.chars()
        .filter_map(|ch| match ch {
            '\n' | '\r' | '\t' => Some(' '),
            ch if ch.is_control() => None,
            ch => Some(ch),
        })
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Clean a gateway message for a notice.
#[must_use]
pub fn clean_message(text: &str) -> String {
    let text = clean_transcript(text);
    let mut chars = text.chars();
    let mut value: String = chars.by_ref().take(120).collect();
    if chars.next().is_some() {
        value.pop();
        value.push('…');
    }
    if value.ends_with('.') {
        value.pop();
    }
    if value.is_empty() {
        "no message given".into()
    } else {
        value
    }
}

fn record_error(code: Option<i64>, message: &str) -> String {
    match code {
        Some(4015) => "Voice is off for this session. Type /voice on.".into(),
        Some(4016) => "Another session is recording. Stop it first.".into(),
        Some(4019) => {
            "Talaria sent a voice request Hermes did not understand (4019). Please report this."
                .into()
        }
        _ => format!(
            "Voice failed on the gateway: {}. Run /voice status.",
            clean_message(message)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RecordStatus, SubmitMode, TurnOutcome, VoiceEffect, VoiceEventKind, VoiceInput, VoicePhase,
        VoiceState,
    };
    use std::time::Duration;

    const CHECKING: &str = "Checking voice mode…";
    const OFF_PRESS: &str = "Voice is off. Type /voice on to use the microphone.";
    const UNAVAILABLE: &str = "Voice is not available on this gateway.";
    const WAKE: &str = "The wake word holds the microphone. Turn it off in Hermes, then try again.";
    const BUSY: &str = "The microphone is busy.";
    const CODE_4015: &str = "Voice is off for this session. Type /voice on.";
    const CODE_4016: &str = "Another session is recording. Stop it first.";
    const CODE_4019: &str =
        "Talaria sent a voice request Hermes did not understand (4019). Please report this.";
    const DROPPED: &str = "Recording state unknown: the connection dropped.";
    const INTERRUPTED: &str = "Interrupted by voice.";
    const STOP_PHRASE: &str = "Voice chat ended (stop phrase).";
    const NO_SPEECH: &str = "Voice turned off after three recordings with no speech.";
    const DRAFT_NOTICE: &str = "Transcript added to your draft. Press Enter to send.";
    const READ_ON: &str = "Read-aloud on.";
    const READ_OFF: &str = "Read-aloud off. A reply already speaking will finish.";
    const CODE_4020: &str = "Talaria sent an empty text to speak (4020). Please report this.";

    fn session() -> VoiceState {
        VoiceState::new("fixture-session", false, 1500)
    }

    fn notices(effects: &[VoiceEffect]) -> Vec<&str> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                VoiceEffect::Notice(text) => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    fn starts(effects: &[VoiceEffect]) -> Vec<bool> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                VoiceEffect::Record { start } => Some(*start),
                _ => None,
            })
            .collect()
    }

    fn require(effects: &[VoiceEffect], text: &str) {
        assert!(
            notices(effects).contains(&text),
            "missing {text:?} in {effects:?}"
        );
    }

    fn mode(state: &mut VoiceState, value: Option<&str>) {
        state.update(VoiceInput::ModeRead(value.map(str::to_owned)), true);
    }

    fn ready(state: &mut VoiceState) {
        mode(state, Some("voice_only"));
        assert_eq!(state.phase(), VoicePhase::Ready, "voice_only reaches Ready");
    }

    fn press(state: &mut VoiceState) -> Vec<VoiceEffect> {
        state.update(VoiceInput::Press, true)
    }

    fn status(state: &mut VoiceState, name: &str) -> Vec<VoiceEffect> {
        state.update(
            VoiceInput::Event(VoiceEventKind::Status {
                state: name.to_owned(),
            }),
            true,
        )
    }

    fn reply(state: &mut VoiceState, start: bool, status: RecordStatus, reason: Option<&str>) {
        state.update(
            VoiceInput::RecordReply {
                start,
                status,
                reason: reason.map(str::to_owned),
            },
            true,
        );
    }

    fn failed(
        state: &mut VoiceState,
        start: bool,
        code: Option<i64>,
        message: &str,
        dropped: bool,
    ) -> Vec<VoiceEffect> {
        state.update(
            VoiceInput::RecordFailed {
                start,
                code,
                message: message.to_owned(),
                dropped,
            },
            true,
        )
    }

    fn transcript(
        text: &str,
        focused: bool,
        session_id: &str,
        stop_phrase: bool,
        typed: bool,
        no_speech: bool,
    ) -> VoiceInput {
        VoiceInput::Event(VoiceEventKind::Transcript {
            text: text.to_owned(),
            stop_phrase,
            typed,
            no_speech,
            focused,
            session_id: session_id.to_owned(),
        })
    }

    #[test]
    fn press_when_off_or_unknown_sends_nothing() {
        let mut state = session();
        assert_eq!(state.phase(), VoicePhase::Unknown);
        let effects = press(&mut state);
        require(&effects, CHECKING);
        assert!(starts(&effects).is_empty(), "Unknown press sends nothing");
        mode(&mut state, None);
        assert_eq!(state.phase(), VoicePhase::Off, "an absent mode is off");
        let effects = press(&mut state);
        require(&effects, OFF_PRESS);
        assert!(starts(&effects).is_empty(), "Off press sends nothing");
        assert!(
            state.view().label.is_none(),
            "Unknown and Off hide the segment"
        );
    }

    #[test]
    fn press_cycle_and_single_pending_call() {
        let mut state = session();
        ready(&mut state);
        let effects = press(&mut state);
        assert_eq!(starts(&effects), vec![true], "Ready press starts");
        assert_eq!(state.phase(), VoicePhase::Starting);
        let ignored = press(&mut state);
        assert!(starts(&ignored).is_empty(), "Starting ignores a press");
        status(&mut state, "listening");
        assert_eq!(state.phase(), VoicePhase::Recording);
        reply(&mut state, true, RecordStatus::Recording, None);
        assert_eq!(state.phase(), VoicePhase::Recording, "recording confirms");
        let effects = press(&mut state);
        assert_eq!(starts(&effects), vec![false], "Recording press stops");
        assert_eq!(state.phase(), VoicePhase::Stopping);
        let ignored = press(&mut state);
        assert!(starts(&ignored).is_empty(), "Stopping ignores a press");
        reply(&mut state, false, RecordStatus::Stopped, None);
        assert_eq!(state.phase(), VoicePhase::Transcribing);
        let ignored = press(&mut state);
        assert!(starts(&ignored).is_empty(), "Transcribing ignores a press");
    }

    #[test]
    fn record_replies_and_errors_map_to_notices() {
        let mut state = session();
        ready(&mut state);
        press(&mut state);
        reply(&mut state, true, RecordStatus::Recording, None);
        assert_eq!(state.phase(), VoicePhase::Recording);

        let mut state = session();
        ready(&mut state);
        press(&mut state);
        let effects = state.update(
            VoiceInput::RecordReply {
                start: true,
                status: RecordStatus::Busy,
                reason: Some("wake_owned".to_owned()),
            },
            true,
        );
        require(&effects, WAKE);
        assert_eq!(state.phase(), VoicePhase::Ready);

        let mut state = session();
        ready(&mut state);
        press(&mut state);
        let effects = state.update(
            VoiceInput::RecordReply {
                start: true,
                status: RecordStatus::Busy,
                reason: None,
            },
            true,
        );
        require(&effects, BUSY);
        assert_eq!(state.phase(), VoicePhase::Ready);

        let mut state = session();
        ready(&mut state);
        press(&mut state);
        let effects = failed(&mut state, true, Some(4015), "off", false);
        require(&effects, CODE_4015);
        assert_eq!(state.phase(), VoicePhase::Off);

        for (code, text) in [(4016, CODE_4016), (4019, CODE_4019)] {
            let mut state = session();
            ready(&mut state);
            press(&mut state);
            let effects = failed(&mut state, true, Some(code), "no", false);
            require(&effects, text);
            assert_eq!(state.phase(), VoicePhase::Ready, "{code}");
        }

        let mut state = session();
        ready(&mut state);
        press(&mut state);
        let effects = failed(&mut state, true, Some(5025), "gateway down", false);
        require(
            &effects,
            "Voice failed on the gateway: gateway down. Run /voice status.",
        );
        assert_eq!(state.phase(), VoicePhase::Ready);

        let mut state = session();
        ready(&mut state);
        press(&mut state);
        status(&mut state, "listening");
        assert!(
            starts(&press(&mut state)).is_empty(),
            "the start reply is still pending"
        );
        reply(&mut state, true, RecordStatus::Recording, None);
        press(&mut state);
        assert_eq!(state.phase(), VoicePhase::Stopping);
        let effects = failed(&mut state, false, Some(4016), "no", false);
        require(&effects, CODE_4016);
        assert_eq!(
            state.phase(),
            VoicePhase::Ready,
            "a failed stop returns Ready"
        );
        assert_ne!(state.phase(), VoicePhase::Transcribing);

        let mut state = session();
        ready(&mut state);
        press(&mut state);
        status(&mut state, "listening");
        reply(&mut state, true, RecordStatus::Recording, None);
        press(&mut state);
        let effects = state.update(
            VoiceInput::RecordReply {
                start: false,
                status: RecordStatus::Busy,
                reason: None,
            },
            true,
        );
        require(&effects, BUSY);
        assert_eq!(state.phase(), VoicePhase::Ready);

        let mut state = session();
        ready(&mut state);
        press(&mut state);
        let effects = failed(&mut state, true, Some(-32601), "method not found", false);
        require(&effects, UNAVAILABLE);
        assert_eq!(state.phase(), VoicePhase::Off);
        assert!(!state.read_aloud());
        let effects = press(&mut state);
        require(&effects, UNAVAILABLE);
        assert!(
            starts(&effects).is_empty(),
            "unavailable press sends nothing"
        );

        let mut state = session();
        ready(&mut state);
        press(&mut state);
        let effects = failed(&mut state, true, None, "", true);
        require(&effects, DROPPED);
        assert_eq!(state.phase(), VoicePhase::Unknown);
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, VoiceEffect::ReadMode)),
            "a drop re-reads the mode"
        );
        assert!(starts(&effects).is_empty(), "a drop never resends start");
    }

    #[test]
    fn out_of_order_replies_are_harmless() {
        let mut state = session();
        ready(&mut state);
        press(&mut state);
        status(&mut state, "listening");
        assert_eq!(state.phase(), VoicePhase::Recording);
        assert!(state.view().clock.is_some(), "listening starts the clock");
        reply(&mut state, true, RecordStatus::Recording, None);
        assert_eq!(
            state.phase(),
            VoicePhase::Recording,
            "a late recording reply is a no-op"
        );
        assert_eq!(
            starts(&press(&mut state)).len(),
            1,
            "one stop, not a second start"
        );

        let mut state = session();
        ready(&mut state);
        press(&mut state);
        status(&mut state, "listening");
        reply(&mut state, true, RecordStatus::Recording, None);
        press(&mut state);
        state.update(
            transcript(
                "what is two plus two",
                true,
                "fixture-session",
                false,
                false,
                false,
            ),
            true,
        );
        status(&mut state, "idle");
        assert_eq!(
            state.phase(),
            VoicePhase::Ready,
            "idle before stopped reaches Ready"
        );
        reply(&mut state, false, RecordStatus::Stopped, None);
        assert_eq!(
            state.phase(),
            VoicePhase::Ready,
            "a late stopped reply stays Ready"
        );
        assert!(
            state.view().clock.is_none(),
            "a late reply does not restore REC"
        );

        let mut state = session();
        ready(&mut state);
        press(&mut state);
        status(&mut state, "listening");
        reply(&mut state, true, RecordStatus::Recording, None);
        press(&mut state);
        state.update(
            transcript("stop now", true, "fixture-session", true, false, false),
            true,
        );
        assert_eq!(state.phase(), VoicePhase::Off, "the stop phrase wins");
        reply(&mut state, false, RecordStatus::Stopped, None);
        assert_eq!(
            state.phase(),
            VoicePhase::Off,
            "stopped after the phrase does nothing"
        );

        let mut state = session();
        ready(&mut state);
        press(&mut state);
        status(&mut state, "listening");
        status(&mut state, "transcribing");
        assert_eq!(state.phase(), VoicePhase::Transcribing);
        state.update(VoiceInput::Tick(Duration::from_secs(119)), true);
        assert_eq!(
            state.phase(),
            VoicePhase::Transcribing,
            "119s still waits for idle"
        );
        state.update(VoiceInput::Tick(Duration::from_secs(1)), true);
        assert_eq!(
            state.phase(),
            VoicePhase::Ready,
            "120s without idle returns Ready"
        );
    }

    #[test]
    fn status_events_and_labels() {
        let mut state = session();
        ready(&mut state);
        status(&mut state, "listening");
        assert_eq!(state.phase(), VoicePhase::Recording);
        status(&mut state, "transcribing");
        assert_eq!(state.phase(), VoicePhase::Transcribing);
        status(&mut state, "idle");
        assert_eq!(state.phase(), VoicePhase::Ready);
        let raw = "warming\u{1b}[31m".to_string();
        status(&mut state, &raw);
        assert_eq!(state.view().label.as_deref(), Some("warming[31m"));
        assert_eq!(
            state.phase(),
            VoicePhase::Ready,
            "a label does not move the phase"
        );
        let long = "abcdefghijklmnopqrstuvwxyz0123";
        assert_eq!(long.chars().count(), 30);
        status(&mut state, long);
        assert_eq!(
            state.view().label.as_deref(),
            Some("abcdefghijklmnopqrstuvwx")
        );
        assert!(!state.view().label.unwrap_or_default().contains('…'));
    }

    #[test]
    fn partial_line_lifecycle() {
        let mut state = session();
        ready(&mut state);
        let effects = state.update(
            VoiceInput::Event(VoiceEventKind::Partial {
                text: "one".to_owned(),
            }),
            true,
        );
        assert_eq!(state.view().partial.as_deref(), Some("heard: one"));
        assert!(
            notices(&effects).is_empty(),
            "a partial is not a scrollback notice"
        );
        state.update(
            VoiceInput::Event(VoiceEventKind::Partial {
                text: "two\nlines".to_owned(),
            }),
            true,
        );
        assert_eq!(state.view().partial.as_deref(), Some("heard: two lines"));
        state.update(
            transcript("two lines", true, "fixture-session", false, false, false),
            true,
        );
        assert!(
            state.view().partial.is_none(),
            "a transcript clears the line"
        );
        state.update(
            VoiceInput::Event(VoiceEventKind::Partial {
                text: "again".to_owned(),
            }),
            true,
        );
        status(&mut state, "idle");
        assert!(state.view().partial.is_none(), "idle clears the line");
        state.update(
            VoiceInput::Event(VoiceEventKind::Partial {
                text: "last".to_owned(),
            }),
            true,
        );
        mode(&mut state, Some("off"));
        assert_eq!(state.phase(), VoicePhase::Off);
        assert!(state.view().partial.is_none(), "Off clears the line");
    }

    #[test]
    fn transcript_delivery_matrix() {
        let mut state = session();
        ready(&mut state);
        state.update(VoiceInput::SubmitModeRead(Some("direct".to_owned())), true);
        assert_eq!(state.submit_mode(), SubmitMode::Direct);
        let effects = state.update(
            transcript(
                "a\u{0001}b\tc",
                true,
                "fixture-session",
                false,
                false,
                false,
            ),
            true,
        );
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, VoiceEffect::Submit(text) if text == "ab c")),
            "direct and empty submits the cleaned text: {effects:?}"
        );

        let mut state = session();
        ready(&mut state);
        let effects = state.update(
            transcript("more", true, "fixture-session", false, false, false),
            false,
        );
        assert!(
            effects.iter().any(|effect| {
                matches!(effect, VoiceEffect::AppendToDraft(text) if text == "more")
            }),
            "direct with a draft appends"
        );
        require(&effects, DRAFT_NOTICE);

        let mut state = session();
        ready(&mut state);
        state.update(VoiceInput::SubmitModeRead(Some("draft".to_owned())), true);
        assert_eq!(state.submit_mode(), SubmitMode::Draft);
        let effects = state.update(
            transcript("later", true, "fixture-session", false, false, false),
            true,
        );
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, VoiceEffect::AppendToDraft(_))),
            "draft mode appends"
        );
        assert!(
            !notices(&effects).contains(&DRAFT_NOTICE),
            "draft mode is quiet"
        );
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, VoiceEffect::Submit(_)))
        );

        let mut state = session();
        ready(&mut state);
        let effects = state.update(
            transcript(" \n\t ", true, "fixture-session", false, false, false),
            true,
        );
        assert!(effects.is_empty(), "whitespace sends nothing: {effects:?}");

        let mut state = session();
        ready(&mut state);
        let effects = state.update(
            transcript("hello", false, "other-session", false, false, false),
            true,
        );
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                VoiceEffect::NoticeIn { session_id, text }
                    if session_id == "other-session"
                        && text == "Voice transcript (not sent): hello"
            )),
            "another session is notified, not submitted: {effects:?}"
        );
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, VoiceEffect::Submit(_)))
        );
    }

    #[test]
    fn one_automatic_submit_per_capture() {
        let mut state = session();
        ready(&mut state);
        press(&mut state);
        status(&mut state, "listening");
        let first = state.update(
            transcript("one", true, "fixture-session", false, false, false),
            true,
        );
        assert!(
            first
                .iter()
                .any(|effect| matches!(effect, VoiceEffect::Submit(_))),
            "the first transcript submits"
        );
        let second = state.update(
            transcript("two", true, "fixture-session", false, false, false),
            true,
        );
        assert!(
            second
                .iter()
                .any(|effect| matches!(effect, VoiceEffect::AppendToDraft(_))),
            "the second transcript appends"
        );
        assert!(
            !second
                .iter()
                .any(|effect| matches!(effect, VoiceEffect::Submit(_))),
            "the capture submits once"
        );
    }

    #[test]
    fn stop_phrase_typed_no_speech_and_voice_stopped() {
        let mut state = session();
        ready(&mut state);
        let effects = state.update(
            transcript("please stop", true, "fixture-session", true, false, false),
            true,
        );
        require(&effects, STOP_PHRASE);
        assert!(
            !effects.iter().any(|effect| matches!(
                effect,
                VoiceEffect::Submit(_) | VoiceEffect::AppendToDraft(_)
            )),
            "stop-phrase text is discarded"
        );
        assert_eq!(state.phase(), VoicePhase::Off);

        let mut state = session();
        ready(&mut state);
        let effects = state.update(
            transcript("stop", true, "fixture-session", false, true, false),
            true,
        );
        require(&effects, STOP_PHRASE);
        assert_eq!(
            state.phase(),
            VoicePhase::Off,
            "typed takes the stop-phrase branch"
        );

        let mut state = session();
        ready(&mut state);
        let effects = state.update(
            transcript("", true, "fixture-session", false, false, true),
            true,
        );
        require(&effects, NO_SPEECH);
        assert_eq!(state.phase(), VoicePhase::Off);

        let mut state = session();
        ready(&mut state);
        state.set_focused(false);
        let effects = state.update(VoiceInput::VoiceStopped, true);
        assert_eq!(state.phase(), VoicePhase::Off);
        assert!(state.view().partial.is_none());
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, VoiceEffect::ReadMode)),
            "VoiceStopped re-reads this session"
        );
        assert_eq!(state.session_id(), "fixture-session");
    }

    #[test]
    fn reconnect_resets_and_never_resends_start() {
        let mut state = session();
        ready(&mut state);
        press(&mut state);
        status(&mut state, "listening");
        assert_eq!(state.phase(), VoicePhase::Recording);
        let effects = state.update(VoiceInput::ConnectionOpened, true);
        assert_eq!(state.phase(), VoicePhase::Unknown);
        require(&effects, DROPPED);
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, VoiceEffect::ReadMode))
        );
        assert!(starts(&effects).is_empty(), "reconnect never resends start");
        let effects = press(&mut state);
        assert!(
            starts(&effects).is_empty(),
            "Unknown after reconnect sends nothing"
        );
        require(&effects, CHECKING);
    }

    #[test]
    fn read_aloud_rules() {
        let mut state = VoiceState::new("fixture-session", true, 1500);
        ready(&mut state);
        let effects = state.update(
            VoiceInput::TurnCompleted {
                assistant_markdown: vec!["Hello.".to_owned(), "See you.".to_owned()],
                outcome: TurnOutcome::Completed,
                focused: true,
            },
            true,
        );
        assert!(
            effects.iter().any(
                |effect| matches!(effect, VoiceEffect::Speak(text) if text == "Hello. See you.")
            ),
            "cells are joined in order: {effects:?}"
        );

        let mut state = VoiceState::new("fixture-session", true, 1500);
        ready(&mut state);
        let effects = state.update(
            VoiceInput::TurnCompleted {
                assistant_markdown: vec!["```\nfn main() {}\n```".to_owned()],
                outcome: TurnOutcome::Completed,
                focused: true,
            },
            true,
        );
        assert!(
            effects.iter().any(
                |effect| matches!(effect, VoiceEffect::Speak(text) if text == "(code omitted)")
            ),
            "code alone is still spoken: {effects:?}"
        );

        let mut state = VoiceState::new("fixture-session", true, 1500);
        ready(&mut state);
        let effects = state.update(
            VoiceInput::TurnCompleted {
                assistant_markdown: vec!["***".to_owned()],
                outcome: TurnOutcome::Completed,
                focused: true,
            },
            true,
        );
        assert!(effects.is_empty(), "punctuation is not spoken: {effects:?}");

        for outcome in [TurnOutcome::Failed, TurnOutcome::Interrupted] {
            let mut state = VoiceState::new("fixture-session", true, 1500);
            ready(&mut state);
            let effects = state.update(
                VoiceInput::TurnCompleted {
                    assistant_markdown: vec!["Hello.".to_owned()],
                    outcome,
                    focused: true,
                },
                true,
            );
            assert!(effects.is_empty(), "{outcome:?} is not spoken");
        }

        let mut state = VoiceState::new("fixture-session", true, 1500);
        ready(&mut state);
        let effects = state.update(
            VoiceInput::TurnCompleted {
                assistant_markdown: vec!["Hello.".to_owned()],
                outcome: TurnOutcome::Completed,
                focused: false,
            },
            true,
        );
        assert!(effects.is_empty(), "an unfocused session is not spoken");

        let mut state = session();
        ready(&mut state);
        let effects = state.update(
            VoiceInput::TurnCompleted {
                assistant_markdown: vec!["Hello.".to_owned()],
                outcome: TurnOutcome::Completed,
                focused: true,
            },
            true,
        );
        assert!(effects.is_empty(), "the flag off speaks nothing");

        let mut state = VoiceState::new("fixture-session", true, 1500);
        let effects = state.update(
            VoiceInput::SpeakFailed {
                code: Some(4020),
                message: "empty".to_owned(),
                dropped: false,
            },
            true,
        );
        require(&effects, CODE_4020);
        assert!(state.read_aloud(), "4020 keeps the flag");

        let effects = state.update(
            VoiceInput::Command(super::VoiceCommand::Read(Some(true))),
            true,
        );
        require(&effects, READ_ON);
        let effects = state.update(
            VoiceInput::Command(super::VoiceCommand::Read(Some(false))),
            true,
        );
        require(&effects, READ_OFF);
        assert!(!state.read_aloud());

        let mut state = VoiceState::new("fixture-session", true, 1500);
        let effects = state.update(VoiceInput::Event(VoiceEventKind::Interrupted), true);
        require(&effects, INTERRUPTED);
    }

    #[test]
    fn detach_stops_capture_and_drops_transcripts() {
        let mut state = session();
        ready(&mut state);
        press(&mut state);
        assert_eq!(state.phase(), VoicePhase::Starting);
        state.set_focused(false);
        let effects = state.update(VoiceInput::DetachBegins, true);
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, VoiceEffect::StopBeforeDetach)),
            "Starting stops the capture's session: {effects:?}"
        );
        assert_eq!(state.session_id(), "fixture-session");
        let later = state.update(
            transcript("too late", true, "fixture-session", false, false, false),
            true,
        );
        assert!(later.is_empty(), "a transcript after detach is dropped");
        assert!(state.view().partial.is_none());

        let mut state = session();
        ready(&mut state);
        press(&mut state);
        status(&mut state, "listening");
        assert_eq!(state.phase(), VoicePhase::Recording);
        let effects = state.update(VoiceInput::DetachBegins, true);
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, VoiceEffect::StopBeforeDetach)),
            "Recording stops the capture"
        );

        let mut state = session();
        ready(&mut state);
        let effects = state.update(VoiceInput::DetachBegins, true);
        assert!(
            effects.is_empty(),
            "Ready detach sends nothing: {effects:?}"
        );
    }
}
