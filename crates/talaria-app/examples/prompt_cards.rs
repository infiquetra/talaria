//! Interactive prompt cards against the same gateway mock used by the protocol tests.
//!
//! Events and server requests are logged from the inbound stream. After awaited answers,
//! the demo mirrors the mock's fixed resolved reply; these entries are not raw wire captures.

#[path = "../tests/support/convert.rs"]
mod convert;
#[path = "../tests/support/mock_gateway.rs"]
mod mock_gateway;

use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::event::Event as Input;
use hermes_client::{CallOptions, Client, Inbound};
use hermes_contract::methods::canonical::session_interrupt;
use mock_gateway::{Fixture, FrameKind, MockGateway, SESSION_ID, fixture_events, session_event};
use serde_json::{Value, json};
use talaria_app::prompts::{MaskedKind, MaskedRequestInfo, PromptNotice, PromptService};
use talaria_core::session::{Effect, Event, PromptKind, SessionId, SessionState, update};
use talaria_tui::history::sanitize;
use talaria_tui::keys::Key;
use talaria_tui::live::{LiveView, max_live_height};
use talaria_tui::prompts::{KeyOutcome, MaskedView, PromptIntent, PromptStack};
use talaria_tui::terminal::{
    InlineTerminal, TerminalGuard, probe_startup, spawn_input, terminal_size,
};

type DemoResult<T> = Result<T, Box<dyn std::error::Error>>;

const SCENARIOS: &[&str] = &[
    "approval",
    "approval-note",
    "clarify-multi",
    "sudo",
    "unsupported",
    "interrupt",
    "withdrawn",
];

struct Output {
    file: File,
    terminal: InlineTerminal<io::Stdout>,
    outbound: usize,
}

impl Output {
    fn line(&mut self, line: &str) -> io::Result<()> {
        let line = sanitize(line).into_owned();
        writeln!(self.file, "{line}")?;
        self.file.flush()?;
        self.terminal.commit_lines(vec![line.into()])
    }

    fn notices(&mut self, notices: Vec<PromptNotice>) -> io::Result<()> {
        for notice in notices {
            self.line(&format!("notice: {}", notice.text))?;
        }
        Ok(())
    }

    fn outbound(&mut self, mock: &MockGateway) -> io::Result<()> {
        for seen in mock.seen().into_iter().skip(self.outbound) {
            let frame = if seen.kind == FrameKind::Request {
                json!({"jsonrpc": "2.0", "id": seen.id, "method": seen.method, "params": seen.params})
            } else if seen.is_error {
                json!({"jsonrpc": "2.0", "id": seen.id, "error": seen.params})
            } else {
                let mut result = seen.params;
                if let Some(value) = result.get("value").and_then(Value::as_str) {
                    result = json!({"value": format!("[withheld, {} characters]", value.chars().count())});
                }
                json!({"jsonrpc": "2.0", "id": seen.id, "result": result})
            };
            self.line(&format!("out {frame}"))?;
            if seen.kind == FrameKind::Request
                && matches!(
                    seen.method.as_deref(),
                    Some("approval.respond" | "clarify.respond")
                )
            {
                self.line(&format!(
                    "in {}",
                    json!({"jsonrpc": "2.0", "id": seen.id, "result": {"status": "resolved"}})
                ))?;
            }
            if seen.kind == FrameKind::Response
                && seen.is_error
                && let Some(id) = seen.id.as_str()
                && let Some(written) = mock
                    .written_server_requests()
                    .iter()
                    .find(|item| item.id.as_deref() == Some(id))
            {
                self.line(&format!(
                    "refused {} in {} ms",
                    written.method,
                    seen.at.saturating_duration_since(written.at).as_millis()
                ))?;
            }
            self.outbound += 1;
        }
        Ok(())
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut scenario = None;
    let mut log = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--scenario" => scenario = args.next(),
            "--log" => log = args.next(),
            _ => usage(),
        }
    }
    let (Some(scenario), Some(log)) = (scenario, log) else {
        usage()
    };
    if !SCENARIOS.contains(&scenario.as_str()) {
        eprintln!("unknown scenario: {scenario}");
        std::process::exit(2);
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build();
    let result = runtime
        .map_err(|error| error.into())
        .and_then(|runtime| runtime.block_on(run(scenario, log)));
    if let Err(error) = result {
        eprintln!("prompt cards: {error}");
        std::process::exit(1);
    }
}

