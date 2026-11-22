#![cfg(feature = "mock")]
//! A built-in whose agent the budget stops keeps what it has and returns it, so the run ends as
//! budget exhausted with a partial result rather than failing.
use futures::future::BoxFuture;
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use talaria_workflow::{
    builtins::BUILTINS,
    engine::{
        self, AgentBackend, AgentOutcome, AgentRequest, BudgetSnapshot, EngineConfig, EngineEvent,
        MapResolver, RunOutcome,
    },
    mock::MockBackend,
};
use tokio_util::sync::CancellationToken;

/// Stops the first call whose label starts with `label`, as the runner stops a running agent
/// once the budget is reached; from then on the budget reads as spent, so the engine refuses
/// every new call.
struct StopAt {
    inner: Arc<MockBackend>,
    label: &'static str,
    taken: AtomicBool,
    reached: Arc<AtomicBool>,
    labels: Mutex<Vec<String>>,
}
impl AgentBackend for StopAt {
    fn run_agent(&self, request: AgentRequest) -> BoxFuture<'static, AgentOutcome> {
        self.labels.lock().unwrap().push(request.label.clone());
        if request.label.starts_with(self.label) && !self.taken.swap(true, Ordering::SeqCst) {
            let reached = self.reached.clone();
            return Box::pin(async move {
                reached.store(true, Ordering::SeqCst);
                AgentOutcome::Stopped {
                    reason: "token budget exhausted (spent 1200 of 1000)".into(),
                }
            });
        }
        self.inner.run_agent(request)
    }
    fn budget(&self) -> BudgetSnapshot {
        BudgetSnapshot {
            tokens_total: Some(1000),
            tokens_spent: if self.reached.load(Ordering::SeqCst) {
                1200
            } else {
                0
            },
            money_total: None,
            money_spent: 0.0,
        }
    }
}

struct Stopped {
    result: Value,
    labels: Vec<String>,
    refused: usize,
}

async fn stop_at(name: &str, args: Option<Value>, mock: &str, label: &'static str) -> Stopped {
    let source = BUILTINS.iter().find(|b| b.name == name).unwrap().source;
    let review = BUILTINS
        .iter()
        .find(|b| b.name == "review-branch")
        .unwrap()
        .source;
    let inner = Arc::new(MockBackend::from_json(mock).unwrap());
    let backend = Arc::new(StopAt {
        inner,
        label,
        taken: AtomicBool::new(false),
        reached: Arc::new(AtomicBool::new(false)),
        labels: Mutex::new(Vec::new()),
    });
    let mut cfg = EngineConfig::new("fixture", 0, 0);
    cfg.args = args;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let outcome = engine::run(
        engine::parse_script(source).unwrap(),
        cfg,
        backend.clone(),
        Arc::new(MapResolver::new().with_name("review-branch", review)),
        tx,
        CancellationToken::new(),
    )
    .await;
    assert!(
        backend.reached.load(Ordering::SeqCst),
        "{name}: no call labelled {label} ran"
    );
    let mut refused = 0;
    while let Ok(event) = rx.try_recv() {
        if matches!(event, EngineEvent::AgentRefused { .. }) {
            refused += 1;
        }
    }
    let result = match outcome {
        RunOutcome::Completed { result } => result,
        other => panic!("{name}: a budget stop at {label} must return a result: {other:?}"),
    };
    Stopped {
        result,
        labels: backend.labels.lock().unwrap().clone(),
        refused,
    }
}

fn args(text: &str) -> Option<Value> {
    Some(serde_json::from_str(text).unwrap())
}

#[tokio::test(flavor = "current_thread")]
async fn hivecode_returns_what_it_has_when_the_budget_stops_an_agent() {
    let mock = include_str!("../data/builtins/hivecode/happy.mock.json");
    let task = include_str!("../data/builtins/hivecode/happy.args.json");

    let scout = stop_at("hivecode", args(task), mock, "understand").await;
    assert_eq!(scout.result["status"], "budget-exhausted");
    assert_eq!(scout.result["design"], Value::Null);
    assert_eq!(scout.result["proposals"], json!([]));
    assert_eq!(scout.labels, ["understand"]);
    assert_eq!(scout.refused, 0, "no agent may be asked for after the stop");

    let designer = stop_at("hivecode", args(task), mock, "design 2").await;
    assert_eq!(designer.result["status"], "budget-exhausted");
    assert_eq!(designer.result["proposals"], json!(["Plan", "Third"]));
    assert!(
        !designer
            .labels
            .iter()
            .any(|l| l == "choose" || l == "implement")
    );

    let chooser = stop_at("hivecode", args(task), mock, "choose").await;
    assert_eq!(chooser.result["status"], "budget-exhausted");
    assert_eq!(chooser.result["proposals"].as_array().unwrap().len(), 3);
    assert!(!chooser.labels.iter().any(|l| l == "implement"));

    let implementer = stop_at("hivecode", args(task), mock, "implement").await;
    assert_eq!(implementer.result["status"], "budget-exhausted");
    assert!(implementer.result["design"]["plan"].is_array());
    assert_eq!(implementer.result["implementation"], Value::Null);
    assert!(
        implementer.result["next_steps"]
            .as_str()
            .unwrap()
            .contains("budget"),
        "{}",
        implementer.result
    );
    assert!(!implementer.labels.iter().any(|l| l == "collect"));

    // The review child returns its own partial result, and the implementation survives it.
    let review = stop_at("hivecode", args(task), mock, "review: correctness").await;
    assert_eq!(review.result["status"], "implemented");
    assert_eq!(review.result["review"]["status"], "budget-exhausted");
    assert_eq!(review.result["review_error"], Value::Null);
}

