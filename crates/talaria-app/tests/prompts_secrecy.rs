//! A masked value typed into a card reaches Hermes in one place only: the response frame.
//!
//! The test types (or pastes) a marker into a `sudo` card, hands the resulting intent straight
//! to the prompt service, and then searches everything else the public interfaces produced: the
//! bytes and rows the card drew, the `Debug` text of every type on the path, every notice, and
//! the recorder's form of the session. None of them may hold the marker. The response frame the
//! gateway read must hold it exactly.
//!
//! What this file cannot do: the workspace has no tracing crate and the dependency list for this
//! work is closed, so no subscriber can be installed to capture log events, and the output of the
//! test process cannot be read back from inside it. Absence from log lines is therefore covered
//! only through the text the public interfaces return (all of it is searched below). Absence
//! from process memory is checked by reading the value type, not here.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hermes_client::{REDACTED, Redactor, scan_literals};
use mock_gateway::{
    MockGateway, SESSION_ID, next_event, next_server_request, server_request_params,
};
use serde_json::json;
use talaria_app::prompts::{Executed, MaskedKind, PromptNotice, PromptService};
use talaria_tui::keys::{Key, KeyCode, KeyKind, Mods};
use talaria_tui::live::{LiveSection, LiveView};
use talaria_tui::prompts::{KeyOutcome, MaskedView, PromptIntent, PromptStack};
use talaria_tui::terminal::{InlineTerminal, ProbeReply, RestoreLink, TerminalCaps};

const COLS: u16 = 80;
const ROWS: u16 = 24;
const COMMAND: &str = "sudo apt-get install jq";

/// A value that cannot be mistaken for anything the program prints on its own. Built at run time
/// so the literal is not in the test binary's source text.
fn fresh_marker() -> String {
    ["TALARIA", "MASK", "7f3a"].join("-")
}

/// Bytes written to the terminal under test, shared with the reader.
#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl Sink {
    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|poison| poison.into_inner()))
    }
}

impl io::Write for Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// What one draw of the card put on the terminal.
struct Drawn {
    rows: Vec<String>,
    bytes: Vec<u8>,
}

/// Draw the card alone in an empty live region and read the screen back through a terminal
/// emulator.
fn draw(stack: &PromptStack) -> Drawn {
    let section = stack
        .section()
        .unwrap_or_else(|| panic!("no card is drawn"));
    let height = section.height(COLS);
    let sink = Sink::default();
    let probe = ProbeReply {
        caps: TerminalCaps {
            synchronized_output: false,
            keyboard_enhancement: false,
        },
        cursor: Some((0, 0)),
    };
    let mut terminal = InlineTerminal::new(
        sink.clone(),
        (COLS, ROWS),
        height,
        probe,
        RestoreLink::detached(),
    )
    .unwrap_or_else(|_| panic!("the inline terminal could not start"));
    let view = LiveView {
        sections: vec![&section as &dyn LiveSection],
    };
    terminal
        .draw_live(&view)
        .unwrap_or_else(|_| panic!("the live region could not be drawn"));
    let bytes = sink.take();
    let mut parser = vt100::Parser::new(ROWS, COLS, 0);
    parser.process(&bytes);
    let rows = parser
        .screen()
        .rows(0, COLS)
        .map(|row| row.trim_end().to_owned())
        .collect();
    Drawn { rows, bytes }
}

/// Everything the public interfaces returned as text, each with a label that says where it came
/// from. A failure names the label, never the text.
#[derive(Default)]
struct Observed {
    texts: Vec<(String, String)>,
}

impl Observed {
    fn add(&mut self, label: &str, text: impl Into<String>) {
        self.texts.push((label.to_owned(), text.into()));
    }

    fn add_notices(&mut self, label: &str, notices: &[PromptNotice]) {
        for notice in notices {
            self.add(&format!("{label} (text)"), notice.text.clone());
            self.add(&format!("{label} (debug)"), format!("{notice:?}"));
        }
    }

    fn add_drawn(&mut self, label: &str, drawn: &Drawn) {
        self.add(&format!("{label} (rows)"), drawn.rows.join("\n"));
        self.add(
            &format!("{label} (terminal bytes)"),
            String::from_utf8_lossy(&drawn.bytes).into_owned(),
        );
    }

