//! Issue 232. Queue and steer. Live queue calls against a running gateway.
//!
//! Ignored unless a person asks for them. A skip is not a pass. With
//! `TALARIA_LIVE_REQUIRE=1` a skip fails the test. Each test opens its own session on the
//! `fixture` profile, starts a turn that runs `sleep`, queues prompts while it runs, and drives
//! the queue through `update_queue` and `QueueService` as the app does. Nothing here sends
//! `unattended` or changes Hermes's configuration. A failure caused by the model (no shell call,
//! a steer ignored) gets one rerun; a second failure goes to the operator.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use hermes_client::liveness::LinkEvent;
use hermes_client::{
    CallOptions, Client, DiscoverOptions, EnsurePolicy, Inbound, default_home, discover_with,
};
use hermes_contract::schemas::AdmissionStatus;
use talaria_app::app::apply_inbound;
use talaria_app::connection::{ConnectionService, OpenConnection};
use talaria_app::launch::{
    Launch, NewSession, SessionRequest, StartMode, new_launch_id, new_request_id,
};
use talaria_app::queue::QueueService;
use talaria_app::sessions::{ExecuteOutcome, SessionService, submission_id};
use talaria_core::queue::{
    Direction, IdSource, QueueAction, QueueIntent, RowStatus, SubmissionId, update_queue,
};
use talaria_core::session::{Cell, Effect, Event, SessionId, SessionState, TurnState};
use tokio::time::{Instant, sleep};

const MODEL: &str = "minimax-m3.1-flash";
const PROVIDER: &str = "ollama-cloud";
const SLEEP: &str =
    "Run the shell command sleep 20, then reply with the word done in capital letters.";
const STEER_TURN: &str = "Run sleep 15 and then sleep 15 as two separate shell commands. Then reply with the code word I gave you followed by the digits 42 with no space, or NONE.";
const PELICAN: &str = "The code word is PELICAN.";
const ALPHA: &str = "Reply with the word alpha in capital letters.";
const BRAVO: &str = "Reply with the word bravo in capital letters.";
const CHARLIE: &str = "Reply with the word charlie in capital letters.";
const ANCHOR: &str = "Reply with the word anchor in capital letters.";
const ONE: &str = "Reply with the word one in capital letters.";
const TWO: &str = "Reply with the word two in capital letters.";
const THREE: &str = "Reply with the word three in capital letters.";

fn hermes_home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

fn gateway_code_sha(home: &Path) -> Result<Option<String>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "runtime".to_string())?;
    runtime.block_on(async {
        let options = DiscoverOptions {
            control_timeout: Duration::from_secs(5),
            ensure: EnsurePolicy::Never,
        };
        discover_with(home, &options)
            .await
            .map(|endpoint| endpoint.code_sha().map(str::to_owned))
            .map_err(|_| "no gateway".to_string())
    })
}

fn skip_reason() -> Option<String> {
    match gateway_code_sha(&hermes_home()) {
        Err(reason) => Some(reason),
        Ok(sha) => hermes_contract::pinned::live_skip_reason(sha.as_deref()),
    }
}

/// Print a skip and say whether the test should stop; `TALARIA_LIVE_REQUIRE=1` makes a skip fail.
fn begin() -> bool {
    let Some(reason) = skip_reason() else {
        return true;
    };
    println!("LIVE_QUEUE_RESULT: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live test skipped: {reason}");
    }
    false
}

/// Run one live scenario on its own runtime, then print the result line.
fn live<F, Fut>(scenario: F)
where
    F: FnOnce(Live) -> Fut,
    Fut: std::future::Future<Output = Live>,
{
    if !begin() {
        return;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|_| panic!("runtime"));
    runtime.block_on(async {
        let session = Live::open().await;
        let session = scenario(session).await;
        session.close().await;
    });
    println!("LIVE_QUEUE_RESULT: PASSED");
}

/// Submission ids from one launch id and counter, as the app makes them.
#[derive(Clone)]
struct Ids {
    launch_id: String,
    counter: Arc<AtomicU64>,
}

