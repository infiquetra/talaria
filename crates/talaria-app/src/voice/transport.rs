//! Workstation audio transport.

pub fn remote_audio_allowed(_remote: bool, _opt_in: bool) -> bool {
    false
}

pub struct Helpers;

impl Helpers {
    pub fn start(_capture: &str, _playback: &str) -> std::io::Result<Self> {
        Err(std::io::Error::other("not implemented"))
    }

    pub fn pids(&self) -> (u32, u32) {
        (0, 0)
    }

    pub fn close(&mut self) {}
}
