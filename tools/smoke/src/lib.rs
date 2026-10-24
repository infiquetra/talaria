//! Black-box live smoke test: drives the built binary through the Herdr command line.

pub mod clock;
pub mod config;
pub mod report;
pub mod screen;
pub mod scrub;
pub mod steps;
pub mod terminal;

pub use clock::{Clock, ManualClock, SystemClock};
pub use config::{SmokeConfig, launch_line};
pub use report::{PaneCapture, SmokeReport, StepOutcome};
pub use screen::{
    Card, Token, approval_card, count_token, counting_values, exit_code, highest_number, settled,
};
pub use scrub::{Scrubber, public_safety_violations};
pub use steps::{CleanupError, TabGuard, run_smoke};
pub use terminal::{HerdrCli, NewTab, PaneId, ReadSource, TabId, TermError, Terminal};
