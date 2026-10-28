use crate::{commands::catalog::CommandCatalog, session::ids::SessionId};

pub const SKILL_COMMAND: &str = "/talaria-workflows";
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitRoute {
    Plain,
    Workflow {
        command_line: String,
        one_shot: bool,
    },
}
pub fn route_submit(_text: &str, _enabled: bool) -> SubmitRoute {
    todo!()
}
pub fn contains_keyword(_text: &str) -> bool {
    todo!()
}
pub struct UltracodeState;
impl UltracodeState {
    pub fn new(_default_on: bool) -> Self {
        todo!()
    }
    pub fn is_on(&self, _id: &SessionId) -> bool {
        todo!()
    }
    pub fn set(&mut self, _id: &SessionId, _on: bool) {
        todo!()
    }
    pub fn toggle(&mut self, _id: &SessionId) -> bool {
        todo!()
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UltracodeCommand {
    Toggle,
    On,
    Off,
    Status,
}
#[derive(Debug)]
pub struct UsageError;
impl UltracodeCommand {
    pub fn parse(_arg: &str) -> Result<Self, UsageError> {
        todo!()
    }
}
pub enum SkillAvailability {
    Installed { origin: String },
    Missing,
    Collision,
    Unknown { reason: String },
}
pub fn skill_availability(_catalog: &CommandCatalog) -> SkillAvailability {
    todo!()
}
