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

/// A second press of the interrupt key confirms only when it is strictly sooner than this after
/// the first press, and the working row's "again" hint lasts the same time.
pub const INTERRUPT_CONFIRM_WINDOW_MS: u64 = 2_000;

/// What one press of the interrupt key aimed at a running turn did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterruptPress {
    /// The first press: nothing is interrupted, and the hint asks for the key again.
    Armed,
    /// The second press inside the window: interrupt the turn.
    Confirmed,
}

/// Memory of the last unconfirmed press of the interrupt key. Pure: the caller gives the time.
#[derive(Debug, Default)]
pub struct InterruptConfirm {
    armed_at: Option<u64>,
}

impl InterruptConfirm {
    /// One press of the interrupt key aimed at a running turn.
    pub fn press(&mut self, now_ms: u64) -> InterruptPress {
        if self.is_armed(now_ms) {
            self.armed_at = None;
            InterruptPress::Confirmed
        } else {
            self.armed_at = Some(now_ms);
            InterruptPress::Armed
        }
    }

    /// Whether a press at `now_ms` would interrupt, which is when the hint says "again".
    #[must_use]
    pub fn is_armed(&self, now_ms: u64) -> bool {
        self.armed_at
            .is_some_and(|armed| now_ms.saturating_sub(armed) < INTERRUPT_CONFIRM_WINDOW_MS)
    }

    /// When the armed state ends, if one is armed: the time to wake and draw the hint again.
    #[must_use]
    pub fn expires_at(&self) -> Option<u64> {
        self.armed_at
            .map(|armed| armed.saturating_add(INTERRUPT_CONFIRM_WINDOW_MS))
    }

    /// Forget an armed press whose window has passed.
    pub fn expire(&mut self, now_ms: u64) {
        if self.armed_at.is_some() && !self.is_armed(now_ms) {
            self.armed_at = None;
        }
    }

    /// Forget any armed press: the turn ended or the session changed.
    pub fn reset(&mut self) {
        self.armed_at = None;
    }
}