fn usage() -> ! {
    eprintln!("usage: prompt_cards --scenario <name> --log <file>");
    std::process::exit(2);
}

async fn run(scenario: String, log: String) -> DemoResult<()> {
    if let Some(parent) = std::path::Path::new(&log).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(log)?;
    let guard = TerminalGuard::acquire().map_err(|error| io::Error::other(format!("{error:?}")))?;
    let probe = probe_startup(Duration::from_millis(100))
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    let size = terminal_size()?;
    let terminal = InlineTerminal::new(
        io::stdout(),
        size,
        max_live_height(size.1),
        probe,
        guard.link(),
    )?;
    let mut output = Output {
        file,
        terminal,
        outbound: 0,
    };
    let (input_tx, mut input_rx) = tokio::sync::mpsc::unbounded_channel();
    let input = spawn_input(move |event| input_tx.send(event).is_ok());
    let (mock, client, mut inbound) = MockGateway::start().await;
    mock.set_auto_settle(true);
    mock.publish_cancel_after_response(Some("resolved"));
    let script_mock = mock.clone();
    let script = tokio::spawn(async move { script(&script_mock, &scenario).await });
    let mut state = SessionState::empty(SessionId::new(SESSION_ID));
    let mut next = 0;
    let mut service = PromptService::new(Box::new(move || {
        next += 1;
        format!("sub-{next}")
    }));
    let mut stack = PromptStack::new();
    let mut shown: Option<(String, String)> = None;
    let mut finished_at: Option<Instant> = None;
    let mut dirty = true;
    let mut ticks = tokio::time::interval(Duration::from_millis(10));
    let result = async {
        loop {
            tokio::select! {
                item = inbound.next() => {
                    match item.ok_or_else(|| io::Error::other("gateway stream closed"))? {
                        Inbound::Event(event) => {
                            output.line(&format!("in {}", json!({"jsonrpc":"2.0", "method":"event", "params":event.raw})))?;
                            if event.raw["type"] == "request.cancel" {
                                let payload = &event.raw["payload"];
                                output.notices(service.on_request_cancel(
                                    payload["id"].as_str().unwrap_or_default(),
                                    payload["method"].as_str().unwrap_or_default(),
                                    payload["reason"].as_str().unwrap_or_default(),
                                ))?;
                            } else {
                                update(&mut state, session_event(&event));
                            }
                        }
                        Inbound::ServerRequest(request) => {
                            let method = mock.written_server_requests().into_iter().find(|frame| frame.id.as_deref() == Some(&request.id)).map(|frame| frame.method).unwrap_or_default();
                            let tagged = serde_json::to_value(&request.request)?;
                            let params = tagged.as_object().and_then(|map| map.values().next()).cloned().unwrap_or(Value::Null);
                            output.line(&format!("in {}", json!({"jsonrpc":"2.0", "id":request.id, "method":method, "params":params})))?;
                            output.notices(service.on_server_request(request).await)?;
                        }
                        Inbound::Closed(_) => return Err(io::Error::other("gateway connection closed").into()),
                        Inbound::Malformed(_) => return Err(io::Error::other("gateway frame did not decode").into()),
                    }
                    dirty = true;
                }
                event = input_rx.recv() => {
                    let outcome = match event {
                        Some(Input::Key(key)) => stack.handle_key(Key::from(key), Instant::now()),
                        Some(Input::Paste(text)) => stack.handle_paste(&text, Instant::now()),
                        Some(Input::Resize(cols, rows)) => { output.terminal.resize(cols, rows)?; dirty = true; continue; }
                        Some(_) => continue,
                        None => return Err(io::Error::other("terminal input closed").into()),
                    };
                    if let KeyOutcome::Consumed { intent, interrupt_requested, notice } = outcome {
                        if let Some(notice) = notice { output.line(&format!("notice: {notice}"))?; }
                        let mut effects = Vec::new();
                        if let Some(intent) = intent {
                            if let PromptIntent::Masked { request_id, value } = intent {
                                if let Err(error) = service.answer_masked(&request_id, value.into_inner()).await {
                                    output.line(&format!("notice: could not answer masked request: {error}"))?;
                                }
                            } else {
                                effects = convert::apply_intent(&mut service, &mut state, intent);
                            }
                        }
                        if interrupt_requested { effects.extend(update(&mut state, Event::Interrupt)); }
                        execute(&client, &mut service, &mut state, &mut stack, &mut output, &mock, effects).await?;
                        if interrupt_requested { stack.stop_cleared(); }
                        dirty = true;
                    }
                }
                _ = ticks.tick() => {}
            }
            output.outbound(&mock)?;
            if dirty {
                let prompts: Vec<_> = state.open_prompts().collect();
                let infos = service.masked_requests();
                let masked: Vec<_> = infos.iter().map(masked_view).collect();
                let keys: Vec<_> = prompts.iter().map(|prompt| prompt.key.clone()).collect();
                output.notices(service.on_prompts_changed(&keys))?;
                stack.sync(Some(SESSION_ID), &prompts, &masked, true, Instant::now());
                let next = infos.first().map(|info| (info.id.clone(), masked_title(info))).or_else(|| prompts.first().map(|prompt| (
                    format!("{}:{}", prompt.key.prompt_id, prompt.key.execution_generation),
                    if prompt.kind == PromptKind::Approval { "Approval needed" } else { "Question" }.to_owned(),
                )));
                if next != shown {
                    if let Some((_, header)) = shown.take() { output.line(&format!("card closed: {header}"))?; }
                    if let Some((_, header)) = &next { output.line(&format!("card shown: {header}"))?; }
                    shown = next;
                }
                let section = stack.section();
                output.terminal.draw_live(&LiveView { sections: section.iter().map(|section| section as &dyn talaria_tui::live::LiveSection).collect() })?;
                dirty = false;
            }
            if script.is_finished() && shown.is_none() {
                if finished_at.get_or_insert_with(Instant::now).elapsed() >= Duration::from_millis(100) {
                    break;
                }
            } else {
                finished_at = None;
            }
        }
        script.await.map_err(io::Error::other)?;
        output.outbound(&mock)?;
        output.line("scenario done")?;
        Ok(())
    }.await;
    output.terminal.restore()?;
    if let Some(input) = input {
        input.stop();
    }
    drop(guard);
    result
}

