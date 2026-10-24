//! Application loop: one channel, a pure update, and the effect executor.

use std::collections::VecDeque;
use std::future::Future;
use std::path::Path;
use std::time::Instant;

use hermes_client::liveness::LinkEvent;
use hermes_client::replay::ReplayGap;
use hermes_contract::events::EventFrame;
use serde_json::Value;
use talaria_core::session::{Cell, Effect, SessionState};

use crate::launch::Launch;
use crate::sessions::{DetachOutcome, ExecuteOutcome, OpenedSession};

#[derive(Debug)]
pub struct AppState {
    pub opened: Option<OpenedSession>,
    pub held: VecDeque<String>,
    pub launch_id: String,
    pub prompt_counter: u64,
    pub quitting: bool,
    pub committed_cells: usize,
    pub link_failed: bool,
    pub session_gone: bool,
}

impl AppState {
    #[must_use]
    pub fn new(launch_id: impl Into<String>) -> Self {
        Self {
            opened: None,
            held: VecDeque::new(),
            launch_id: launch_id.into(),
            prompt_counter: 0,
            quitting: false,
            committed_cells: 0,
            link_failed: false,
            session_gone: false,
        }
    }
}

#[allow(clippy::large_enum_variant)]
pub enum AppMessage {
    Input(String),
    Resize { rows: u16, cols: u16 },
    Link(LinkEvent),
    Opened(OpenedSession),
    ExecuteDone(ExecuteOutcome),
    Tick(Instant),
    Signal(i32),
    Quit,
}

pub enum AppEffect {
    CommitCells { texts: Vec<String> },
    Redraw,
    CallSession(Effect),
    StartRecovery(ReplayGap),
    ShowNotice(String),
    Quit { code: u8 },
}

pub fn update(state: &mut AppState, message: AppMessage) -> Vec<AppEffect> {
    let _ = (state, message);
    todo!("binary")
}

pub async fn run(launch: Launch) -> u8 {
    let _ = launch;
    todo!("binary")
}

pub trait ComposerDeadline {
    fn tick(&mut self, now: Instant);
    fn next_tick(&self) -> Option<Instant>;
}

pub trait StatusHost {
    fn on_tick(&mut self, now: Instant);
    fn next_wake(&self, now: Instant) -> Option<Instant>;
    fn shutdown(&mut self) -> impl Future<Output = ()> + Send;
}

#[must_use]
pub fn startup_notices(home: &Path) -> Vec<String> {
    let _ = home;
    todo!("binary")
}

pub fn drive_composer(deadline: &mut impl ComposerDeadline, now: Instant) {
    let _ = (deadline, now);
    todo!("binary")
}

pub async fn drive_status(host: &mut impl StatusHost, is_credential_name: fn(&str) -> bool) {
    let _ = (host, is_credential_name);
    todo!("binary")
}

pub fn apply_inbound(
    state: &mut SessionState,
    session_id: &str,
    frame: EventFrame,
    raw: Value,
) -> Vec<Effect> {
    let _ = (state, session_id, frame, raw);
    todo!("binary")
}

pub fn note_connection(state: &mut SessionState, generation: u64) -> Vec<Effect> {
    let _ = (state, generation);
    todo!("binary")
}

pub fn hold_prompt(state: &mut AppState, text: &str) -> Result<(), String> {
    let _ = (state, text);
    todo!("binary")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slash {
    Quit,
    Notice(String),
    NotSlash,
}

#[must_use]
pub fn slash_line(text: &str) -> Slash {
    let _ = text;
    todo!("binary")
}

#[must_use]
pub fn resume_history(cells: &[Cell], title: &str) -> Vec<String> {
    let _ = (cells, title);
    todo!("binary")
}

#[must_use]
pub fn other_folder_notice(cwd: &str) -> String {
    let _ = cwd;
    todo!("binary")
}

pub async fn answer_server_request(name: &str, params: Value) -> Result<Value, String> {
    let _ = (name, params);
    todo!("binary")
}

#[must_use]
pub fn link_notice(event: &LinkEvent) -> Option<String> {
    let _ = event;
    todo!("binary")
}

pub async fn quit(state: &mut AppState) -> (DetachOutcome, u8, String) {
    let _ = state;
    todo!("binary")
}

pub async fn quit_while_opening(state: &mut AppState) -> (DetachOutcome, u8, String) {
    let _ = state;
    todo!("binary")
}

pub async fn interrupt(state: &mut AppState) -> ExecuteOutcome {
    let _ = state;
    todo!("binary")
}