impl Ids {
    fn next(&self) -> String {
        submission_id(
            &self.launch_id,
            self.counter.fetch_add(1, Ordering::SeqCst) + 1,
        )
    }
}

impl IdSource for Ids {
    fn submission_id(&mut self) -> SubmissionId {
        SubmissionId(self.next())
    }
}

/// One live session: the connection, the reducer state, and the two services.
struct Live {
    connection: OpenConnection,
    client: Client,
    sessions: SessionService,
    queue: QueueService,
    state: SessionState,
    session_id: String,
    subscription_id: String,
    ids: Ids,
    _folder: tempfile::TempDir,
}

impl Live {
    async fn open() -> Live {
        let folder = tempfile::tempdir().unwrap_or_else(|_| panic!("a work folder"));
        let launch = Launch {
            mode: StartMode::New,
            model: Some(MODEL.to_string()),
            provider: Some(PROVIDER.to_string()),
            effort: None,
            profile: "fixture".to_string(),
            folder: folder.path().to_path_buf(),
            record: None,
            bench_replay: None,
            no_ensure: true,
        };
        let connection = ConnectionService::open(&launch, &hermes_home(), None)
            .await
            .unwrap_or_else(|_| panic!("open the fixture gateway without starting it"));
        let client = connection.link.client();
        let launch_id = new_launch_id();
        let sessions = SessionService::with_client(launch_id.clone(), client.clone());
        let opened = sessions
            .open(SessionRequest::New(NewSession {
                request_id: new_request_id(),
                source: "tui".to_string(),
                cwd: folder.path().to_path_buf(),
                model: Some(MODEL.to_string()),
                provider: Some(PROVIDER.to_string()),
                reasoning: None,
            }))
            .await
            .unwrap_or_else(|_| panic!("create a fixture session"));
        let session_id = opened.session_id.clone();
        Live {
            queue: QueueService::new(client.clone(), SessionId::new(&session_id)),
            client,
            connection,
            sessions,
            state: opened.state,
            subscription_id: opened.subscription_id,
            session_id,
            ids: Ids {
                launch_id,
                counter: Arc::new(AtomicU64::new(0)),
            },
            _folder: folder,
        }
    }

    /// Submit text as the composer would, and run what the reducer asks for.
    async fn submit(&mut self, text: &str) {
        let effects = talaria_core::session::update(
            &mut self.state,
            Event::Submit {
                text: text.to_string(),
                display: text.to_string(),
                attachments: Vec::new(),
                submission_id: self.ids.next(),
            },
        );
        self.run(effects).await;
    }

    /// Apply one queue intent and run its calls.
    async fn intent(&mut self, intent: QueueIntent) {
        let effects = update_queue(&mut self.state, intent, &mut self.ids);
        self.run(effects).await;
    }

    /// Run effects one at a time, feeding every result back, until nothing is left.
    async fn run(&mut self, effects: Vec<Effect>) {
        let mut pending: std::collections::VecDeque<Effect> = effects.into();
        while let Some(effect) = pending.pop_front() {
            let events = match effect {
                Effect::SendPrompt {
                    session_id,
                    text,
                    submission_id,
                    attachments: _,
                } => {
                    // The receipt feeds the queue ledger, as the app's submit path does.
                    let reply = self
                        .client
                        .call_raw(
                            "prompt.submit",
                            serde_json::json!({
                                "session_id": session_id.as_str(),
                                "text": text,
                                "submission_id": submission_id,
                            }),
                            CallOptions {
                                timeout: Some(Duration::from_secs(30)),
                            },
                        )
                        .await
                        .unwrap_or_else(|_| panic!("the prompt was not admitted"));
                    let result: AdmissionStatus = serde_json::from_value(reply)
                        .unwrap_or_else(|_| panic!("the submit answered a receipt"));
                    vec![Event::QueueCallResult {
                        action: QueueAction::Submit,
                        admission_id: None,
                        submission_id: Some(SubmissionId(submission_id)),
                        result,
                    }]
                }
                Effect::Interrupt { .. } => {
                    let outcome = self.sessions.execute(effect).await;
                    assert_eq!(outcome, ExecuteOutcome::Sent, "the interrupt was sent");
                    Vec::new()
                }
                Effect::CancelQueued { .. }
                | Effect::ReplaceQueued { .. }
                | Effect::SteerQueued { .. }
                | Effect::Steer { .. }
                | Effect::ResolveUnknown { .. } => self.queue.run(effect).await,
                // Approval answers are not part of these scenarios.
                _ => Vec::new(),
            };
            for event in events {
                let more = talaria_core::session::update(&mut self.state, event);
                pending.extend(more);
            }
        }
    }

