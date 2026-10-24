//! Property test: no server request is ever answered twice, and none is left without an answer
//! unless Hermes withdrew it or the connection it arrived on was replaced.
//!
//! A random sequence of server requests (any of the sixteen kinds), withdrawals, operator
//! answers and connection replacements runs against a real client, the prompt service and the
//! in-process gateway. The sequence always ends by declining every masked request that is still
//! open. Then every request id is checked against the response frames the gateway read.
//!
//! The client does not cancel open server requests when a connection is replaced, and a
//! responder that is dropped while it is still undecided makes the client write an error answer
//! on the new connection. So a masked request orphaned by a replaced connection must be given
//! up without a frame, and that is part of what this test pins.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::time::{Duration, Instant};

use hermes_client::{Inbound, InboundStream, ServerRequest};
use mock_gateway::{KNOWN_SERVER_REQUEST_KINDS, MockGateway};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use talaria_app::prompts::PromptService;
use zeroize::Zeroizing;

/// The four kinds that become masked cards. Written out here so the test does not depend on the
/// code under test to say which kinds those are.
const MASKED_KINDS: [&str; 4] = ["sudo", "secret", "vault.code", "vault.unlock_prompt"];

/// Reasons a withdrawal can carry. The contract types the reason as a plain string.
const CANCEL_REASONS: [&str; 4] = ["timeout", "interrupted", "resolved", "something-new"];

/// How long one inbound item may take to arrive before the case is given up.
const PATIENCE: Duration = Duration::from_secs(3);

/// How long a whole case may run.
const CASE_LIMIT: Duration = Duration::from_secs(30);

/// How long the gateway watches for frames nobody expects once every expected one has arrived.
const QUIET: Duration = Duration::from_millis(40);

