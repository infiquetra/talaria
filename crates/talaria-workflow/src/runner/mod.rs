//! Headless workflow lifecycle, discovery, and durable run controls.

pub mod control;
pub mod execute;
mod files;
#[cfg(feature = "mock")]
pub mod mock_support;
mod recording;
pub mod resolver;
pub mod runs;
mod services;

pub use control::{
    AnswerError, ControlWrite, PausingGate, StopExit, answer_parked, request_stop, write_control,
};
pub use execute::{
    ResumeRequest, RunExit, RunRequest, RunnerEvent, SpendView, run_child, run_new, run_resume,
};
#[cfg(feature = "mock")]
pub use execute::{run_new_with_services, run_resume_with_services};
pub use resolver::FilesystemResolver;
pub use runs::{
    FindError, RunStatus, RunSummary, discover_runs, find_run, runs_dir, workflows_dir,
};
pub use services::RunnerServices;

/// Discovery rows and selection for the progress viewer's run picker.
#[derive(Clone, Debug)]
pub struct ProgressPicker {
    rows: Vec<RunSummary>,
    selected: usize,
}
impl ProgressPicker {
    pub fn load(runs_dir: &std::path::Path) -> Self {
        Self {
            rows: discover_runs(runs_dir),
            selected: 0,
        }
    }
    pub fn rows(&self) -> &[RunSummary] {
        &self.rows
    }
    pub fn empty_message(&self) -> Option<&'static str> {
        self.rows
            .is_empty()
            .then_some("no runs yet — talaria workflow run starts one")
    }
    pub fn selected_index(&self) -> usize {
        self.selected
    }
    pub fn selected(&self) -> Option<&RunSummary> {
        self.rows.get(self.selected)
    }
    pub fn move_selection(&mut self, down: bool) {
        self.selected = if down {
            self.selected
                .saturating_add(1)
                .min(self.rows.len().saturating_sub(1))
        } else {
            self.selected.saturating_sub(1)
        };
    }
}