async fn execute(
    client: &Client,
    service: &mut PromptService,
    state: &mut SessionState,
    stack: &mut PromptStack,
    output: &mut Output,
    mock: &MockGateway,
    effects: Vec<Effect>,
) -> DemoResult<()> {
    let mut effects: VecDeque<_> = effects.into();
    while let Some(effect) = effects.pop_front() {
        match effect {
            Effect::SendPrompt {
                session_id,
                text,
                submission_id,
            } => {
                client.call_raw("prompt.submit", json!({"session_id":session_id.as_str(), "text":text, "submission_id":submission_id}), CallOptions::default()).await?;
            }
            Effect::Interrupt {
                session_id,
                execution_generation,
            } => {
                output.notices(
                    service
                        .deny_before_interrupt(
                            client,
                            state,
                            session_id.as_str(),
                            Duration::from_secs(2),
                        )
                        .await,
                )?;
                output.outbound(mock)?;
                let params = serde_json::from_value(
                    json!({"session_id":session_id.as_str(), "execution_generation":execution_generation}),
                )?;
                let result = client.call::<session_interrupt::Marker>(params).await;
                stack.stop_cleared();
                result?;
            }
            effect => {
                let executed = service.execute(client, effect).await;
                output.outbound(mock)?;
                output.notices(executed.notices)?;
                for event in executed.events {
                    effects.extend(update(state, event));
                }
            }
        }
    }
    Ok(())
}

