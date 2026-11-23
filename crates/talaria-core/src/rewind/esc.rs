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

/// A second press of the interrupt key counts only when it is strictly sooner than this after
/// the first press, and the working row's "again" hint lasts the same time.
pub const INTERRUPT_CONFIRM_WINDOW_MS: u64 = 2_000;

/// How long a second press waits, with no further press, before it interrupts the turn.
///
/// A terminal sends a held key as a stream of ordinary presses, because Talaria asks only for the
/// basic keyboard mode and so repeats are not marked. Auto-repeat streams are regular and fast,
/// about 30 to 90 ms between events at the default and faster settings, while people rarely tap
/// twice within this time. A second press therefore waits this long: a further press inside it
/// means the key is held and nothing is interrupted. A keyboard repeat rate slower than this time
/// is not covered; such a stream looks like separate taps.
pub const INTERRUPT_SETTLE_MS: u64 = 110;

/// What one press of the interrupt key aimed at a running turn did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterruptPress {
    /// The first press: nothing is interrupted, and the hint asks for the key again.
    Armed,
    /// The second press inside the window: the turn is interrupted when the settle time ends, if
    /// no further press arrives first. The hint still asks for the key again until then.
    Pending,
    /// A press right after the pending one: the key is held, so the pending interrupt is
    /// cancelled.
    Held,
    /// A further press of a held key: nothing changes.
    Ignored,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Idle,
    /// A first press at this time; a second inside the window makes it pending.
    Armed(u64),
    /// A second press at this time; the interrupt is due when the settle time passes.
    Pending(u64),
    /// The last press of a held key was at this time; the state ends when the settle time passes
    /// with no further press.
    Held(u64),
}

/// Memory of the last presses of the interrupt key. Pure: the caller gives the time and wakes at
/// [`InterruptConfirm::next_deadline`] to call [`InterruptConfirm::tick`]. A caller that gets a
/// press before the wake ticks first, so a settled interrupt is never lost.
#[derive(Debug, Default)]
pub struct InterruptConfirm {
    phase: Phase,
}

impl InterruptConfirm {
    /// One press of the interrupt key aimed at a running turn. Nothing is interrupted here: an
    /// interrupt comes only from [`InterruptConfirm::tick`].
    pub fn press(&mut self, now_ms: u64) -> InterruptPress {
        let (phase, result) = match self.phase {
            Phase::Armed(at) if now_ms.saturating_sub(at) < INTERRUPT_CONFIRM_WINDOW_MS => {
                (Phase::Pending(now_ms), InterruptPress::Pending)
            }
            Phase::Pending(at) if now_ms.saturating_sub(at) < INTERRUPT_SETTLE_MS => {
                (Phase::Held(now_ms), InterruptPress::Held)
            }
            Phase::Held(at) if now_ms.saturating_sub(at) < INTERRUPT_SETTLE_MS => {
                (Phase::Held(now_ms), InterruptPress::Ignored)
            }
            // Idle, or a state whose time has passed: this is a first press.
            _ => (Phase::Armed(now_ms), InterruptPress::Armed),
        };
        self.phase = phase;
        result
    }

    /// Move to `now_ms`. Returns `true` exactly once when a pending press has settled: the caller
    /// interrupts the turn now. An armed state whose window has passed, and a held state whose
    /// settle time has passed, end here.
    #[must_use]
    pub fn tick(&mut self, now_ms: u64) -> bool {
        match self.phase {
            Phase::Pending(at) if now_ms.saturating_sub(at) >= INTERRUPT_SETTLE_MS => {
                self.phase = Phase::Idle;
                true
            }
            Phase::Armed(at) if now_ms.saturating_sub(at) >= INTERRUPT_CONFIRM_WINDOW_MS => {
                self.phase = Phase::Idle;
                false
            }
            Phase::Held(at) if now_ms.saturating_sub(at) >= INTERRUPT_SETTLE_MS => {
                self.phase = Phase::Idle;
                false
            }
            _ => false,
        }
    }

    /// Whether the working row's hint says "again": a first press is waiting for its second, or a
    /// second press is waiting out the settle time.
    #[must_use]
    pub fn is_armed(&self, now_ms: u64) -> bool {
        match self.phase {
            Phase::Armed(at) => now_ms.saturating_sub(at) < INTERRUPT_CONFIRM_WINDOW_MS,
            Phase::Pending(_) => true,
            Phase::Idle | Phase::Held(_) => false,
        }
    }

    /// When to wake and call [`InterruptConfirm::tick`]: the armed window's end, the pending
    /// press's settle time, or the held state's end. `None` when idle.
    #[must_use]
    pub fn next_deadline(&self) -> Option<u64> {
        match self.phase {
            Phase::Idle => None,
            Phase::Armed(at) => Some(at.saturating_add(INTERRUPT_CONFIRM_WINDOW_MS)),
            Phase::Pending(at) | Phase::Held(at) => Some(at.saturating_add(INTERRUPT_SETTLE_MS)),
        }
    }

    /// Forget every press: the turn ended or the session changed.
    pub fn reset(&mut self) {
        self.phase = Phase::Idle;
    }
}
