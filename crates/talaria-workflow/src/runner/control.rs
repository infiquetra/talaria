use super::runs::FindError;
use crate::{
    engine::{AgentRequest, BudgetSnapshot},
    journal::{AdmissionGate, GateDecision, GateReason, JournalError, Resolution, ResolvedLaunch},
    schedule::Scheduler,
};
use std::{path::Path, time::Duration};

pub struct PausingGate;
impl PausingGate {
    pub fn new(_inner: Scheduler) -> Self {
        unimplemented!()
    }
    pub fn set_paused(&self, _paused: bool) {
        unimplemented!()
    }
}
impl AdmissionGate for PausingGate {
    fn acquire(
        &self,
        _request: &AgentRequest,
        _launch: &ResolvedLaunch,
        _reason: GateReason,
    ) -> futures::future::BoxFuture<'static, GateDecision> {
        unimplemented!()
    }
    fn budget(&self) -> BudgetSnapshot {
        unimplemented!()
    }
}

pub fn write_control(
    _dir: &Path,
    _kind: &str,
    _key: Option<&str>,
    _at_ms: i64,
) -> std::io::Result<ControlWrite> {
    unimplemented!()
}
pub enum ControlWrite {
    Written,
    AlreadyRequested,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StopExit {
    Stopped,
    AlreadyFinished { outcome: String },
    Unconfirmed,
    UnknownId,
}
pub fn request_stop(_runs_dir: &Path, _target: &str, _timeout: Duration) -> StopExit {
    unimplemented!()
}

#[derive(Debug)]
pub enum AnswerError {
    Find(FindError),
    Journal(JournalError),
}
pub fn answer_parked(
    _runs_dir: &Path,
    _target: &str,
    _key: &str,
    _attempt: u32,
    _episode: u32,
    _resolution: Resolution,
) -> Result<(), AnswerError> {
    unimplemented!()
}
