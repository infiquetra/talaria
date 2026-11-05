//! Client-side voice state.

#[derive(Default)]
pub struct ClientVoice {
    epoch: u64,
    phase: Phase,
    speaking: bool,
    waiting_quiet: bool,
    record_after_quiet: bool,
}

#[cfg(test)]
mod tests {
    use super::{ClientVoice, Cutoff, Effect, Input};

    #[test]
    fn quiet_timeout_kills_output_and_never_starts_capture() {
        let mut voice = ClientVoice::default();
        voice.update(Input::SpeechStarted);
        voice.update(Input::Cutoff(Cutoff::Record));
        let epoch = voice.audio_epoch();
        assert!(matches!(
            voice
                .update(Input::QuietTimeout { audio_epoch: epoch })
                .as_slice(),
            [Effect::KillPlayback { .. }, Effect::Notice(_)]
        ));
        assert!(!voice.recording());
        assert!(voice.update(Input::Quiet { audio_epoch: epoch }).is_empty());
        assert!(!voice.recording());
    }

    #[test]
    fn cancelling_capture_discards_its_late_results() {
        let mut voice = ClientVoice::default();
        voice.update(Input::Start);
        let old_epoch = voice.audio_epoch();
        let effects = voice.update(Input::Cutoff(Cutoff::Stop));
        assert!(effects.contains(&Effect::CancelCapture { epoch: old_epoch }));
        assert!(
            voice
                .update(Input::CaptureFinished { epoch: old_epoch })
                .is_empty()
        );
        assert!(
            voice
                .update(Input::Text {
                    epoch: old_epoch,
                    text: "late".into()
                })
                .is_empty()
        );
        assert!(!voice.recording());
    }