    /// Apply inbound frames until `done` holds, failing after `limit`.
    async fn wait(&mut self, what: &str, limit: Duration, done: impl Fn(&SessionState) -> bool) {
        let deadline = Instant::now() + limit;
        while !done(&self.state) {
            assert!(
                self.state.open_prompts().next().is_none(),
                "an approval card opened while waiting for {what}"
            );
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "timed out waiting for {what}");
            let event = tokio::time::timeout(left, self.connection.inbound.recv()).await;
            let Ok(Some(LinkEvent::Inbound {
                item: Inbound::Event(event),
                ..
            })) = event
            else {
                continue;
            };
            if event.raw.get("session_id").and_then(|id| id.as_str()) != Some(&self.session_id) {
                continue;
            }
            let effects = apply_inbound(&mut self.state, &self.session_id, event.frame, event.raw);
            self.run(effects).await;
        }
    }

    /// Texts of the counted queue rows, oldest first.
    fn queued(&self) -> Vec<String> {
        let running = *self.state.turn() == TurnState::Running;
        self.state
            .queue
            .view(running)
            .rows
            .into_iter()
            .filter(|row| row.status == RowStatus::Queued)
            .map(|row| row.text)
            .collect()
    }

    /// Wait until the counted rows read exactly `texts`.
    async fn wait_queue(&mut self, texts: &[&str]) {
        let wanted: Vec<String> = texts.iter().map(|text| text.to_string()).collect();
        let what = format!("the queue to read {texts:?}");
        self.wait(&what, Duration::from_secs(60), |state| {
            let running = *state.turn() == TurnState::Running;
            let rows: Vec<String> = state
                .queue
                .view(running)
                .rows
                .into_iter()
                .filter(|row| row.status == RowStatus::Queued)
                .map(|row| row.text)
                .collect();
            rows == wanted
        })
        .await;
    }

    /// Start the long turn and wait until it runs, then give the model time to call the shell.
    async fn start_turn(&mut self, text: &str) {
        self.submit(text).await;
        self.wait("the turn to start", Duration::from_secs(60), |state| {
            *state.turn() == TurnState::Running
        })
        .await;
        sleep(Duration::from_secs(3)).await;
    }

    /// Select the counted row holding `text`: Up selects the newest, then Up walks older.
    async fn select(&mut self, text: &str) {
        self.intent(QueueIntent::EnterSelection).await;
        for _ in 0..8 {
            let running = *self.state.turn() == TurnState::Running;
            let view = self.state.queue.view(running);
            let picked = view
                .selected
                .and_then(|index| view.rows.get(index))
                .map(|row| row.text.clone());
            if picked.as_deref() == Some(text) {
                return;
            }
            self.intent(QueueIntent::Select(Direction::Up)).await;
        }
        panic!("no queued row reads {text:?}");
    }

    fn user_texts(&self) -> Vec<String> {
        self.state
            .cells()
            .iter()
            .filter_map(|cell| match cell {
                Cell::User(user) => Some(user.text.clone()),
                _ => None,
            })
            .collect()
    }

    fn assistant_text(&self) -> String {
        self.state
            .cells()
            .iter()
            .filter_map(|cell| match cell {
                Cell::Assistant(reply) => Some(reply.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Remove what is still queued, stop a running turn, and detach; the session stays in Hermes.
    async fn close(mut self) {
        self.intent(QueueIntent::Leave).await;
        for text in self.queued() {
            self.select(&text).await;
            self.intent(QueueIntent::Remove).await;
        }
        if *self.state.turn() == TurnState::Running {
            let effects = talaria_core::session::update(&mut self.state, Event::Interrupt);
            self.run(effects).await;
        }
        let _ = self
            .client
            .call_raw(
                "session.detach",
                serde_json::json!({
                    "session_id": self.session_id,
                    "subscription_id": self.subscription_id,
                }),
                CallOptions {
                    timeout: Some(Duration::from_secs(10)),
                },
            )
            .await;
        self.connection.link.close().await;
    }
}

#[test]
#[ignore = "live gateway"]
fn live_queue_edit_remove_move() {
    live(|mut session| async move {
        session.start_turn(SLEEP).await;
        for text in [ALPHA, BRAVO, CHARLIE] {
            session.submit(text).await;
        }
        session.wait_queue(&[ALPHA, BRAVO, CHARLIE]).await;

        // Edit alpha: the new text keeps alpha's place.
        session.select(ALPHA).await;
        session.intent(QueueIntent::BeginEdit).await;
        session
            .intent(QueueIntent::SaveEdit(ANCHOR.to_string()))
            .await;
        session.wait_queue(&[ANCHOR, BRAVO, CHARLIE]).await;

        // Remove charlie.
        session.select(CHARLIE).await;
        session.intent(QueueIntent::Remove).await;
        session.wait_queue(&[ANCHOR, BRAVO]).await;

        // Move bravo up.
        session.select(BRAVO).await;
        session.intent(QueueIntent::Move(Direction::Up)).await;
        session.wait_queue(&[BRAVO, ANCHOR]).await;
        session.intent(QueueIntent::Leave).await;

        // The prompts start in the new order; alpha and charlie never run.
        session
            .wait(
                "both queued prompts to start",
                Duration::from_secs(240),
                |state| {
                    state
                        .cells()
                        .iter()
                        .any(|cell| matches!(cell, Cell::User(user) if user.text == ANCHOR))
                },
            )
            .await;
        let users = session.user_texts();
        let bravo = users.iter().position(|text| text == BRAVO);
        let anchor = users.iter().position(|text| text == ANCHOR);
        assert!(
            bravo.is_some() && bravo < anchor,
            "bravo runs before anchor: {users:?}"
        );
        assert!(!users.iter().any(|text| text == ALPHA || text == CHARLIE));
        session
    });
}

#[test]
#[ignore = "live gateway"]
fn live_steer_queued_reaches_the_turn() {
    live(|mut session| async move {
        session.start_turn(STEER_TURN).await;
        session.submit(PELICAN).await;
        session.wait_queue(&[PELICAN]).await;
        session.select(PELICAN).await;
        session.intent(QueueIntent::Steer).await;
        session
            .wait(
                "the steered turn to finish",
                Duration::from_secs(180),
                |state| *state.turn() != TurnState::Running,
            )
            .await;
        let reply = session.assistant_text();
        assert!(
            reply.contains("PELICAN42"),
            "the steered code word reaches the reply: {reply:?}"
        );
        assert!(
            !session.user_texts().iter().any(|text| text == PELICAN),
            "a steer is not a user prompt"
        );
        session
    });
}

#[test]
#[ignore = "live gateway"]
fn live_send_now_starts_head_after_interrupt() {
    live(|mut session| async move {
        session.start_turn(SLEEP).await;
        for text in [ONE, TWO, THREE] {
            session.submit(text).await;
        }
        session.wait_queue(&[ONE, TWO, THREE]).await;
        session.select(THREE).await;
        session.intent(QueueIntent::SendNow).await;
        session
            .wait("three to start", Duration::from_secs(120), |state| {
                state
                    .cells()
                    .iter()
                    .any(|cell| matches!(cell, Cell::User(user) if user.text == THREE))
            })
            .await;
        let users = session.user_texts();
        assert_eq!(
            users.get(1).map(String::as_str),
            Some(THREE),
            "the moved prompt starts right after the interrupted turn: {users:?}"
        );
        session
    });
}