#[derive(Clone, Debug)]
enum Op {
    /// The gateway sends a server request of this kind.
    Request(&'static str),
    /// The gateway withdraws a request. `pick` chooses among the open masked requests; with none
    /// open it chooses among every request seen so far.
    Cancel { pick: usize, reason: &'static str },
    /// The operator answers one open masked request, with a value or with nothing (declined).
    Answer { pick: usize, empty: bool },
    /// The operator answers a request that was already answered or refused.
    AnswerAgain { pick: usize },
    /// The operator answers a request that was withdrawn or orphaned.
    AnswerStale { pick: usize },
    /// The connection is replaced.
    Replace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fate {
    /// The service answered it with an error at once.
    Refused,
    /// A masked request that is still waiting for the operator.
    Open,
    /// The operator answered it (or the drain declined it).
    Answered,
    /// Hermes withdrew it.
    Withdrawn,
    /// The connection it arrived on was replaced while it was open.
    Orphaned,
}

struct Tracked {
    id: String,
    kind: &'static str,
    fate: Fate,
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => prop::sample::select(MASKED_KINDS.to_vec()).prop_map(Op::Request),
        3 => prop::sample::select(KNOWN_SERVER_REQUEST_KINDS.to_vec()).prop_map(Op::Request),
        2 => (any::<usize>(), prop::sample::select(CANCEL_REASONS.to_vec()))
            .prop_map(|(pick, reason)| Op::Cancel { pick, reason }),
        3 => (any::<usize>(), any::<bool>())
            .prop_map(|(pick, empty)| Op::Answer { pick, empty }),
        1 => any::<usize>().prop_map(|pick| Op::AnswerAgain { pick }),
        1 => any::<usize>().prop_map(|pick| Op::AnswerStale { pick }),
        1 => Just(Op::Replace),
    ]
}

/// The index of the `pick`th entry (wrapping) whose fate is one of `fates`.
fn choose(tracked: &[Tracked], fates: &[Fate], pick: usize) -> Option<usize> {
    let matching: Vec<usize> = tracked
        .iter()
        .enumerate()
        .filter(|(_, entry)| fates.contains(&entry.fate))
        .map(|(index, _)| index)
        .collect();
    if matching.is_empty() {
        None
    } else {
        Some(matching[pick % matching.len()])
    }
}

/// The next server request the client delivers. Events and connection closings are skipped.
async fn next_request(stream: &mut InboundStream) -> ServerRequest {
    loop {
        match tokio::time::timeout(PATIENCE, stream.next()).await {
            Ok(Some(Inbound::ServerRequest(request))) => return request,
            Ok(Some(_)) => {}
            Ok(None) => panic!("the inbound stream ended while a server request was expected"),
            Err(_) => panic!("no server request arrived in time"),
        }
    }
}

/// Wait for the `request.cancel` event of this id to arrive.
async fn wait_for_cancel(stream: &mut InboundStream, id: &str) {
    loop {
        match tokio::time::timeout(PATIENCE, stream.next()).await {
            Ok(Some(Inbound::Event(event)))
                if event.raw["type"] == "request.cancel"
                    && event.raw["payload"]["id"].as_str() == Some(id) =>
            {
                return;
            }
            Ok(Some(_)) => {}
            Ok(None) => panic!("the inbound stream ended while a withdrawal was expected"),
            Err(_) => panic!("the withdrawal did not arrive in time"),
        }
    }
}

/// The service's open masked requests must be exactly the ones this test thinks are open.
fn check_listing(service: &PromptService, tracked: &[Tracked]) {
    let mut listed: Vec<String> = service
        .masked_requests()
        .into_iter()
        .map(|info| info.id)
        .collect();
    listed.sort();
    let mut open: Vec<String> = tracked
        .iter()
        .filter(|entry| entry.fate == Fate::Open)
        .map(|entry| entry.id.clone())
        .collect();
    open.sort();
    assert_eq!(
        listed, open,
        "the open masked requests differ from the model"
    );
}

/// Wait until `expected` frames were read, then watch for extra ones.
async fn settle(mock: &MockGateway, expected: usize) {
    let deadline = Instant::now() + PATIENCE;
    while mock.frame_count() < expected {
        assert!(
            Instant::now() < deadline,
            "only {} of {expected} expected answer frames arrived",
            mock.frame_count()
        );
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    tokio::time::sleep(QUIET).await;
}

async fn drive(ops: Vec<Op>) {
    let (mock, client, mut inbound) = MockGateway::start().await;
    let mut counter = 0u32;
    let mut service = PromptService::new(Box::new(move || {
        counter += 1;
        format!("sub-{counter}")
    }));
    let mut tracked: Vec<Tracked> = Vec::new();

    for op in ops {
        match op {
            Op::Request(kind) => {
                let id = mock.server_request(kind).await;
                let request = next_request(&mut inbound).await;
                assert_eq!(request.id, id, "the client delivered another request");
                let _notices = service.on_server_request(request).await;
                let fate = if MASKED_KINDS.contains(&kind) {
                    Fate::Open
                } else {
                    Fate::Refused
                };
                tracked.push(Tracked { id, kind, fate });
            }
            Op::Cancel { pick, reason } => {
                let chosen = choose(&tracked, &[Fate::Open], pick).or_else(|| {
                    if tracked.is_empty() {
                        None
                    } else {
                        Some(pick % tracked.len())
                    }
                });
                if let Some(index) = chosen {
                    let id = tracked[index].id.clone();
                    let kind = tracked[index].kind;
                    mock.request_cancel(&id, kind, reason).await;
                    wait_for_cancel(&mut inbound, &id).await;
                    let _notices = service.on_request_cancel(&id, kind, reason);
                    if tracked[index].fate == Fate::Open {
                        tracked[index].fate = Fate::Withdrawn;
                    }
                }
            }
            Op::Answer { pick, empty } => {
                if let Some(index) = choose(&tracked, &[Fate::Open], pick) {
                    let value = if empty {
                        Zeroizing::new(String::new())
                    } else {
                        Zeroizing::new(format!("typed-value-{index}"))
                    };
                    let result = service.answer_masked(&tracked[index].id, value).await;
                    assert!(result.is_ok(), "answering an open masked request failed");
                    tracked[index].fate = Fate::Answered;
                }
            }
            Op::AnswerAgain { pick } => {
                if let Some(index) = choose(&tracked, &[Fate::Answered, Fate::Refused], pick) {
                    let _result = service
                        .answer_masked(&tracked[index].id, Zeroizing::new("again".to_owned()))
                        .await;
                }
            }
            Op::AnswerStale { pick } => {
                if let Some(index) = choose(&tracked, &[Fate::Withdrawn, Fate::Orphaned], pick) {
                    let _result = service
                        .answer_masked(&tracked[index].id, Zeroizing::new("late".to_owned()))
                        .await;
                }
            }
            Op::Replace => {
                let generation = mock.replace_connection(&client).await;
                let _notices = service.on_connection_replaced(generation.0);
                for entry in &mut tracked {
                    if entry.fate == Fate::Open {
                        entry.fate = Fate::Orphaned;
                    }
                }
            }
        }
        check_listing(&service, &tracked);
    }

    // The drain: decline every masked request that is still open.
    for entry in &mut tracked {
        if entry.fate == Fate::Open {
            let result = service
                .answer_masked(&entry.id, Zeroizing::new(String::new()))
                .await;
            assert!(result.is_ok(), "declining an open masked request failed");
            entry.fate = Fate::Answered;
        }
    }
    check_listing(&service, &tracked);

    let expected = tracked
        .iter()
        .filter(|entry| matches!(entry.fate, Fate::Refused | Fate::Answered))
        .count();
    settle(&mock, expected).await;

    // Everything the client wrote in this case is an answer to a server request.
    assert_eq!(
        mock.frame_count(),
        mock.answers().len(),
        "the client wrote a frame that is not an answer"
    );
    mock.assert_no_duplicate_answers();
    assert_eq!(
        mock.answers().len(),
        expected,
        "the number of answer frames differs from the number of answerable requests"
    );
    for entry in &tracked {
        let frames = mock.answers_for(&entry.id);
        match entry.fate {
            Fate::Refused => {
                assert_eq!(
                    frames.len(),
                    1,
                    "a refused {} request must get exactly one answer",
                    entry.kind
                );
                assert!(
                    frames[0].is_error,
                    "a refused {} request must be answered with an error",
                    entry.kind
                );
            }
            Fate::Answered => {
                assert_eq!(
                    frames.len(),
                    1,
                    "an answered {} request must get exactly one answer",
                    entry.kind
                );
                assert!(
                    !frames[0].is_error,
                    "an answered {} request must carry a result",
                    entry.kind
                );
            }
            Fate::Withdrawn | Fate::Orphaned => {
                assert_eq!(
                    frames.len(),
                    0,
                    "a {:?} {} request must get no answer",
                    entry.fate,
                    entry.kind
                );
            }
            Fate::Open => panic!("a request was still open after the drain"),
        }
    }

    client.close().await;
}

#[test]
fn every_server_request_answered_at_most_once() {
    let mut config = Config::with_cases(24);
    config.failure_persistence = None;
    config.max_shrink_iters = 48;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|_| panic!("the test runtime could not be built"));
    let mut runner = TestRunner::new(config);
    let strategy = prop::collection::vec(op_strategy(), 1..14);
    let outcome = runner.run(&strategy, |ops| {
        runtime.block_on(async {
            if tokio::time::timeout(CASE_LIMIT, drive(ops)).await.is_err() {
                panic!("a case ran past its time limit");
            }
        });
        Ok(())
    });
    if let Err(error) = outcome {
        panic!("{error}");
    }
}
