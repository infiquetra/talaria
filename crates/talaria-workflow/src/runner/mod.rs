//! Workflow runner lifecycle and file discovery.

pub mod control;
pub mod execute;
#[cfg(feature = "mock")]
pub mod mock_support;
pub mod resolver;
pub mod runs;

pub use control::{
    AnswerError, ControlWrite, PausingGate, StopExit, answer_parked, request_stop, write_control,
};
pub use execute::{
    ResumeRequest, RunExit, RunRequest, RunnerEvent, SpendView, run_child, run_new, run_resume,
};
#[cfg(feature = "mock")]
pub use execute::{RunnerServices, run_new_with_services, run_resume_with_services};
pub use resolver::FilesystemResolver;
pub use runs::{
    FindError, RunStatus, RunSummary, discover_runs, find_run, runs_dir, workflows_dir,
};

/// The run-picker state used by the progress viewer.
pub struct ProgressPicker;
impl ProgressPicker {
    pub fn load(_runs_dir: &std::path::Path) -> Self {
        unimplemented!()
    }
    pub fn rows(&self) -> &[RunSummary] {
        unimplemented!()
    }
    pub fn empty_message(&self) -> Option<&'static str> {
        unimplemented!()
    }
}