    fn assert_free_of(&self, marker: &str) {
        assert!(!self.texts.is_empty(), "nothing was observed");
        for (label, text) in &self.texts {
            assert!(
                !text.contains(marker),
                "the masked value appeared in: {label}"
            );
        }
    }
}

fn key(code: KeyCode) -> Key {
    Key {
        code,
        mods: Mods::default(),
        kind: KeyKind::Press,
    }
}

/// A typed character, with shift set for capitals as a terminal reports them.
fn char_key(c: char) -> Key {
    Key {
        code: KeyCode::Char(c),
        mods: Mods {
            shift: c.is_uppercase(),
            ..Mods::default()
        },
        kind: KeyKind::Press,
    }
}

/// A key or paste that only edits the field: no intent, no interrupt. Any notice is observed.
fn expect_editing(outcome: KeyOutcome, observed: &mut Observed) {
    match outcome {
        KeyOutcome::Consumed {
            intent,
            interrupt_requested,
            notice,
        } => {
            assert!(intent.is_none(), "editing the field produced an intent");
            assert!(!interrupt_requested, "editing the field asked to interrupt");
            if let Some(text) = notice {
                observed.add("a card notice while editing", text);
            }
        }
        KeyOutcome::NotHandled => panic!("the focused masked card did not take the key"),
    }
}

#[derive(Clone, Copy)]
enum Entry {
    Typed,
    Pasted,
}

