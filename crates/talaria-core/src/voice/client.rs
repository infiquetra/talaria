//! Client-side voice state.

#[derive(Default)]
pub struct ClientVoice {
    epoch: u64,
    phase: Phase,
}

#[derive(Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Ready,
    Recording,
    Finishing,
    Transcribing,
    Detached,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Input {
    Start,
    Finish,
    CaptureFinished { epoch: u64 },
    Text { epoch: u64, text: String },
    Failed { epoch: u64 },
    Reopen,
    Detach,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    Capture { epoch: u64 },
    FinishCapture { epoch: u64 },
    Transcribe { epoch: u64 },
    Draft(String),
}

impl ClientVoice {
    #[must_use]
    pub fn recording(&self) -> bool {
        self.phase == Phase::Recording
    }

    #[must_use]
    pub fn view(&self, enabled: bool, read_aloud: bool) -> super::VoiceView {
        let phase = if !enabled {
            super::VoicePhase::Off
        } else {
            match self.phase {
                Phase::Ready | Phase::Detached => super::VoicePhase::Ready,
                Phase::Recording => super::VoicePhase::Recording,
                Phase::Finishing => super::VoicePhase::Stopping,
                Phase::Transcribing => super::VoicePhase::Transcribing,
            }
        };
        super::VoiceView {
            phase,
            label: None,
            partial: None,
            clock: None,
            read_aloud,
        }
    }

    pub fn update(&mut self, input: Input) -> Vec<Effect> {
        match input {
            Input::Start if self.phase == Phase::Ready => {
                self.epoch = self.epoch.saturating_add(1);
                self.phase = Phase::Recording;
                vec![Effect::Capture { epoch: self.epoch }]
            }
            Input::CaptureFinished { epoch }
                if epoch == self.epoch
                    && matches!(self.phase, Phase::Recording | Phase::Finishing) =>
            {
                self.phase = Phase::Transcribing;
                vec![Effect::Transcribe { epoch }]
            }
            Input::Finish if self.phase == Phase::Recording => {
                self.phase = Phase::Finishing;
                vec![Effect::FinishCapture { epoch: self.epoch }]
            }
            Input::Text { epoch, text }
                if epoch == self.epoch && self.phase == Phase::Transcribing =>
            {
                self.phase = Phase::Ready;
                let text = super::clean_transcript(&text);
                if text.is_empty() {
                    Vec::new()
                } else {
                    vec![Effect::Draft(text)]
                }
            }
            Input::Detach => {
                self.phase = Phase::Detached;
                Vec::new()
            }
            Input::Failed { epoch } if epoch == self.epoch && self.phase != Phase::Detached => {
                self.phase = Phase::Ready;
                Vec::new()
            }
            Input::Reopen if self.phase == Phase::Detached => {
                self.phase = Phase::Ready;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}
