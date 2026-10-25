//! The standing automation state against the running gateway. Issue 236.
//!
//! Ignored unless a person asks for it. A skip is not a pass: `TALARIA_LIVE_REQUIRE=1` turns a
//! skip into a failure. The printed reason never includes a home path.
//!
//! The assertions are on values, never on exact times or revisions: the goal judge may finish
//! a goal before the next step runs, and a loop may fire at once.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hermes_client::liveness::LinkEvent;
use hermes_client::{DiscoverOptions, EnsurePolicy, Inbound, default_home, discover_with};
use serde_json::Value;
use talaria_app::app::connect;
use talaria_app::automation::AutomationService;
use talaria_app::commands::CommandService;
use talaria_app::launch::{Launch, NewSession, SESSION_SOURCE, SessionRequest, StartMode};
use talaria_app::sessions::SessionService;
use talaria_core::automation::{
    AutomationEffect, AutomationEvent, AutomationState, ModeKey, update,
};

/// How long one step waits for the gateway's answer.
const STEP_WAIT: Duration = Duration::from_secs(5);

fn hermes_home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

/// The reason without a home path, a user name or a machine path.
fn public_reason(reason: &str) -> String {
    let user = std::env::var("USER").unwrap_or_default();
    reason
        .split_whitespace()
        .map(|word| {
            let hidden = word.contains("/Users/")
                || word.contains("/home/")
                || word.contains("/root/")
                || word.contains("/private/")
                || word.contains("/var/")
                || (!user.is_empty() && word.contains(&user));
            if hidden { "<path>" } else { word }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn gateway_code_sha(home: &Path) -> Result<Option<String>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(async {
        let options = DiscoverOptions {
            control_timeout: Duration::from_secs(5),
            ensure: EnsurePolicy::Never,
        };
        discover_with(home, &options)
            .await
            .map(|endpoint| endpoint.code_sha().map(str::to_owned))
            .map_err(|error| public_reason(&error.to_string()))
    })
}

/// Whether the run may go on, and the reason when it may not.
fn begin() -> Result<(), String> {
    match gateway_code_sha(&hermes_home()) {
        Err(reason) => Err(reason),
        Ok(sha) => hermes_contract::pinned::live_skip_reason(sha.as_deref())
            .map(|reason| public_reason(&reason))
            .map_or(Ok(()), Err),
    }
}

fn launch(folder: &Path) -> Launch {
    Launch {
        mode: StartMode::New,
        model: Some("minimax-m3.1-flash".to_string()),
        provider: Some("ollama-cloud".to_string()),
        effort: None,
        profile: "fixture".to_string(),
        folder: folder.to_path_buf(),
        record: None,
        bench_replay: None,
        no_ensure: true,
    }
}

/// The running session, the services and the state the rows read.
struct Live {
    connection: talaria_app::connection::OpenConnection,
    session_id: String,
    commands: CommandService,
    automation: AutomationService,
    state: AutomationState,
}

/// Every `session.control.update` payload the gateway sent, in order.
async fn drain_updates(
    connection: &mut talaria_app::connection::OpenConnection,
    wait: Duration,
) -> Vec<Value> {
    let deadline = Instant::now() + wait;
    let mut updates = Vec::new();
    while Instant::now() < deadline {
        let next =
            tokio::time::timeout(Duration::from_millis(250), connection.inbound.recv()).await;
        let Ok(Some(event)) = next else { continue };
        if let LinkEvent::Inbound { item, .. } = event
            && let Inbound::Event(inbound) = item
            && inbound.frame.type_name().unwrap_or("") == "session.control.update"
        {
            updates.push(inbound.raw["payload"]["control"].clone());
        }
    }
    updates
}

/// Fold every update into the state.
fn apply_updates(state: &mut AutomationState, session_id: &str, updates: &[Value]) {
    for raw in updates {
        update(
            state,
            AutomationEvent::ControlUpdated {
                session_id: session_id.to_owned(),
                raw: raw.clone(),
            },
        );
    }
}

/// Run one slash command and wait for the gateway's answer.
async fn run_command(live: &mut Live, typed: &str) -> talaria_core::commands::CommandOutcome {
    live.commands.execute(&live.session_id, typed).await
}

/// Run one slash command, waiting while the agent is busy with a turn.
///
/// The gateway refuses a new goal or loop while a turn runs, so the step waits for the turn
/// to end instead of failing.
async fn run_command_when_idle(
    live: &mut Live,
    typed: &str,
) -> talaria_core::commands::CommandOutcome {
    for _ in 0..60 {
        let outcome = run_command(live, typed).await;
        if !said(&outcome).contains("Agent is running") {
            return outcome;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    run_command(live, typed).await
}

/// Everything the command said: the reply, or the message and the notice of a queued turn.
fn said(outcome: &talaria_core::commands::CommandOutcome) -> String {
    match outcome {
        talaria_core::commands::CommandOutcome::Output { text, .. } => text.clone(),
        talaria_core::commands::CommandOutcome::Submit { text, notice, .. } => match notice {
            Some(notice) => format!("{notice} {text}"),
            None => text.clone(),
        },
        other => format!("{other:?}"),
    }
}

/// Read one mode back through the automation service.
async fn read_mode(live: &mut Live, key: ModeKey) -> Option<String> {
    let event = live
        .automation
        .execute(AutomationEffect::ReadMode {
            session_id: live.session_id.clone(),
            key,
            token: 1,
        })
        .await
        .expect("a mode read answers");
    match event {
        AutomationEvent::ModeRead { result, .. } => result.ok().flatten(),
        other => panic!("a mode read answers a ModeRead, got {other:?}"),
    }
}

/// Read the standing state back through the automation service.
async fn read_control(live: &mut Live) -> AutomationEvent {
    live.automation
        .execute(AutomationEffect::ReadControl {
            session_id: live.session_id.clone(),
            token: 1,
        })
        .await
        .expect("a control read answers")
}

/// The goal of an update, when it has one.
fn goal_of(update: &Value) -> Option<&Value> {
    update["goal"].as_object().map(|_| &update["goal"])
}

async fn run_live() {
    let temp = tempfile::tempdir().expect("a scratch folder");
    let folder = temp.path().to_path_buf();
    let connection = connect(&launch(&folder), &hermes_home())
        .await
        .unwrap_or_else(|error| panic!("{}", public_reason(&error.render())));
    let service = SessionService::with_client("live-automation", connection.link.client());
    let opened = service
        .open(SessionRequest::New(NewSession {
            request_id: talaria_app::launch::new_request_id(),
            source: SESSION_SOURCE.to_string(),
            cwd: folder,
            model: Some("minimax-m3.1-flash".to_string()),
            provider: Some("ollama-cloud".to_string()),
            reasoning: None,
        }))
        .await
        .unwrap_or_else(|error| panic!("{}", public_reason(&format!("{error:?}"))));
    let session_id = opened.session_id.clone();
    let client = service.client().expect("a client").clone();
    let mut live = Live {
        connection,
        session_id: session_id.clone(),
        commands: CommandService::new(client.clone()),
        automation: AutomationService::new(client),
        state: AutomationState::default(),
    };

    // Step 0: the three attach reads. YOLO must be off before the run touches anything.
    let effects = update(
        &mut live.state,
        AutomationEvent::Attached {
            session_id: session_id.clone(),
        },
    );
    let mut reads = 0;
    for effect in effects {
        if let AutomationEffect::ReadControl { .. } | AutomationEffect::ReadMode { .. } = effect {
            reads += 1;
        }
    }
    assert_eq!(reads, 3, "one control read and two mode reads");
    let yolo = read_mode(&mut live, ModeKey::Yolo).await;
    if yolo.as_deref() != Some("0") {
        let reason = format!("yolo is already on ({yolo:?}); the run turns it off again");
        println!("LIVE_AUTOMATION_RESULT: SKIPPED ({reason})");
        return;
    }
    let fast = read_mode(&mut live, ModeKey::Fast).await;
    assert_eq!(fast.as_deref(), Some("normal"), "the route is normal");

    // A loop without a prompt changes nothing. The agent is idle here, so the command runs.
    let missing = said(&run_command(&mut live, "/loop 45m").await);
    assert!(
        missing.contains("missing prompt"),
        "the missing prompt is answered, got {missing:?}"
    );

    // Step 2: a goal. The judge may finish it at once, so the update that shows it active is
    // the one that matters.
    let goal_reply = said(&run_command(&mut live, "/goal reply with the word ok").await);
    assert!(
        !goal_reply.is_empty(),
        "the goal answers, got {goal_reply:?}"
    );
    // The subgoal and the loop go while the goal is still active: the judge may finish it at
    // once, and a goal that is done holds neither a subgoal nor a loop back.
    let subgoal_reply = said(&run_command(&mut live, "/subgoal keep it short").await);
    assert!(
        subgoal_reply.to_lowercase().contains("subgoal"),
        "the subgoal answers, got {subgoal_reply:?}"
    );
    let loop_reply =
        said(&run_command_when_idle(&mut live, "/loop 45m talaria fixture check").await);
    assert!(
        loop_reply.contains("45m"),
        "the loop answers, got {loop_reply:?}"
    );
    let updates = drain_updates(&mut live.connection, STEP_WAIT).await;
    assert!(!updates.is_empty(), "the goal update arrived");
    assert!(
        updates.iter().any(|update| {
            goal_of(update).is_some_and(|goal| {
                goal["status"] == "active"
                    && goal["max_turns"].as_u64() == Some(20)
                    && goal["title"] == "reply with the word ok"
            })
        }),
        "the goal update shows an active goal, got {updates:?}"
    );
    assert!(
        updates.iter().any(|update| {
            goal_of(update).is_some_and(|goal| {
                goal["subgoals"]
                    .as_array()
                    .is_some_and(|subgoals| !subgoals.is_empty())
            })
        }),
        "the subgoal update arrived, got {updates:?}"
    );
    assert!(
        updates.iter().any(|update| {
            update["loop"].is_object()
                && update["loop"]["interval_seconds"].as_f64() == Some(2700.0)
        }),
        "the loop update arrived, got {updates:?}"
    );
    apply_updates(&mut live.state, &session_id, &updates);

    // Step 5: pause, resume and stop the loop.
    let mut last = Value::Null;
    for typed in ["/loop pause", "/loop resume", "/loop stop"] {
        let _ = run_command_when_idle(&mut live, typed).await;
        let updates = drain_updates(&mut live.connection, STEP_WAIT).await;
        if let Some(update) = updates.last() {
            last = update.clone();
        }
        apply_updates(&mut live.state, &session_id, &updates);
    }
    assert!(last["loop"].is_null(), "the loop is gone, got {last:?}");

    // Step 6: pause and clear the goal.
    let mut last = Value::Null;
    for typed in ["/goal pause", "/goal clear"] {
        let _ = run_command_when_idle(&mut live, typed).await;
        let updates = drain_updates(&mut live.connection, STEP_WAIT).await;
        if let Some(update) = updates.last() {
            last = update.clone();
        }
        apply_updates(&mut live.state, &session_id, &updates);
    }
    assert!(last["goal"].is_null(), "the goal is gone, got {last:?}");

    // Step 7: a heartbeat, then its clear.
    let mut last = Value::Null;
    let set =
        said(&run_command_when_idle(&mut live, "/heartbeat every 1h talaria fixture ping").await);
    let updates = drain_updates(&mut live.connection, STEP_WAIT).await;
    assert!(
        updates.iter().any(|update| update["heartbeat"].is_object()),
        "the heartbeat update arrived, got {updates:?} (the command said {set:?})"
    );
    if let Some(update) = updates.last() {
        last = update.clone();
    }
    apply_updates(&mut live.state, &session_id, &updates);
    let clear = said(&run_command_when_idle(&mut live, "/heartbeat clear").await);
    let updates = drain_updates(&mut live.connection, STEP_WAIT).await;
    assert!(
        updates.iter().any(|update| update["heartbeat"].is_null()),
        "the heartbeat is gone, got {updates:?} (the command said {clear:?})"
    );
    if let Some(update) = updates.last() {
        last = update.clone();
    }
    apply_updates(&mut live.state, &session_id, &updates);
    assert!(
        last["heartbeat"].is_null(),
        "the heartbeat is gone, got {last:?}"
    );

    // Step 9: YOLO on, then off again.
    let on = said(&run_command_when_idle(&mut live, "/yolo").await);
    assert!(on.contains("YOLO"), "yolo answers, got {on:?}");
    assert_eq!(
        read_mode(&mut live, ModeKey::Yolo).await.as_deref(),
        Some("1"),
        "yolo reads back as on"
    );
    let off = said(&run_command_when_idle(&mut live, "/yolo").await);
    assert!(off.contains("YOLO"), "yolo answers again, got {off:?}");
    assert_eq!(
        read_mode(&mut live, ModeKey::Yolo).await.as_deref(),
        Some("0"),
        "yolo reads back as off"
    );

    // Step 10: /fast changes nothing on this route.
    let fast_reply = said(&run_command_when_idle(&mut live, "/fast").await);
    assert!(
        fast_reply.contains("Priority Processing"),
        "fast answers, got {fast_reply:?}"
    );
    assert_eq!(
        read_mode(&mut live, ModeKey::Fast).await.as_deref(),
        Some("normal"),
        "fast is still normal"
    );

    // Step 11: a plan queues a turn that is never submitted.
    let plan = said(&run_command_when_idle(&mut live, "/plan write a haiku about rain").await);
    assert!(!plan.is_empty(), "the plan answers");

    // Step 12: the standing state is empty again.
    let control = read_control(&mut live).await;
    match control {
        AutomationEvent::ControlRead { result, .. } => {
            let view = result.expect("the snapshot decodes");
            assert!(
                view.goal.is_none() && view.loop_.is_none() && view.heartbeat.is_none(),
                "nothing stands any more, got {view:?}"
            );
        }
        other => panic!("a control read answers a ControlRead, got {other:?}"),
    }

    live.connection.link.close().await;
    println!("LIVE_AUTOMATION_RESULT: PASSED");
}

#[test]
#[ignore]
fn live_automation_reads_the_standing_state() {
    if let Err(reason) = begin() {
        println!("LIVE_AUTOMATION_RESULT: SKIPPED ({reason})");
        if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
            panic!("live test skipped: {reason}");
        }
        return;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(run_live());
}
