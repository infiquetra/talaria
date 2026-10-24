//! Performance harness for the Talaria binary: budgets, a pseudo-terminal runner,
//! query responder, frame splitter, replay generator, sampler and report.

#![forbid(unsafe_code)]

pub mod budgets;
pub mod error;
pub mod frames;
pub mod generator;
pub mod measurements;
pub mod pty;
pub mod report;
pub mod responder;
pub mod sampler;
pub mod screen;

pub use budgets::{
    Budgets, Cpu, Idle, Keystroke, Launch, LiveGateway, Memory, Row, Scenario, Status,
    TerminalSize, UnitChoice, WorstFrame, best_of, keystroke_schedule, last_t_ms, median,
    peak_window, percentile_nearest_rank, replay_duration, verdict, window_means,
};
pub use error::BenchError;
pub use frames::{Frame, FrameSplitter, SplitCounts};
pub use generator::generate;
pub use measurements::{RunOptions, exit_code, run};
pub use report::Report;
pub use responder::{Answers, Query, ResponderState, answer_queries};
pub use sampler::{Sample, cpu_percent, sample};
pub use screen::{Emulator, typed_prefix_visible};