    #[test]
    fn cancelling_transcription_aborts_worker_and_discards_reply() {
        let mut voice = ClientVoice::default();
        voice.update(Input::Start);
        let old_epoch = voice.audio_epoch();
        voice.update(Input::CaptureFinished { epoch: old_epoch });
        let effects = voice.update(Input::Cutoff(Cutoff::Stop));
        assert!(effects.contains(&Effect::CancelCapture { epoch: old_epoch }));
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::StopPlayback { .. }))
        );
        assert!(
            voice
                .update(Input::Text {
                    epoch: old_epoch,
                    text: "late".into()
                })
                .is_empty()
        );
    }

    #[test]
    fn cancelled_barge_in_cannot_start_on_old_quiet_confirmation() {
        let mut voice = ClientVoice::default();
        voice.update(Input::SpeechStarted);
        voice.update(Input::Cutoff(Cutoff::Record));
        let old_epoch = voice.audio_epoch();
        voice.update(Input::Cutoff(Cutoff::Stop));
        assert!(
            voice
                .update(Input::Quiet {
                    audio_epoch: old_epoch
                })
                .is_empty()
        );
        assert!(
            voice
                .update(Input::Quiet {
                    audio_epoch: voice.audio_epoch()
                })
                .is_empty()
        );
        assert!(!voice.recording());
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cutoff {
    /// Push-to-talk while speech or synthesis is still outstanding.
    Record,
    Stop,
    ReadOff,
    Escape,
    TargetChanged,
    Quit,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Input {
    Start,
    Finish,
    CaptureFinished {
        epoch: u64,
    },
    Text {
        epoch: u64,
        text: String,
    },
    Failed {
        epoch: u64,
    },
    Reopen,
    Detach,
    /// Playback or synthesis is outstanding.
    SpeechStarted,
    /// Playback completed for this epoch.
    SpeechFinished {
        audio_epoch: u64,
    },
    /// Stop speech before any new recording.
    Cutoff(Cutoff),
    /// The playback helper reported that output is quiet.
    Quiet {
        audio_epoch: u64,
    },
    /// The quiet confirmation deadline expired.
    QuietTimeout {
        audio_epoch: u64,
    },
    /// A synthesis result that may belong to an older audio epoch.
    LateSynthesis {
        audio_epoch: u64,
        text: String,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    Capture { epoch: u64 },
    FinishCapture { epoch: u64 },
    CancelCapture { epoch: u64 },
    Transcribe { epoch: u64 },
    Draft(String),
    StopPlayback { audio_epoch: u64 },
    KillPlayback { audio_epoch: u64 },
    Notice(String),
}

impl ClientVoice {
    #[must_use]
    pub fn active(&self) -> bool {
        self.speaking || self.waiting_quiet || !matches!(self.phase, Phase::Ready | Phase::Detached)
    }

    #[must_use]
    pub fn speaking(&self) -> bool {
        self.speaking
    }

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
            speaking: enabled && self.speaking,
        }
    }

    /// The counter that invalidates late playback and capture results.
    #[must_use]
    pub fn audio_epoch(&self) -> u64 {
        self.epoch
    }

    pub fn update(&mut self, input: Input) -> Vec<Effect> {
        match input {
            Input::Start if self.phase == Phase::Ready && !self.speaking && !self.waiting_quiet => {
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
                self.speaking = false;
                self.record_after_quiet = false;
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
            Input::SpeechStarted
                if self.phase == Phase::Ready && !self.waiting_quiet && !self.speaking =>
            {
                self.epoch = self.epoch.saturating_add(1);
                self.speaking = true;
                Vec::new()
            }
            Input::SpeechFinished { audio_epoch } if audio_epoch == self.epoch => {
                self.speaking = false;
                Vec::new()
            }
            Input::Cutoff(Cutoff::Record) if self.phase == Phase::Recording => {
                self.phase = Phase::Finishing;
                vec![Effect::FinishCapture { epoch: self.epoch }]
            }
            Input::Cutoff(Cutoff::Record)
                if matches!(self.phase, Phase::Finishing | Phase::Transcribing)
                    || self.waiting_quiet =>
            {
                vec![Effect::Notice("Voice is busy.".into())]
            }
            Input::Cutoff(Cutoff::Record) if self.phase == Phase::Ready && !self.speaking => {
                self.update(Input::Start)
            }
            Input::Cutoff(reason) => {
                let previous_epoch = self.epoch;
                let stop_output = self.speaking || self.waiting_quiet;
                let cancel_capture = matches!(
                    self.phase,
                    Phase::Recording | Phase::Finishing | Phase::Transcribing
                );
                self.epoch = self.epoch.saturating_add(1);
                self.speaking = false;
                self.waiting_quiet = stop_output;
                self.record_after_quiet = reason == Cutoff::Record && stop_output;
                if self.phase != Phase::Detached {
                    self.phase = Phase::Ready;
                }
                let mut effects = Vec::new();
                if cancel_capture {
                    effects.push(Effect::CancelCapture {
                        epoch: previous_epoch,
                    });
                }
                effects.push(Effect::StopPlayback {
                    audio_epoch: self.epoch,
                });
                if reason == Cutoff::ReadOff {
                    effects.push(Effect::Notice("Read-aloud stopped.".into()));
                }
                effects
            }
            Input::Quiet { audio_epoch } if self.waiting_quiet && audio_epoch == self.epoch => {
                self.waiting_quiet = false;
                if std::mem::take(&mut self.record_after_quiet) && self.phase == Phase::Ready {
                    self.phase = Phase::Recording;
                    vec![Effect::Capture { epoch: self.epoch }]
                } else {
                    Vec::new()
                }
            }
            Input::QuietTimeout { audio_epoch }
                if self.waiting_quiet && audio_epoch == self.epoch =>
            {
                self.record_after_quiet = false;
                vec![
                    Effect::KillPlayback { audio_epoch },
                    Effect::Notice(
                        "Voice playback did not become quiet; recording was not started.".into(),
                    ),
                ]
            }
            Input::LateSynthesis { .. } => Vec::new(),
            _ => Vec::new(),
        }
    }
}
