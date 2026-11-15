//! Workflow routing decisions; the application executes every read through its event channel.
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use talaria_core::{
    commands::{CommandCatalog, CommandOutcome, ErrorView, SlashLine, describe_error},
    session::SessionId,
    ultracode::{
        SKILL_COMMAND, SkillAvailability, SubmitRoute, UltracodeCommand, UltracodeState, notice,
        route_submit, skill_availability,
    },
};
#[derive(Debug, Clone, PartialEq, Eq)]
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
    InstallSkill {
        session: SessionId,
        profile: String,
    },
    Changed,
}
pub struct Foreground<'a> {
    pub session: &'a SessionId,
    pub profile: &'a str,
}
pub struct UltracodeService {
    state: UltracodeState,
    profiles: HashMap<SessionId, String>,
    asked: HashSet<SessionId>,
    kept: HashSet<SessionId>,
    pending: HashMap<SessionId, String>,
    remote: bool,
}
impl Default for UltracodeService {
    fn default() -> Self {
        Self::new(false)
    }
}
impl UltracodeService {
    pub fn new(default_on: bool) -> Self {
        Self {
            state: UltracodeState::new(default_on),
            profiles: HashMap::new(),
            asked: HashSet::new(),
            kept: HashSet::new(),
            pending: HashMap::new(),
            remote: false,
        }
    }
    pub fn state(&self) -> &UltracodeState {
        &self.state
    }
    pub fn set_remote(&mut self, remote: bool) {
        self.remote = remote;
    }
    pub fn record_shown(&mut self, session: &SessionId) {
        let profile = self.profiles.get(session).map_or("default", String::as_str);
        self.pending.insert(session.clone(), profile.to_owned());
        self.asked.insert(session.clone());
        self.kept.remove(session);
    }
    pub fn show_kept(&mut self, session: &SessionId) -> Option<String> {
        if !self.kept.contains(session) {
            return None;
        }
        let profile = self.profiles.get(session).map_or("default", String::as_str);
        let text = notice::offer(profile);
        self.record_shown(session);
        Some(text)
    }
    pub fn answer_install(&mut self, session: &SessionId, line: &str) -> Option<Vec<Action>> {
        let profile = self.pending.remove(session)?;
        Some(vec![if line.trim() == "y" {
            Action::InstallSkill {
                session: session.clone(),
                profile,
            }
        } else {
            Action::Notice(notice::missing(&profile))
        }])
    }
    pub fn bind(&mut self, session: &SessionId, profile: &str) {
        self.profiles.insert(session.clone(), profile.into());
    }
    pub fn handle_local_command(
        &mut self,
        arg: &str,
        foreground: Option<Foreground<'_>>,
    ) -> Vec<Action> {
        let command = match UltracodeCommand::parse(arg) {
            Ok(c) => c,
            Err(_) => return vec![Action::Notice(notice::USAGE.into())],
        };
        let Some(Foreground { session, profile }) = foreground else {
            return vec![Action::Notice(notice::NO_SESSION.into())];
        };
        self.bind(session, profile);
        if command == UltracodeCommand::Off
            || (command == UltracodeCommand::Toggle && self.state.is_on(session))
        {
            self.state.set(session, false);
            vec![Action::Notice(notice::OFF.into()), Action::Changed]
        } else {
            vec![Action::FetchCatalog {
                session: session.clone(),
                profile: profile.into(),
                command,
            }]
        }
    }
    pub fn on_catalog_reply(
        &mut self,
        session: &SessionId,
        profile: &str,
        command: UltracodeCommand,
        result: Result<CommandCatalog, String>,
    ) -> Vec<Action> {
        self.bind(session, profile);
        let availability = match result {
            Ok(c) => skill_availability(&c),
            Err(reason) => SkillAvailability::Unknown { reason },
        };
        if !matches!(availability, SkillAvailability::Missing) {
            self.kept.remove(session);
        }
        if command == UltracodeCommand::Status {
            return vec![Action::Notice(notice::status(
                self.state.is_on(session),
                profile,
                &availability,
            ))];
        }
        let text = match availability {
            SkillAvailability::Installed { .. } => {
                self.state.set(session, true);
                return vec![Action::Notice(notice::ON.into()), Action::Changed];
            }
            SkillAvailability::Missing if self.remote => format!(
                "{}. The skill must be installed on the machine that runs the gateway.",
                notice::missing(profile)
            ),
            SkillAvailability::Missing if !self.asked.contains(session) => {
                self.kept.insert(session.clone());
                notice::offer(profile)
            }
            SkillAvailability::Missing => notice::missing(profile),
            SkillAvailability::Collision => notice::collision(profile),
            SkillAvailability::Unknown { reason } => notice::unknown(&reason),
        };
        vec![Action::Notice(text)]
    }
    pub fn intercept_submit(&self, session: &SessionId, text: &str) -> Option<Vec<Action>> {
        match route_submit(text, self.state.is_on(session)) {
            SubmitRoute::Plain => None,
            SubmitRoute::Workflow {
                command_line,
                one_shot,
            } => {
                let mut actions = vec![];
                if one_shot {
                    actions.push(Action::Notice(notice::ONE_SHOT.into()));
                }
                actions.push(Action::RunCommand {
                    session: session.clone(),
                    original_text: text.into(),
                    command_line,
                });
                Some(actions)
            }
        }
    }
    pub fn on_command_outcome(
        &self,
        session: &SessionId,
        original_text: &str,
        outcome: Result<Value, ErrorView>,
    ) -> Vec<Action> {
        let profile = self.profiles.get(session).map_or("default", String::as_str);
        let failure = match outcome {
            Ok(value) if value["type"] == "skill" && value["name"] == "talaria-workflows" => {
                if let (Some(text), Some(display)) = (
                    value["message"].as_str().filter(|s| !s.trim().is_empty()),
                    value["display"].as_str(),
                ) {
                    let mut actions = vec![];
                    if let Some(notice) = value["notice"].as_str().filter(|s| !s.trim().is_empty())
                    {
                        actions.push(Action::Notice(notice.into()));
                    }
                    actions.push(Action::SubmitCommand {
                        text: text.into(),
                        display: display.into(),
                        submission_id: crate::commands::fresh_submission_id(),
                    });
                    return actions;
                }
                notice::unavailable(profile)
            }
            Ok(_) => notice::collision(profile),
            Err(ErrorView::Rpc {
                reason: Some(reason),
                ..
            }) if reason == "unsupported_command" => {
                if self.remote {
                    format!(
                        "{}. The skill must be installed on the machine that runs the gateway.",
                        notice::missing(profile)
                    )
                } else if self.asked.contains(session) {
                    notice::missing(profile)
                } else {
                    notice::offer(profile)
                }
            }
            Err(ErrorView::Rpc {
                reason: Some(reason),
                ..
            }) if reason == "command_unavailable" => notice::unavailable(profile),
            Err(error) => {
                let line = SlashLine {
                    name: SKILL_COMMAND.trim_start_matches('/').into(),
                    arg: original_text.into(),
                };
                match describe_error(&line, &error, None) {
                    CommandOutcome::Refused { message, .. }
                    | CommandOutcome::Failed { message }
                    | CommandOutcome::OutcomeUnknown { message } => message,
                    _ => "The workflow command did not return a skill.".into(),
                }
            }
        };
        vec![
            Action::Notice(failure),
            Action::RestoreComposer(original_text.into()),
        ]
    }
    pub fn status_input(&self, foreground: Option<&SessionId>) -> bool {
        foreground.is_some_and(|id| self.state.is_on(id))
    }
}