#[tokio::test(flavor = "current_thread")]
async fn research_returns_what_it_has_when_the_budget_stops_an_agent() {
    let mock = include_str!("../data/builtins/research/happy.mock.json");
    let question = include_str!("../data/builtins/research/happy.args.json");
    for label in ["plan", "search: code", "read: 1", "synthesize"] {
        let stopped = stop_at("research", args(question), mock, label).await;
        assert_eq!(stopped.result["status"], "budget-exhausted", "{label}");
        assert_eq!(stopped.result["answer"], Value::Null, "{label}");
        assert_eq!(stopped.labels.last().unwrap(), label);
        assert_eq!(
            stopped.refused, 0,
            "{label}: nothing is asked for after the stop"
        );
    }
    let reader = stop_at("research", args(question), mock, "read: 1").await;
    assert_eq!(reader.result["sources_read"], json!([]));
    // The source it was reading leads the unread list, ahead of the one past maxReads.
    assert_eq!(reader.result["sources_unread"].as_array().unwrap().len(), 2);
    assert_eq!(reader.result["unavailable"], json!([]));
}

#[tokio::test(flavor = "current_thread")]
async fn review_branch_returns_what_it_has_when_the_budget_stops_an_agent() {
    let mock = include_str!("../data/builtins/review-branch/happy.mock.json");
    let collector = stop_at("review-branch", None, mock, "collect").await;
    assert_eq!(collector.result["status"], "budget-exhausted");
    assert_eq!(collector.result["head_commit"], Value::Null);
    assert_eq!(collector.labels, ["collect"]);

    let reviewer = stop_at("review-branch", None, mock, "review: correctness").await;
    assert_eq!(reviewer.result["status"], "budget-exhausted");
    assert_eq!(reviewer.result["reviewers_failed"], json!(["correctness"]));
    assert!(!reviewer.labels.iter().any(|l| l.starts_with("verify")));

    let verifier = stop_at("review-branch", None, mock, "verify: ").await;
    assert_eq!(verifier.result["status"], "budget-exhausted");
    assert_eq!(verifier.result["confirmed"], json!([]));
    assert_eq!(verifier.result["uncertain"], json!([]));
    assert_eq!(
        verifier.result["not_verified"][0]["id"], "correctness-1",
        "{}",
        verifier.result
    );
    assert!(!verifier.labels.iter().any(|l| l == "synthesize"));

    let synthesizer = stop_at("review-branch", None, mock, "synthesize").await;
    assert_eq!(synthesizer.result["status"], "budget-exhausted");
    assert_eq!(synthesizer.result["summary"], Value::Null);
    assert!(!synthesizer.result["ranked"].as_array().unwrap().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn fix_failing_tests_returns_what_it_has_when_the_budget_stops_an_agent() {
    let mock = include_str!("../data/builtins/fix-failing-tests/happy.mock.json");
    let finder = stop_at("fix-failing-tests", None, mock, "find").await;
    assert_eq!(finder.result["status"], "budget-exhausted");
    assert_eq!(finder.result["baseline"], json!([]));
    assert_eq!(finder.labels, ["find"]);

    let fixer = stop_at("fix-failing-tests", None, mock, "fix 1").await;
    assert_eq!(fixer.result["status"], "budget-exhausted");
    assert_eq!(
        fixer.result["attempted"][0]["reason"],
        "stopped by the budget"
    );
    assert!(!fixer.labels.iter().any(|l| l.starts_with("verify")));

    let verifier = stop_at("fix-failing-tests", None, mock, "verify 1").await;
    assert_eq!(verifier.result["status"], "budget-exhausted");
    assert_eq!(verifier.result["attempted"][0]["status"], "unverified");
    assert_eq!(
        verifier.result["attempted"][0]["reason"],
        "stopped by the budget"
    );

    // Wherever the budget stops it, the next steps say the run stopped early.
    for stopped in [&finder, &fixer, &verifier] {
        let next = stopped.result["next_steps"].as_str().unwrap();
        assert!(
            next.starts_with("Nothing was merged or pushed. The run stopped early: "),
            "{next}"
        );
    }
}
