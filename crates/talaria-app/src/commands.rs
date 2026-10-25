//! Slash-command service: catalogue fetches, completion, and `slash.exec`. Issue #223.
//!
//! The decisions live in `talaria_core::commands`; this module runs the calls they ask for.
//! Talaria never re-implements Hermes's command logic: it runs every Hermes command through
//! `slash.exec` and shows what comes back.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use hermes_client::{CallError, CallOptions, Client};
use hermes_contract::methods::legacy::{
    command_dispatch, commands_catalog, complete_slash, slash_exec,
};
use serde_json::Value;
use talaria_core::commands::{
    CommandCatalog, CommandOutcome, CommandsEffect, CommandsInput, CommandsState, ErrorView,
    LocalCommandTable, PaletteContext, PaletteEffect, PaletteInput, PaletteState, SlashLine,
    describe_error, interpret_result, palette_update, parse_slash_line, state_update,
};
use talaria_core::session::{Event, NoticeKind};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::app::UserAction;
use crate::launch::new_launch_id;
use crate::sessions::{submission_id, submit_params};

/// `commands.catalog` gives up after this long.
pub const CATALOG_TIMEOUT: Duration = Duration::from_secs(15);
/// `complete.slash` gives up after this long.
pub const COMPLETE_TIMEOUT: Duration = Duration::from_secs(5);

/// The notice for a Hermes command typed while no session is attached. Nothing is sent.
#[must_use]
pub fn no_session_notice(name: &str) -> String {
    format!(
        "No session is attached, so /{name} cannot run. Start one with talaria, talaria -c or \
         talaria --resume <id>."
    )
}

/// The notice when neither `slash.exec` nor `command.dispatch` exists on the gateway.
pub const NO_SLASH_METHODS: &str = "This Hermes gateway does not run slash commands. Check that \
                                    it is at the release Talaria is pinned to.";

/// What a slash outcome asks the application loop to apply.
///
/// These are the session events of issue #223, fed through the existing
/// [`crate::app::AppMessage`] path. This is not a second event channel.
#[allow(clippy::large_enum_variant)]
pub enum AppAction {
    /// Apply one session event.
    Event(Event),
    /// Load the composer.
    LoadDraft(String),
    /// Remember a bare name that was refused.
    Refused { name: String },
    /// A foreground key already turned into a person action.
    Key(UserAction),
    /// The catalogue store changed, so the palette rows are derived again.
    CatalogUpdated,
}

/// What a background call of the service came back with.
#[allow(clippy::large_enum_variant)]
pub enum CommandReply {
    /// A catalogue fetch finished.
    Commands(CommandsInput),
    /// A completion request finished.
    Palette(PaletteInput),
}

/// A fresh `submission_id` for a prompt a slash command builds.
#[must_use]
pub fn fresh_submission_id() -> String {
    static LAUNCH: OnceLock<String> = OnceLock::new();
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let launch = LAUNCH.get_or_init(new_launch_id);
    submission_id(launch, COUNTER.fetch_add(1, Ordering::SeqCst) + 1)
}

/// A short plain reason for a failed read, for the palette's footer and failure row.
fn reason_of(error: &CallError) -> String {
    match error {
        CallError::Rpc(error) => error
            .reason
            .clone()
            .filter(|reason| !reason.is_empty())
            .or_else(|| (!error.message.is_empty()).then(|| error.message.clone()))
            .unwrap_or_else(|| format!("code {}", error.code)),
        CallError::TimedOut { .. } => "no answer in time".to_owned(),
        CallError::Unknown { .. } => "the connection dropped".to_owned(),
        CallError::NotConnected => "not connected".to_owned(),
        CallError::BadResult { .. } => "the reply did not decode".to_owned(),
        CallError::Encode | CallError::InvalidTimeout => "Talaria could not send it".to_owned(),
    }
}

