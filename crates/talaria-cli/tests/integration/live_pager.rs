//! A live transcript search. Issue 222. Ignored unless a person asks for it.
//!
//! A skip is not a pass. `TALARIA_LIVE_REQUIRE=1` turns a skip into a failure.
//! The printed reason never includes a home path.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hermes_client::liveness::LinkEvent;
use hermes_client::{DiscoverOptions, EnsurePolicy, Inbound, default_home, discover_with};
use serde_json::json;
use talaria_app::app::{apply_inbound, connect};
use talaria_app::launch::{Launch, NewSession, SESSION_SOURCE, SessionRequest, StartMode};
use talaria_app::sessions::SessionService;
use talaria_core::session::{Cell, SessionState, TurnState};
use talaria_tui::history::PlainCellLines;
use talaria_tui::pager::{Pager, Query, SearchIndex, find_all};

const ASK: &str = "Run the shell command echo PIPIT-$((6*7)) and then reply with the word done in capital letters.";
const ECHO_A: &str = "echo PIPIT-$((6*7))";
const ECHO_B: &str = "echo PIPIT-42";

fn hermes_home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

fn public_reason(reason: &str) -> String {
    let user = std::env::var("USER").unwrap_or_default();
    reason
        .split_whitespace()
        .map(|word| {
            let hidden = word.contains("/Users/")
                || word.contains("/home/")
                || word.contains("/root/")
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

fn skip_reason() -> Option<String> {
    match gateway_code_sha(&hermes_home()) {
        Err(reason) => Some(reason),
        Ok(sha) => hermes_contract::pinned::live_skip_reason(sha.as_deref())
            .map(|reason| public_reason(&reason)),
    }
}

fn begin() -> bool {
    if let Some(reason) = skip_reason() {
        println!("LIVE_PAGER_RESULT: SKIPPED ({reason})");
        if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
            panic!("live test skipped: {reason}");
        }
        return false;
    }
    true
}

fn launch(folder: &Path, record: PathBuf) -> Launch {
    Launch {
        mode: StartMode::New,
        model: Some("minimax-m3.1-flash".to_string()),
        provider: Some("ollama-cloud".to_string()),
        effort: None,
        profile: "fixture".to_string(),
        folder: folder.to_path_buf(),
        record: Some(record),
        bench_replay: None,
        no_ensure: true,
    }
}

fn cell_text(cell: &Cell) -> Option<(&str, String)> {
    match cell {
        Cell::Tool(tool) => Some((
            "tool",
            format!("{} {}", tool.name, tool.result.as_deref().unwrap_or("")),
        )),
        Cell::Assistant(assistant) => Some(("assistant", assistant.text.clone())),
        _ => None,
    }
}

async fn run_live(record: &Path) {
    let temp = tempfile::tempdir().expect("a scratch folder");
    let folder = temp.path().to_path_buf();
    let mut connection = connect(&launch(&folder, record.to_path_buf()), &hermes_home())
        .await
        .unwrap_or_else(|error| panic!("{}", public_reason(&error.render())));
    let service = SessionService::with_client("live-pager", connection.link.client());
    let mut opened = service
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
    let submitted = service
        .execute(talaria_core::session::Effect::SendPrompt {
            attachments: Vec::new(),
            session_id: talaria_core::session::SessionId::new(opened.session_id.clone()),
            text: ASK.to_string(),
            submission_id: format!("live-pager-{}", talaria_app::launch::new_request_id()),
        })
        .await;
    assert!(
        matches!(submitted, talaria_app::sessions::ExecuteOutcome::Sent),
        "the prompt was sent"
    );

    let deadline = Instant::now() + Duration::from_secs(180);
    let mut complete = false;
    while Instant::now() < deadline {
        let next = tokio::time::timeout(Duration::from_secs(2), connection.inbound.recv()).await;
        let Ok(Some(event)) = next else {
            if complete {
                break;
            }
            continue;
        };
        if let LinkEvent::Inbound { item, .. } = event {
            match item {
                Inbound::Event(inbound) => {
                    let kind = inbound.frame.type_name().unwrap_or("").to_string();
                    apply_inbound(
                        &mut opened.state,
                        &opened.session_id,
                        inbound.frame,
                        inbound.raw,
                    );
                    if kind == "message.complete" {
                        complete = true;
                    }
                }
                Inbound::ServerRequest(request) => answer(request).await,
                Inbound::Malformed(_) | Inbound::Closed(_) => {}
            }
        }
        if complete && !matches!(opened.state.turn(), TurnState::Running) {
            break;
        }
    }
    connection.link.close().await;
    assert!(complete, "message.complete did not arrive");

    let state: &SessionState = &opened.state;
    let _ = find_all(
        state.cells(),
        1,
        &mut SearchIndex::new(),
        &Query::new("pipit-42"),
        &PlainCellLines,
    );
    let tool_hit = state.cells().iter().any(|cell| {
        matches!(cell_text(cell), Some(("tool", text)) if text.to_lowercase().contains("pipit-42"))
    });
    let reply_hit = state
        .cells()
        .iter()
        .any(|cell| matches!(cell_text(cell), Some(("assistant", text)) if text.contains("DONE")));
    assert!(tool_hit, "pipit-42 is in a tool cell");
    assert!(reply_hit, "DONE is in an assistant cell");
    let log = std::fs::read_to_string(record).unwrap_or_default();
    let history = log.matches("\"session.history\"").count();
    let resumes = log.matches("\"session.resume\"").count();
    assert_eq!(history, 0, "the transcript sends no session.history");
    assert!(
        resumes <= 1,
        "no second session.resume after the transcript opened"
    );
    let mut index = SearchIndex::new();
    let ctx = talaria_tui::pager::PagerContext {
        cells: state.cells(),
        source_epoch: 1,
        title: "",
        running: false,
        needs_answer: false,
        connection: talaria_tui::pager::ConnectionBanner::Connected,
        history_truncated: false,
    };
    let _pager = Pager::open(&ctx, 80, 24, &mut index);
    println!("LIVE_PAGER_RESULT: PASSED");
}

async fn answer(request: hermes_client::ServerRequest) {
    let decision = match &request.request {
        hermes_contract::events::ServerRequest::Approval(params) => {
            let command = params.command.trim().to_string();
            if command == ECHO_A || command == ECHO_B {
                Some("once".to_string())
            } else {
                None
            }
        }
        _ => Some(String::new()),
    };
    match decision {
        Some(choice) if choice == "once" => {
            request
                .responder
                .respond(&json!({"choice": "once"}))
                .await
                .ok();
        }
        Some(_) => {
            request.responder.respond(&json!(null)).await.ok();
        }
        None => {
            let command = match &request.request {
                hermes_contract::events::ServerRequest::Approval(params) => params.command.clone(),
                _ => String::new(),
            };
            request
                .responder
                .respond(&json!({"choice": "deny"}))
                .await
                .ok();
            panic!("unexpected approval for: {command}");
        }
    }
}

#[test]
#[ignore]
fn live_pager_finds_reply_and_tool_output() {
    if !begin() {
        return;
    }
    let dir = tempfile::tempdir().expect("record dir");
    let record = dir.path().join("frames.jsonl");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(run_live(&record));
}
