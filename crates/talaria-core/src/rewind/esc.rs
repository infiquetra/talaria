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
        if !eligible {
            self.last = None;
            return false;
        }
        match self.last {
            Some(previous) if now_ms.saturating_sub(previous) < DOUBLE_ESC_WINDOW_MS => {
                self.last = None;
                true
            }
            _ => {
                self.last = Some(now_ms);
                false
            }
        }
    }
}