fn data_text(data: Option<&Value>, field: &str) -> Option<String> {
    data.and_then(|data| data.get(field))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// The client's error as the outcome rules read it.
#[must_use]
pub fn error_view(error: CallError) -> ErrorView {
    match error {
        CallError::Rpc(error) => ErrorView::Rpc {
            code: error.code,
            field: data_text(error.data.as_ref(), "field"),
            detail: data_text(error.data.as_ref(), "detail"),
            message: error.message,
            reason: error.reason,
        },
        CallError::TimedOut { .. } => ErrorView::TimedOut,
        CallError::Unknown { .. } => ErrorView::ConnectionLost,
        CallError::NotConnected => ErrorView::NotConnected,
        CallError::BadResult { .. } => ErrorView::BadResult,
        CallError::Encode | CallError::InvalidTimeout => ErrorView::LocalBug,
    }
}

fn session(session_id: Option<String>) -> Option<String> {
    session_id.filter(|id| !id.is_empty())
}

async fn fetch(client: &Client, request_id: u64, session_id: Option<String>) -> CommandsInput {
    let params = commands_catalog::Params {
        profile: None,
        session_id: session(session_id),
    };
    let options = CallOptions {
        timeout: Some(CATALOG_TIMEOUT),
    };
    match client
        .call_with::<commands_catalog::Marker>(params, options)
        .await
    {
        Ok(result) => CommandsInput::CatalogArrived { request_id, result },
        Err(error) => CommandsInput::CatalogFailed {
            request_id,
            reason: reason_of(&error),
        },
    }
}

async fn complete(
    client: &Client,
    request_id: u64,
    text: String,
    session_id: Option<String>,
) -> PaletteInput {
    let params = complete_slash::Params {
        profile: None,
        session_id: session(session_id),
        text: Some(text),
    };
    let options = CallOptions {
        timeout: Some(COMPLETE_TIMEOUT),
    };
    match client
        .call_with::<complete_slash::Marker>(params, options)
        .await
    {
        Ok(result) => PaletteInput::CompletionArrived { request_id, result },
        Err(error) => PaletteInput::CompletionFailed {
            request_id,
            reason: reason_of(&error),
        },
    }
}

/// Runs one typed line through `slash.exec`, its fallback, and one alias. Cloning is cheap
/// enough to give each command its own runner in a spawned task.
#[derive(Clone)]
pub struct CommandRunner {
    client: Client,
    catalog: Option<CommandCatalog>,
}

impl CommandRunner {
    /// A runner that words refusals against `catalog`, the one held on this run, if any.
    #[must_use]
    pub fn new(client: Client, catalog: Option<CommandCatalog>) -> Self {
        Self { client, catalog }
    }

    /// The catalogue the runner holds after its calls.
    #[must_use]
    pub fn catalog(&self) -> Option<&CommandCatalog> {
        self.catalog.as_ref()
    }

    /// Send the line to Hermes and say what its answer means. Never submits a prompt: a
    /// `send` or `skill` directive comes back as [`CommandOutcome::Submit`]. An alias is
    /// followed once; a second alias in the same call stops with plain words.
    pub async fn dispatch(&mut self, session_id: &str, typed: &str) -> CommandOutcome {
        let mut typed = typed.to_owned();
        let mut first: Option<String> = None;
        loop {
            let Some(line) = parse_slash_line(&typed) else {
                return CommandOutcome::Nothing;
            };
            if line.name.is_empty() {
                return CommandOutcome::Nothing;
            }
            match self.run_once(session_id, &typed, &line).await {
                CommandOutcome::Rerun { line: next } => {
                    if let Some(name) = first {
                        return CommandOutcome::Failed {
                            message: format!(
                                "/{name} points to another alias; Talaria stops after one."
                            ),
                        };
                    }
                    first = Some(line.name);
                    typed = next;
                }
                outcome => return outcome,
            }
        }
    }

    async fn run_once(
        &mut self,
        session_id: &str,
        typed: &str,
        line: &SlashLine,
    ) -> CommandOutcome {
        let params = slash_exec::Params {
            command: typed.trim_start_matches('/').to_owned(),
            profile: None,
            session_id: session_id.to_owned(),
        };
        let reply = match serde_json::to_value(params) {
            Ok(params) => {
                self.client
                    .call_raw(slash_exec::NAME, params, CallOptions::default())
                    .await
            }
            Err(_) => Err(CallError::Encode),
        };
        match reply {
            Ok(result) => interpret_result(line, typed, &result),
            // Only a missing method falls back, and only once for this command: a refusal is
            // the answer, and a timeout may already have run it.
            Err(CallError::Rpc(error)) if error.is_method_not_found() => {
                self.fall_back(session_id, typed, line).await
            }
            Err(error) => self.describe(session_id, line, error).await,
        }
    }

    async fn fall_back(
        &mut self,
        session_id: &str,
        typed: &str,
        line: &SlashLine,
    ) -> CommandOutcome {
        let params = command_dispatch::Params {
            arg: (!line.arg.is_empty()).then(|| line.arg.clone()),
            name: line.name.clone(),
            profile: None,
            session_id: Some(session_id.to_owned()),
        };
        let reply = match serde_json::to_value(params) {
            Ok(params) => {
                self.client
                    .call_raw(command_dispatch::NAME, params, CallOptions::default())
                    .await
            }
            Err(_) => Err(CallError::Encode),
        };
        match reply {
            Ok(result) => interpret_result(line, typed, &result),
            Err(CallError::Rpc(error)) if error.is_method_not_found() => CommandOutcome::Failed {
                message: NO_SLASH_METHODS.to_owned(),
            },
            Err(error) => self.describe(session_id, line, error).await,
        }
    }

    /// Plain words for a failure. Telling an unknown name from a known one needs a catalogue;
    /// when none has arrived yet, one is fetched first, and if that fails the wording for "no
    /// catalogue" is used.
    async fn describe(
        &mut self,
        session_id: &str,
        line: &SlashLine,
        error: CallError,
    ) -> CommandOutcome {
        let view = error_view(error);
        let unsupported = matches!(
            &view,
            ErrorView::Rpc { reason: Some(reason), .. } if reason == "unsupported_command"
        );
        if unsupported
            && self.catalog.is_none()
            && let CommandsInput::CatalogArrived { result, .. } =
                fetch(&self.client, 0, Some(session_id.to_owned())).await
        {
            self.catalog = Some(CommandCatalog::from_result(&result));
        }
        describe_error(line, &view, self.catalog.as_ref())
    }
}

/// Runs catalogue fetches, completion, and `slash.exec`. Issue #223.
///
/// [`handle_input`](Self::handle_input) and [`execute`](Self::execute) drive the service's own
/// catalogue store, for a caller without an application loop. The application keeps its store
/// beside the session state and uses the call methods below.
pub struct CommandService {
    client: Client,
    state: CommandsState,
    local: LocalCommandTable,
    replies: mpsc::UnboundedSender<CommandReply>,
    receiver: Option<mpsc::UnboundedReceiver<CommandReply>>,
    /// The completion waiting out its delay, or in flight.
    pending: Option<JoinHandle<()>>,
    /// A catalogue fetch waiting out its delay.
    delayed: Option<JoinHandle<()>>,
    /// The draft palette of [`draft_changed`](Self::draft_changed).
    drafts: PaletteState,
    /// A catalogue fetched only to word a refusal, kept until the store holds one.
    wording: Option<CommandCatalog>,
}

impl CommandService {
    /// A service on this client. The catalogue starts unloaded.
    #[must_use]
    pub fn new(client: Client) -> Self {
        let (replies, receiver) = mpsc::unbounded_channel();
        Self {
            client,
            state: CommandsState::default(),
            local: LocalCommandTable::builtin(),
            replies,
            receiver: Some(receiver),
            pending: None,
            delayed: None,
            drafts: PaletteState::closed(),
            wording: None,
        }
    }

    /// The catalogue store.
    #[must_use]
    pub fn state(&self) -> &CommandsState {
        &self.state
    }

    /// The replies of background fetches and completions, once. The application forwards
    /// them into its own message channel.
    pub fn take_replies(&mut self) -> Option<mpsc::UnboundedReceiver<CommandReply>> {
        self.receiver.take()
    }

    /// Drive the catalogue state machine. Fetches it asks for start in the background and
    /// come back through [`take_replies`](Self::take_replies).
    #[must_use]
    pub fn handle_input(&mut self, input: CommandsInput) -> Vec<AppAction> {
        let mut actions = Vec::new();
        for effect in state_update(&mut self.state, input) {
            match effect {
                CommandsEffect::FetchCatalog {
                    request_id,
                    session_id,
                    after,
                } => self.start_fetch(request_id, session_id, after),
                CommandsEffect::CatalogUpdated => actions.push(AppAction::CatalogUpdated),
            }
        }
        actions
    }

    /// Start a catalogue fetch, after `after`. A newer fetch cancels a delayed one.
    pub fn start_fetch(&mut self, request_id: u64, session_id: Option<String>, after: Duration) {
        if let Some(delayed) = self.delayed.take() {
            delayed.abort();
        }
        let client = self.client.clone();
        let replies = self.replies.clone();
        let task = tokio::spawn(async move {
            if !after.is_zero() {
                tokio::time::sleep(after).await;
            }
            let input = fetch(&client, request_id, session_id).await;
            let _ = replies.send(CommandReply::Commands(input));
        });
        if !after.is_zero() {
            self.delayed = Some(task);
        }
    }

    /// Fetch the catalogue. The timeout is 15 seconds.
    pub async fn fetch_catalog(
        &self,
        request_id: u64,
        session_id: Option<String>,
    ) -> CommandsInput {
        fetch(&self.client, request_id, session_id).await
    }

    /// Complete one draft. The timeout is 5 seconds.
    pub async fn complete(
        &self,
        request_id: u64,
        text: String,
        session_id: Option<String>,
    ) -> PaletteInput {
        complete(&self.client, request_id, text, session_id).await
    }

    /// Request a completion `after` from now, replacing any pending one: the request leaves
    /// once the draft has been still for that long.
    pub fn request_completion(
        &mut self,
        request_id: u64,
        text: String,
        after: Duration,
        session_id: Option<String>,
    ) {
        self.cancel_completion();
        let client = self.client.clone();
        let replies = self.replies.clone();
        self.pending = Some(tokio::spawn(async move {
            tokio::time::sleep(after).await;
            let input = complete(&client, request_id, text, session_id).await;
            let _ = replies.send(CommandReply::Palette(input));
        }));
    }

    /// Drop the pending completion, if any. Its reply would be stale.
    pub fn cancel_completion(&mut self) {
        if let Some(pending) = self.pending.take() {
            pending.abort();
        }
    }

    /// A runner for one command, wording refusals against `catalog`.
    #[must_use]
    pub fn runner(&self, catalog: Option<CommandCatalog>) -> CommandRunner {
        CommandRunner::new(self.client.clone(), catalog)
    }

    /// Run one typed line. An alias is followed once inside this call.
    ///
    /// For a caller without the session pipeline: a `send` or `skill` directive's prompt is
    /// submitted here, with a fresh `submission_id`. The application never calls this; it
    /// dispatches with a [`CommandRunner`] and submits through the session reducer.
    pub async fn execute(&mut self, session_id: &str, typed: &str) -> CommandOutcome {
        let Some(line) = parse_slash_line(typed) else {
            return CommandOutcome::Nothing;
        };
        if line.name.is_empty() {
            return CommandOutcome::Nothing;
        }
        if let Some(command) = self.local.find(&line.name) {
            if !command.takes_argument && !line.arg.is_empty() {
                return CommandOutcome::Failed {
                    message: format!("/{} does not take an argument.", line.name),
                };
            }
            return CommandOutcome::Nothing;
        }
        if session_id.is_empty() {
            return CommandOutcome::Failed {
                message: no_session_notice(&line.name),
            };
        }
        let held = self.state.catalog.clone().or_else(|| self.wording.clone());
        let mut runner = self.runner(held);
        let outcome = runner.dispatch(session_id, typed).await;
        if self.state.catalog.is_none() {
            self.wording = runner.catalog;
        }
        if let CommandOutcome::Submit { text, .. } = &outcome {
            let params = submit_params(session_id, text, &fresh_submission_id());
            if let Err(error) = self
                .client
                .call_raw("prompt.submit", params, CallOptions::default())
                .await
            {
                let message = if error.outcome_unknown() {
                    crate::failure::prompt_unknown_notice()
                } else {
                    crate::failure::prompt_refused_notice(&reason_of(&error))
                };
                return CommandOutcome::Failed { message };
            }
        }
        outcome
    }

    /// The draft changed. At most one completion leaves, 60 ms after the last change.
    ///
    /// The draft goes through the same palette rules the screen uses, so a draft the palette
    /// would not complete (prose, a local command, an `@` word) cancels the pending request.
    pub fn draft_changed(&mut self, text: String, caret: usize) {
        let ctx = PaletteContext {
            catalog: self.state.catalog.as_ref(),
            catalog_status: &self.state.status,
            local: &self.local,
            refused: &self.state.refused,
        };
        let effects = palette_update(
            &mut self.drafts,
            PaletteInput::DraftChanged { text, caret },
            &ctx,
        );
        let wanted = effects.into_iter().find_map(|effect| match effect {
            PaletteEffect::RequestCompletion {
                request_id,
                text,
                after,
            } => Some((request_id, text, after)),
            _ => None,
        });
        match wanted {
            Some((request_id, text, after)) => {
                let session_id = self.state.bound_session.clone();
                self.request_completion(request_id, text, after, session_id);
            }
            None => self.cancel_completion(),
        }
    }
}

impl Drop for CommandService {
    fn drop(&mut self) {
        self.cancel_completion();
        if let Some(delayed) = self.delayed.take() {
            delayed.abort();
        }
    }
}

/// Turn an outcome into session events for the application loop.
///
/// Output and refusals become notices under the typed line; a directive becomes a prompt
/// whose user cell shows `display` while the model receives `message`.
#[must_use]
pub fn outcome_actions(outcome: CommandOutcome, typed: &str) -> Vec<AppAction> {
    let notice = |kind, text: String| AppAction::Event(Event::Notice { kind, text });
    match outcome {
        CommandOutcome::Output { line, text } => {
            let line = if line.is_empty() {
                typed.trim().to_owned()
            } else {
                line
            };
            vec![notice(NoticeKind::System, format!("{line}\n{text}"))]
        }
        CommandOutcome::Submit {
            text,
            display,
            notice: shown,
        } => {
            let mut actions: Vec<AppAction> = shown
                .into_iter()
                .map(|shown| notice(NoticeKind::System, shown))
                .collect();
            actions.push(AppAction::Event(Event::SubmitCommand {
                text,
                display,
                submission_id: fresh_submission_id(),
            }));
            actions
        }
        CommandOutcome::Prefill {
            text,
            notice: shown,
        } => {
            let mut actions: Vec<AppAction> = shown
                .into_iter()
                .map(|shown| notice(NoticeKind::System, shown))
                .collect();
            actions.push(AppAction::LoadDraft(text));
            actions
        }
        CommandOutcome::Refused { name, message, dim } => {
            let mut actions = vec![notice(NoticeKind::Error, message)];
            if dim {
                actions.push(AppAction::Refused { name });
            }
            actions
        }
        CommandOutcome::Failed { message } | CommandOutcome::OutcomeUnknown { message } => {
            vec![notice(NoticeKind::Error, message)]
        }
        // A runner follows aliases itself, so a re-run never reaches the loop.
        CommandOutcome::Rerun { .. } | CommandOutcome::Nothing => Vec::new(),
    }
}
