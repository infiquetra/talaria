//! Client-side voice state.

#[derive(Default)]
pub struct ClientVoice;

#[derive(Debug, PartialEq, Eq)]
pub enum Input {
    Start,
    CaptureFinished { epoch: u64 },
    Text { epoch: u64, text: String },
    Detach,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    Capture { epoch: u64 },
    Transcribe { epoch: u64 },
    Draft(String),
}

impl ClientVoice {
    pub fn update(&mut self, _input: Input) -> Vec<Effect> {
        Vec::new()
    }
}