async fn exercise(entry: Entry) {
    let marker = fresh_marker();
    let marker_len = marker.chars().count();
    let mut observed = Observed::default();

    let (mock, client, mut inbound) = MockGateway::start().await;
    // Hermes sends request.cancel with reason "resolved" to the answering viewer too.
    mock.publish_cancel_after_response(Some("resolved"));
    let mut counter = 0u32;
    let mut service = PromptService::new(Box::new(move || {
        counter += 1;
        format!("sub-{counter}")
    }));

    // The request arrives and the service turns it into an open masked request.
    let id = mock
        .server_request_with("sudo", json!({"command": COMMAND}))
        .await;
    let request = next_server_request(&mut inbound).await;
    observed.add("the inbound server request (debug)", format!("{request:?}"));
    let mut notices = service.on_server_request(request).await;
    let infos = service.masked_requests();
    assert_eq!(infos.len(), 1, "one masked request should be open");
    assert_eq!(infos[0].id, id);
    assert_eq!(infos[0].kind, MaskedKind::Sudo);
    observed.add("the masked request info (debug)", format!("{infos:?}"));
    let view = MaskedView {
        request_id: infos[0].id.clone(),
        session_id: infos[0].session_id.clone(),
        method: "sudo".to_owned(),
        fields: infos[0]
            .fields
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect(),
    };

    // The card is drawn before anything is typed.
    let start = Instant::now();
    let mut stack = PromptStack::new();
    stack.sync(
        Some(SESSION_ID),
        &[],
        std::slice::from_ref(&view),
        true,
        start,
    );
    let empty = draw(&stack);
    assert!(
        empty.rows.iter().any(|row| row.contains("sudo password")),
        "the card title should be on the screen before typing"
    );

    // The marker goes in, past the focus guard.
    let later = start + Duration::from_millis(300);
    match entry {
        Entry::Typed => {
            for c in marker.chars() {
                expect_editing(stack.handle_key(char_key(c), later), &mut observed);
            }
        }
        Entry::Pasted => expect_editing(stack.handle_paste(&marker, later), &mut observed),
    }

    // While the value sits in the buffer the screen shows bullets and nothing else of it.
    let typed = draw(&stack);
    let field = typed
        .rows
        .iter()
        .find(|row| row.starts_with("value:"))
        .unwrap_or_else(|| panic!("the card has no value line"));
    assert_eq!(
        field.matches('\u{2022}').count(),
        marker_len,
        "the value line should hold one bullet per typed character"
    );
    observed.add_drawn("the card with the value typed", &typed);

    // Enter hands over the intent.
    let outcome = stack.handle_key(key(KeyCode::Enter), later);
    let KeyOutcome::Consumed {
        intent: Some(intent),
        interrupt_requested,
        notice,
    } = outcome
    else {
        panic!("Enter on a masked card should produce an intent");
    };
    assert!(!interrupt_requested, "an answer is not an interrupt");
    if let Some(text) = notice {
        observed.add("a card notice on Enter", text);
    }
    observed.add("the masked intent (debug)", format!("{intent:?}"));
    let PromptIntent::Masked { request_id, value } = intent else {
        panic!("the intent should be a masked answer");
    };
    assert_eq!(request_id, id);
    assert_eq!(value.len_chars(), marker_len);
    if stack.section().is_some() {
        observed.add_drawn("the card after Enter", &draw(&stack));
    }

    // The intent goes straight to the service, which writes the response frame.
    let result = service.answer_masked(&request_id, value.into_inner()).await;
    assert!(result.is_ok(), "the masked answer was not written");

    // The card leaves once the open request is gone.
    stack.sync(Some(SESSION_ID), &[], &[], true, later);
    assert!(
        stack.section().is_none(),
        "the card should leave after the answer"
    );

    // The response frame is the one place the marker appears.
    let answer = mock.wait_for_answer(&id, 0).await;
    assert!(!answer.is_error, "the answer should carry a result");
    assert_eq!(answer.params, json!({"value": marker}));
    let answer_count = mock.answers().len();
    assert_eq!(answer_count, 1, "exactly one response frame was expected");

    // Hermes' own cancel with reason resolved comes back to the answering viewer.
    let event = next_event(&mut inbound).await;
    observed.add("the inbound cancel event (debug)", format!("{event:?}"));
    assert_eq!(event.raw["type"], "request.cancel");
    let payload = &event.raw["payload"];
    notices.extend(service.on_request_cancel(
        payload["id"].as_str().unwrap_or_default(),
        payload["method"].as_str().unwrap_or_default(),
        payload["reason"].as_str().unwrap_or_default(),
    ));
    notices.extend(service.on_prompts_changed(&[]));
    notices.extend(service.on_connection_replaced(1));
    observed.add(
        "the masked requests after the answer (debug)",
        format!("{:?}", service.masked_requests()),
    );
    observed.add_notices("a prompt notice", &notices);
    let executed = Executed {
        events: Vec::new(),
        notices: notices.clone(),
    };
    observed.add("the executed result (debug)", format!("{executed:?}"));
    observed.add(
        "the gateway's record of what it wrote (debug)",
        format!("{:?}", mock.written()),
    );

    // The recorder's form of the session: the answer is withheld whole.
    let mut redactor = Redactor::new(Vec::new())
        .unwrap_or_else(|_| panic!("a redactor without substitutions could not be built"));
    let mut params = server_request_params("sudo", SESSION_ID);
    params["command"] = json!(COMMAND);
    let request_text =
        json!({"jsonrpc": "2.0", "id": id, "method": "sudo", "params": params}).to_string();
    let _ = redactor.redact_inbound(&request_text);
    let response_text =
        json!({"jsonrpc": "2.0", "id": answer.id, "result": answer.params}).to_string();
    // A control: the scan does find the marker where it is known to be.
    assert!(
        !scan_literals(&response_text, std::slice::from_ref(&marker)).is_empty(),
        "the scan should find the marker in the unredacted response frame"
    );
    let redacted = redactor.redact_outbound(&response_text);
    assert!(
        !redacted.redactions.is_empty(),
        "the recorder should report that it withheld the answer"
    );
    let recorded = redacted
        .frame
        .unwrap_or_else(|| panic!("the response frame could not be read by the recorder"))
        .to_string();
    assert!(
        recorded.contains(&format!("\"result\":\"{REDACTED}\"")),
        "the recorded answer should be the redaction marker"
    );
    observed.add("the recorded session", recorded);

    observed.assert_free_of(&marker);

    client.close().await;
}

#[tokio::test]
async fn masked_value_only_in_response_frame() {
    exercise(Entry::Typed).await;
}

#[tokio::test]
async fn pasted_masked_value_only_in_response_frame() {
    exercise(Entry::Pasted).await;
}
