use serde_json::Value;
use talaria_core::{
    commands::{CommandCatalog, ErrorView},
    session::ids::SessionId,
    ultracode::UltracodeCommand,
};
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    FetchCatalog {
        session: SessionId,
        profile: String,
        command: UltracodeCommand,
    },
    RunCommand {
        session: SessionId,
        original_text: String,
        command_line: String,
    },
    Notice(String),
    SubmitCommand {
        text: String,
        display: String,
        submission_id: String,
    },
    RestoreComposer(String),
    Changed,
}
pub struct Foreground<'a> {
    pub session: &'a SessionId,
    pub profile: &'a str,
}
pub struct UltracodeService;
impl UltracodeService {
    pub fn new(_default_on: bool) -> Self {
        todo!()
    }
    pub fn handle_local_command(
        &mut self,
        _arg: &str,
        _foreground: Option<Foreground<'_>>,
    ) -> Vec<Action> {
        todo!()
    }
    pub fn on_catalog_reply(
        &mut self,
        _session: &SessionId,
        _profile: &str,
        _command: UltracodeCommand,
        _result: Result<CommandCatalog, String>,
    ) -> Vec<Action> {
        todo!()
    }
    pub fn intercept_submit(&self, _session: &SessionId, _text: &str) -> Option<Vec<Action>> {
        todo!()
    }
    pub fn on_command_outcome(
        &self,
        _session: &SessionId,
        _original_text: &str,
        _outcome: Result<Value, ErrorView>,
    ) -> Vec<Action> {
        todo!()
    }
    pub fn status_input(&self, _foreground: Option<&SessionId>) -> bool {
        todo!()
    }
}
