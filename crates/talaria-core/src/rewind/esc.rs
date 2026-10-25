//! Two eligible Esc presses open rewind. Issue 233. Checkpoint rewind and diff.

/// A second eligible press opens rewind only when it is strictly sooner than this.
pub const DOUBLE_ESC_WINDOW_MS: u64 = 600;

/// Memory of the previous eligible Esc.
#[derive(Debug, Default)]
pub struct DoubleEsc {
    last: Option<u64>,
}

impl DoubleEsc {
    /// `true` when this press opens the rewind overlay.
    pub fn press(&mut self, now_ms: u64, eligible: bool) -> bool {
        let _ = (self.last, now_ms, eligible);
        todo!("Issue 233. Checkpoint rewind and diff.")
    }
}
