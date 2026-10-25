//! Headless runs against the in-process mock gateway, in real time.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::time::{Duration, Instant};

use mock_gateway::{HeadlessScenario, StandaloneGateway};
use talaria_app::headless::run::StopCause;
use talaria_app::headless::service::{HeadlessTimings, run_headless};
use talaria_app::headless::{OutputFormat, PrintOptions};
use talaria_app::launch::{Launch, StartMode};

fn launch() -> Launch {
    Launch {
        mode: StartMode::New,
        model: None,
        provider: None,
        effort: None,
        profile: "fixture".to_string(),
        folder: std::path::PathBuf::from("/work/proj"),
        record: None,
        bench_replay: None,
        no_ensure: true,
    }
}

fn options() -> PrintOptions {
    PrintOptions {
        prompt_arg: Some("hello".to_string()),
        format: OutputFormat::Text,
        timeout: None,
    }
}

async fn run_once(
    scenario: HeadlessScenario,
    mode: StartMode,
) -> talaria_app::headless::output::HeadlessResult {
    tokio::task::spawn_blocking(move || {
        let home = tempfile::tempdir().expect("home");
        let gateway = StandaloneGateway::start(home.path());
        gateway.configure_headless(scenario);
        let mut launch = launch();
        launch.mode = mode;
        let timings = HeadlessTimings::scaled_for_tests();
        gateway.block_on(async {
            let (tx, stops) = tokio::sync::mpsc::channel(8);
            let mock = (*gateway).clone();
            let driver = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(2), async {
                    while mock.requests("prompt.submit").is_empty() {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    match scenario {
                        HeadlessScenario::StartedStop => {
                            let _ = tx.send(StopCause::Signal(15)).await;
                        }
                        HeadlessScenario::QueuedStop => {
                            let _ = tx.send(StopCause::Timeout).await;
                        }
                        HeadlessScenario::ServerRequest => {
                            let _foreign = mock.server_request("sudo").await;
                            tokio::time::sleep(Duration::from_millis(20)).await;
                            mock.send_event("message.start", serde_json::json!({}))
                                .await;
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            let id = mock.server_request("sudo").await;
                            while mock.answers_for(&id).is_empty() {
                                tokio::time::sleep(Duration::from_millis(1)).await;
                            }
                            let frame = mock_gateway::headless_events().pop().expect("completion");
                            mock.send_event("message.complete", frame["params"]["payload"].clone())
                                .await;
                        }
                        _ => {}
                    }
                })
                .await
                .expect("the scenario reached its trigger");
            });
            let done = tokio::time::timeout(
                Duration::from_secs(5),
                run_headless(
                    &launch,
                    &options(),
                    "hello".to_string(),
                    gateway.home(),
                    Instant::now(),
                    stops,
                    timings,
                ),
            )
            .await
            .expect("the headless run finished");
            driver.await.expect("the scenario driver finished");
            done
        })
    })
    .await
    .expect("the mock gateway thread finished")
}

#[tokio::test]
async fn new_session_one_turn_completed() {
    // A new headless session runs one turn, detaches without ending the
    // session, and exits zero with the answer.
    let done = run_once(HeadlessScenario::Complete, StartMode::New).await;
    assert_eq!(done.exit_code, 0);
    assert_eq!(done.text, "5555");
}

#[tokio::test]
async fn continue_and_resume_reuse_p1_12_frames() {
    // Continue and resume send the same frames as an interactive open and
    // never create a session.
    for mode in [
        StartMode::Continue,
        StartMode::Resume(Some("local-session-1".to_string())),
    ] {
        let done = run_once(HeadlessScenario::Complete, mode).await;
        assert_eq!(done.exit_code, 0);
    }
}

#[tokio::test]
async fn completion_before_submit_reply() {
    // The owner may publish before the submit reply lands; the run still
    // settles from its own completion.
    let done = run_once(HeadlessScenario::BeforeReply, StartMode::New).await;
    assert_eq!(done.exit_code, 0);
}

#[tokio::test]
async fn receipt_failure_still_reports_the_answer() {
    // A failed receipt read keeps the live answer, nulls the usage figures,
    // warns once, and still exits zero.
    let done = run_once(HeadlessScenario::ReceiptFailure, StartMode::New).await;
    assert_eq!(done.exit_code, 0);
    assert_eq!(done.text, "5555");
}

#[tokio::test]
async fn sigterm_interrupts_own_turn_only() {
    // A stop on a started turn interrupts with our own generation, never
    // cancels, and exits with the signal code.
    let done = run_once(HeadlessScenario::StartedStop, StartMode::New).await;
    assert_eq!(done.exit_code, 143);
}

#[tokio::test]
async fn timeout_while_queued_cancels_without_interrupt() {
    // A stop on a queued prompt cancels it, waits for the settlement first,
    // never interrupts, and exits 124.
    let done = run_once(HeadlessScenario::QueuedStop, StartMode::New).await;
    assert_eq!(done.exit_code, 124);
}

#[tokio::test]
async fn server_request_of_own_turn_refused_at_once() {
    // A server request on our running turn is refused within a second;
    // another viewer's request stays unanswered.
    let done = run_once(HeadlessScenario::ServerRequest, StartMode::New).await;
    assert_eq!(done.exit_code, 0);
}

#[test]
fn mock_completion_matches_recorded_shape() {
    // Guard: the committed recording and the recorded Hermes frames agree on
    // the completion shape, so the mock builds what Hermes sends.
    use std::collections::BTreeSet;

    let text = std::fs::read_to_string("tests/data/headless/finite_turn.frames.jsonl")
        .expect("the committed recording");
    assert!(!text.contains("/Users/"));
    assert!(!text.contains("/home/"));
    let frames: Vec<serde_json::Value> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("one JSON frame per line"))
        .collect();
    let ours = frames
        .iter()
        .find(|frame| frame["params"]["type"] == "message.complete")
        .expect("a completion frame");
    let keys: BTreeSet<&str> = ours["params"]["payload"]
        .as_object()
        .expect("a payload")
        .keys()
        .map(String::as_str)
        .collect();
    for key in ["text", "content", "admission_id", "outcome", "status"] {
        assert!(keys.contains(key), "the recording lacks {key}");
    }
    let recorded =
        mock_gateway::fixture_event(mock_gateway::Fixture::PlainAnswer, "message.complete");
    let recorded_keys: BTreeSet<&str> = recorded["params"]["payload"]
        .as_object()
        .expect("a payload")
        .keys()
        .map(String::as_str)
        .collect();
    for key in ["text", "content", "admission_id", "outcome", "status"] {
        assert!(
            recorded_keys.contains(key),
            "the recorded frames lack {key}"
        );
    }
}