fn masked_view(info: &MaskedRequestInfo) -> MaskedView {
    let method = match info.kind {
        MaskedKind::Sudo => "sudo",
        MaskedKind::Secret => "secret",
        MaskedKind::VaultCode => "vault.code",
        MaskedKind::VaultUnlockPrompt => "vault.unlock_prompt",
    };
    MaskedView {
        request_id: info.id.clone(),
        session_id: info.session_id.clone(),
        method: method.to_owned(),
        fields: info
            .fields
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect(),
    }
}

fn masked_title(info: &MaskedRequestInfo) -> String {
    let field = |name| {
        info.fields
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
            .unwrap_or_default()
    };
    match info.kind {
        MaskedKind::Sudo => "sudo password".to_owned(),
        MaskedKind::Secret => format!("Secret for {}", field("env_var")),
        MaskedKind::VaultCode => {
            if field("site").is_empty() {
                "One-time code".to_owned()
            } else {
                format!("One-time code for {}", field("site"))
            }
        }
        MaskedKind::VaultUnlockPrompt => format!("Unlock {}", field("display_name")),
    }
}

async fn recorded_approval(mock: &MockGateway) {
    // Keep the recorded payloads while letting the mock own its monotonic sequence counter.
    for frame in fixture_events(Fixture::Approval) {
        let params = &frame["params"];
        let kind = params["type"].as_str().unwrap_or_default();
        mock.send_event(kind, params["payload"].clone()).await;
        if kind == "approval.request" {
            break;
        }
    }
}

async fn script(mock: &MockGateway, scenario: &str) {
    match scenario {
        "approval" | "interrupt" => {
            recorded_approval(mock).await;
            mock.wait_for_request(
                if scenario == "interrupt" {
                    "session.interrupt"
                } else {
                    "approval.respond"
                },
                0,
            )
            .await;
        }
        "approval-note" => {
            mock.approval_request(
                "approval-note",
                1,
                "git push --force origin main",
                &["once", "deny", "session", "always"],
            )
            .await;
            mock.wait_for_request("prompt.submit", 0).await;
        }
        "clarify-multi" => {
            mock.clarify_request(
                "environments",
                1,
                "Which environments?",
                &["staging", "prod", "qa"],
                true,
            )
            .await;
            mock.wait_for_request("clarify.respond", 0).await;
            mock.clarify_request("notes", 1, "Any notes?", &[], false)
                .await;
            mock.wait_for_request("clarify.respond", 1).await;
        }
        "sudo" => {
            let id = mock
                .server_request_with("sudo", json!({"command":"sudo apt-get install jq"}))
                .await;
            mock.wait_for_answer(&id, 0).await;
        }
        "unsupported" => {
            for kind in ["theme.pick", "theme.pick", "terminal.read"] {
                let id = mock.server_request(kind).await;
                mock.wait_for_answer(&id, 0).await;
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
        "withdrawn" => {
            mock.approval_request("withdrawn", 1, "rm -rf .pytest_cache", &["once", "deny"])
                .await;
            tokio::time::sleep(Duration::from_secs(2)).await;
            mock.approval_settled("withdrawn", 1).await;
            let id = mock
                .server_request_with("sudo", json!({"command":"sudo apt-get install jq"}))
                .await;
            tokio::time::sleep(Duration::from_secs(2)).await;
            mock.request_cancel(&id, "sudo", "timeout").await;
        }
        _ => unreachable!("scenario checked before terminal setup"),
    }
}
