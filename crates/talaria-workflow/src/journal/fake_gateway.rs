use super::{CreateSession, GatewayPort, PortError, Receipt, SessionBound, SubmitTurn};
use futures_util::future::BoxFuture;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone)]
pub struct FakeGateway;
impl FakeGateway {
    pub fn new() -> Self {
        todo!()
    }
    pub fn with_script(self, _rules: Vec<ScriptRule>) -> Self {
        todo!()
    }
    pub fn autosave(self, _path: PathBuf) -> Self {
        todo!()
    }
    pub fn save_state(&self, _path: &Path) -> std::io::Result<()> {
        todo!()
    }
    pub fn load_state(_path: &Path) -> std::io::Result<Self> {
        todo!()
    }
    pub fn release(&self, _hold: &str) {
        todo!()
    }
    pub fn restart(&self) {
        todo!()
    }
    pub fn disconnect(&self) {
        todo!()
    }
    pub fn delete_session(&self, _session_id: &str) {
        todo!()
    }
    pub fn fail_next(&self, _n: u32, _error: PortError) {
        todo!()
    }
    pub fn stats(&self) -> FakeStats {
        todo!()
    }
}
impl Default for FakeGateway {
    fn default() -> Self {
        todo!()
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptRule {
    pub label: Option<String>,
    pub model: Option<String>,
    pub text_contains: Option<String>,
    pub outcome: FakeOutcome,
    pub final_text: String,
    pub usage: Value,
    pub duration: Duration,
    pub hold: Option<String>,
    pub settlement_lost: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FakeOutcome {
    Completed,
    Failed,
    Interrupted,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FakeStats {
    pub sessions: u32,
    pub admissions: u32,
    pub receipts: u32,
    pub resolves: u32,
    pub attaches: u32,
    pub calls: Vec<String>,
    pub admissions_by_session: BTreeMap<String, Vec<String>>,
}
impl GatewayPort for FakeGateway {
    fn create_session(
        &self,
        _req: CreateSession,
    ) -> BoxFuture<'static, Result<SessionBound, PortError>> {
        todo!()
    }
    fn attach(
        &self,
        _profile: String,
        _session_id: String,
    ) -> BoxFuture<'static, Result<(), PortError>> {
        todo!()
    }
    fn submit(&self, _req: SubmitTurn) -> BoxFuture<'static, Result<Receipt, PortError>> {
        todo!()
    }
    fn receipt(
        &self,
        _profile: String,
        _session_id: String,
        _admission_id: String,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        todo!()
    }
    fn wait_settled(
        &self,
        _profile: String,
        _session_id: String,
        _admission_id: String,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        todo!()
    }
    fn resolve_unknown(
        &self,
        _profile: String,
        _session_id: String,
        _admission_id: String,
        _generation: i64,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        todo!()
    }
}
