# Talaria patterns guide

Read this after your issue and before you write code. `cargo test --workspace` compiles and runs
every Rust block here. Each example defines local stand-ins named as the
[roadmap](../handoff/2026-10-22-next-steps/07-roadmap-and-work-units.md) names them (`Event`,
`Effect`, `State`, `update`); no crate imports them, and every other name is illustrative. Owners
are named by crate and file. Example bodies are test code, so `unwrap` is fine in them; the parts
that stand for library code (a reducer, a render function, an executor) use `?` or `match`.

## Where code goes

| Crate | Owns | Must not | Guard |
|---|---|---|---|
| `hermes-contract` | Types generated from the pinned Hermes contract. | Hold hand-written wire structs. | review |
| `hermes-client` | Connection, JSON-RPC calls, reconnect, recorder and replay source. | Know about the screen. | review; layering test |
| `talaria-core` | `Event`, `Effect`, `SessionState` and `update` in `src/session/`. | Reach `hermes-client` or a terminal library. | layering tests |
| `talaria-app` | The message channel and loop (`src/app.rs`), the effect executor `SessionService::execute` (`src/sessions.rs`), configuration (`src/config.rs`). | Reach a terminal library; await network calls in the loop. | layering test; review |
| `talaria-tui` | The terminal module `src/terminal.rs`, `history.rs`, `live.rs`, `composer/`, `keys.rs`, render functions, view state with overlays. | Hold protocol or session state. | `boundaries.rs` scans; review |
| `talaria-workflow` | Running workflows. | Reach `talaria-tui` or a terminal library. | layering tests |
| `talaria-cli` | The `talaria` binary and argument parsing. | Hold application logic. | review |

## Rules

| # | Rule | Guard |
|---|---|---|
| 1 | No `unsafe`. The workspace sets `unsafe_code = "forbid"`. | `crates/talaria-core/tests/layering.rs` |
| 2 | No `Rc<RefCell<…>>` webs and no shared mutable application state. State is owned data, passed by `&mut` into pure functions; tasks and threads talk only through channels. | `tools/patterns-check/tests/boundaries.rs` |
| 3 | Every source of change (network, keyboard, timers, workflow runner) sends a message into the one application message channel. | review |
| 4 | `hermes-contract`, `hermes-client`, `talaria-core`, `talaria-app` and `talaria-workflow` never depend on ratatui or crossterm, directly or through another crate. | `layering.rs::domain_crates_never_reach_a_terminal_library` |
| 5 | Only the terminal module of `talaria-tui` writes escape sequences or changes terminal modes. That covers raw mode, the alternate screen, scroll regions and bracketed paste. | `guide_rules.rs` (sentence); `boundaries.rs` (tokens) |
| 6 | Reducers and render functions read no clock, environment, file or socket. Time arrives inside events. | review |
| 7 | The event loop never awaits network input or output. Effects run in spawned tasks and report back as messages. | review |
| 8 | The loop draws at most once per batch of drained messages, and only when something visible changed. Idle means zero frames. | review |
| 9 | In crates, Hermes types come from `hermes-contract`; never hand-write a struct the contract provides. Test frames are copied from recordings. This guide's stand-ins are the one exception. | review |
| 10 | Untrusted text is rendered literally: no markup interpretation; control and bidirectional-override characters become visible stand-ins. | review |
| 11 | Never set the `unattended` flag on `prompt.submit`: Hermes treats an unattended turn as YOLO mode and runs dangerous commands without asking. | `boundaries.rs` (`"unattended"` scan) |
| 12 | Library code returns typed errors; `unwrap` and `expect` appear only in tests (example bodies included), or with a comment proving they cannot fail. | review |
| 13 | No planning codes in source comments. | review |

The boundary rules apply to `crates/`; tool crates under `tools/` never link into the binary and
are not scanned. The one cursor query the terminal module needs goes under
`#[allow(clippy::disallowed_methods)]` on that item with a comment; the scan rejects it elsewhere.

## Pinned versions

| crate | version |
|---|---|
| ratatui | 0.30.2 |
| crossterm | 0.29.0 |
| tokio | 1.54.1 |

The root `Cargo.toml` pins them. Use these as `{ workspace = true }`: `ratatui`, `crossterm`,
`tokio`, `tokio-tungstenite`, `futures-util`, `serde`, `serde_json`. Reach crossterm through
`ratatui::crossterm` or the workspace entry, never a version of your own: `Cargo.lock` holds one.

## Interfaces not to use

A banned call fails `cargo clippy` (bans in `clippy.toml`) with a note naming this guide. Evidence:
the stack bake-off, [section 3](../handoff/2026-10-22-next-steps/research/u16-stack-bakeoff.md#3-fixes-grouped-by-cause).

| Interface | Why | Instead | Guard |
|---|---|---|---|
| `Viewport::Inline` (any use) | A draw after a size change asks the terminal for the cursor position, and crossterm cannot read the answer while the key reader runs (fixes 3 and 4). | The terminal module uses a fixed viewport it moves itself. | `boundaries.rs` |
| `Terminal::insert_before` | It scrolls with `CSI S`, which may not reach native scrollback (fix 5). | The terminal module commits history by newline scrolling inside a scroll region. | clippy; `boundaries.rs` |
| `Terminal::get_cursor_position`, `crossterm::cursor::position` | The same cursor-query race. | One query in `terminal.rs` before the key reader starts. | clippy (both paths) |
| `ratatui::init`, `try_init`, `init_with_options`, `try_init_with_options`, `run`, `restore`, `try_restore`, `DefaultTerminal` | They take the alternate screen and own raw mode, which breaks the inline design; `init` also installs its own panic hook. | The terminal module's setup and restore routine, and its panic hook that restores the terminal. | clippy |
| crossterm `EventStream` when standard input is not a terminal | It panicked in the bake-off's plain mode (fix 2). | The key reader in the terminal module checks for a terminal first. | `boundaries.rs` (only in `terminal.rs`) |
| `print!`, `println!`, `eprintln!` while the screen is live | Stray bytes corrupt the live region. | Send a notice message; the terminal module draws it. | review |

## Pattern 1: The application event enum and the single channel

Every source of change sends into one unbounded channel: a source never blocks or drops input,
and the loop drains a burst before it draws once. A blocking source (the key reader) runs on a
`std::thread` with the non-async `send`; a timer is a tokio task. The key reader turns crossterm's
key event into a plain key value first, so no terminal type crosses the channel. The real loop is
in `crates/talaria-app/src/app.rs`, the key type in `crates/talaria-tui/src/keys.rs`; test the
loop the same way, by sending messages and counting draws.

```rust
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

#[derive(Debug)]
enum Key { Char(char) }
#[derive(Debug)]
enum Event { Key(Key), Tick { now_ms: u64 }, Quit }
#[derive(Default)]
struct State { typed: String, now_ms: u64, quit: bool, seen: Vec<String> }
fn to_key(event: KeyEvent) -> Option<Key> {
    match event.code { KeyCode::Char(c) => Some(Key::Char(c)), _ => None }
}
// Applies one message; returns true when something on screen changed.
fn apply(state: &mut State, event: Event) -> bool {
    state.seen.push(format!("{event:?}"));
    match event {
        Event::Key(Key::Char(c)) => { state.typed.push(c); true }
        Event::Tick { now_ms } => { state.now_ms = now_ms; false }
        Event::Quit => { state.quit = true; false }
    }
}
// Returns (batches, draws).
async fn run(mut rx: UnboundedReceiver<Event>, state: &mut State) -> (u32, u32) {
    let (mut batches, mut draws) = (0, 0);
    while let Some(first) = rx.recv().await {
        let mut visible = apply(state, first);
        while let Ok(next) = rx.try_recv() { visible |= apply(state, next); }
        batches += 1;
        draws += u32::from(visible); // the real loop draws through the terminal module here
        if state.quit { break; }
    }
    (batches, draws)
}

let (tx, rx) = unbounded_channel();
let keys = tx.clone();
std::thread::spawn(move || {
    for code in [KeyCode::Char('h'), KeyCode::F(1), KeyCode::Char('i')] {
        let Some(key) = to_key(KeyEvent::from(code)) else { continue };
        if keys.send(Event::Key(key)).is_err() { break; }
    }
}).join().unwrap();
let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
let mut state = State::default();
let (batches, draws) = runtime.block_on(async {
    let timer = tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        let _ = timer.send(Event::Tick { now_ms: 5 });
        let _ = timer.send(Event::Quit);
    });
    run(rx, &mut state).await
});
assert_eq!(state.seen, ["Key(Char('h'))", "Key(Char('i'))", "Tick { now_ms: 5 }", "Quit"]);
assert_eq!((batches, draws), (2, 1)); // four messages, one draw: the tick batch drew nothing
```

## Pattern 2: The pure reducer

`update` does no input or output and reads no clock or randomness, so a recorded session replays
exactly. Identifiers that must survive a retry are minted from state: a prefix given when the state
is created plus a counter (the application uses `talaria-<16 hex launch id>-<n>`), never the
process id or the clock. The real reducer is `update(&mut SessionState, Event) -> Vec<Effect>` in
`crates/talaria-core/src/session/`. In `message.delta`, `text` is a string or null: a non-empty
string appends to the live tail, null seals the reply segment and makes no cell, and an empty
string changes nothing. Hermes sends null only to a local viewer, so the state needs no flag for it.
Test the real reducer this way, and by replaying whole recorded sessions through it.

```rust
#[derive(Debug, PartialEq)]
enum Event { Submit { text: String }, MessageDelta { text: Option<String> } }
#[derive(Debug, PartialEq)]
enum Effect { SendPrompt { submission_id: String, text: String } }
#[derive(Debug, Default)]
struct State { id_prefix: String, next_id: u64, live: String, sealed: Vec<String>, revision: u64 }
impl State {
    fn new(id_prefix: &str) -> Self { State { id_prefix: id_prefix.to_string(), ..State::default() } }
}
fn update(state: &mut State, event: Event) -> Vec<Effect> {
    match event {
        Event::Submit { text } => {
            state.next_id += 1;
            vec![Effect::SendPrompt { submission_id: format!("{}-{}", state.id_prefix, state.next_id), text }]
        }
        Event::MessageDelta { text: Some(chunk) } => {
            if !chunk.is_empty() { state.live.push_str(&chunk); state.revision += 1; }
            Vec::new()
        }
        Event::MessageDelta { text: None } => {
            if !state.live.is_empty() { state.sealed.push(std::mem::take(&mut state.live)); state.revision += 1; }
            Vec::new()
        }
    }
}

let mut state = State::new("talaria-00000000000000ab");
let sent = update(&mut state, Event::Submit { text: "hello".into() });
assert_eq!(sent, [Effect::SendPrompt { submission_id: "talaria-00000000000000ab-1".into(), text: "hello".into() }]);
assert!(update(&mut state, Event::MessageDelta { text: Some("Hi".into()) }).is_empty());
assert_eq!((state.live.as_str(), state.revision), ("Hi", 1));
assert!(update(&mut state, Event::MessageDelta { text: Some(String::new()) }).is_empty());
assert_eq!((state.live.as_str(), state.revision), ("Hi", 1)); // the empty string changed nothing
assert!(update(&mut state, Event::MessageDelta { text: None }).is_empty());
assert_eq!((state.live.as_str(), state.sealed.as_slice(), state.revision), ("", ["Hi".to_string()].as_slice(), 2));
```

## Pattern 3: The effect executor

Effects are data. The executor runs each call in `tokio::spawn` and sends the outcome back as a
message carrying the correlation key, the `submission_id`; it never touches state, and the loop
never waits. The real executor is `SessionService::execute` in `crates/talaria-app/src/sessions.rs`,
calling `Client::call::<M>` in `crates/hermes-client`, which owns timeouts (120 s by default, 300 s
for `approval.respond`). A dropped connection is an unknown outcome (`CallError::Unknown`), never
success. `prompt.submit` carries exactly `session_id`, `text` and `submission_id`: Hermes refuses it
without a `submission_id`, and Talaria never sends `input_id` or the unattended flag. Test the real
executor with such a fake, replying with recorded frames or contract shapes, never invented ones.

```rust
use std::sync::{Arc, Mutex};
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

enum Effect { SendPrompt { session_id: String, submission_id: String, text: String } }
#[derive(Debug, PartialEq)]
enum Event {
    PromptAdmitted { submission_id: String, status: Value },
    PromptFailed { submission_id: String, reason: String },
}
// Stand-in client. It records each call and returns the `result` of the frame value of
// reference-slices/rust/tests/fixtures/live_two_turns.jsonl, line 6.
#[derive(Clone, Default)]
struct FakeClient { calls: Arc<Mutex<Vec<(String, Value)>>> }
impl FakeClient {
    async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.calls.lock().map_err(|error| error.to_string())?.push((method.to_string(), params));
        let reply = r#"{"jsonrpc": "2.0", "id": 103, "result": {"admission_id": "id-0004", "ref": {"profile_id": "default", "session_id": "local-session-1"}, "sequence": 68, "status": "queued", "outcome": null, "authority_epoch": 1, "execution_generation": null}}"#;
        let reply: Value = serde_json::from_str(reply).map_err(|error| error.to_string())?;
        Ok(reply["result"].clone())
    }
}
fn execute(client: &FakeClient, effect: Effect, tx: &UnboundedSender<Event>) {
    let (client, tx) = (client.clone(), tx.clone());
    match effect {
        Effect::SendPrompt { session_id, submission_id, text } => {
            tokio::spawn(async move {
                let params = json!({ "session_id": session_id, "text": text, "submission_id": submission_id });
                let event = match client.call("prompt.submit", params).await {
                    Ok(receipt) => Event::PromptAdmitted { submission_id, status: receipt["status"].clone() },
                    Err(reason) => Event::PromptFailed { submission_id, reason },
                };
                let _ = tx.send(event); // a failed send means the loop has stopped
            });
        }
    }
}

let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
let (client, (tx, mut rx)) = (FakeClient::default(), unbounded_channel());
let id = "talaria-00000000000000ab-1";
let effect = Effect::SendPrompt { session_id: "local-session-1".into(), submission_id: id.into(), text: "hello".into() };
let event = runtime.block_on(async { execute(&client, effect, &tx); rx.recv().await });
let calls = client.calls.lock().unwrap();
assert_eq!(calls[0].0, "prompt.submit");
assert_eq!(calls[0].1, json!({ "session_id": "local-session-1", "text": "hello", "submission_id": id }));
assert_eq!(event, Some(Event::PromptAdmitted { submission_id: id.into(), status: json!("queued") }));
```

## Pattern 4: A render function and its snapshot test

A render function reads state by shared reference and draws into a `Frame` (or an area and a
`Buffer`); it writes no escape sequences, because the terminal module flushes frames. Measure text
with `Line::width()` or `Span::width()`, never a byte or char count. Render functions live in
`crates/talaria-tui/src/`. A snapshot test draws into `Terminal::new(TestBackend::new(width,
height))` and compares rows of cell symbols with a literal list; styles are checked at named
cells. Do not use `TestBackend::assert_buffer_lines` with plain strings: it compares styles too, so
it fails for any styled render. Each issue sets its own widths. Byte-level tests of the terminal
module use emulators `talaria-tui` adds as its own dev-dependencies; render functions never do.

```rust
use ratatui::{Frame, Terminal, backend::TestBackend};
use ratatui::{style::{Color, Style}, text::{Line, Span}};

struct State { model: String, busy: bool }
fn render_status(state: &State, frame: &mut Frame) {
    let area = frame.area();
    let left = Span::raw(state.model.as_str());
    let right = if state.busy { Span::styled("busy", Style::new().fg(Color::Yellow)) } else { Span::raw("") };
    let gap = usize::from(area.width).saturating_sub(left.width() + right.width());
    frame.render_widget(Line::from(vec![left, Span::raw(" ".repeat(gap)), right]), area);
}
fn rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| (0..buffer.area.width).map(|x| buffer[(x, y)].symbol()).collect::<String>().trim_end().to_string())
        .collect()
}

let state = State { model: "sonnet".into(), busy: true };
for width in [40, 100] {
    let mut terminal = Terminal::new(TestBackend::new(width, 1)).unwrap();
    terminal.draw(|frame| render_status(&state, frame)).unwrap();
    let gap = usize::from(width) - 10;
    assert_eq!(rows(&terminal), [format!("sonnet{}busy", " ".repeat(gap))]);
    assert_eq!(terminal.backend().buffer()[(width - 4, 0)].fg, Color::Yellow);
}
assert_eq!(Span::raw("会話").width(), 4); // two chars, four columns
```

## Pattern 5: A transport round-trip test from a recorded frame

Copy the unescaped `frame` value of one recorded line into the test as a raw string and name the
recording and line; never copy the `{"frame": ..., "t_ms": ...}` wrapper or type a frame from
memory. Decode, assert, re-encode, and compare as `serde_json::Value`, never as strings: recordings
use Python's separators. Keep unmodelled fields with `#[serde(flatten)]`, skip absent options when
encoding, and never put `deny_unknown_fields` on a wire type: Hermes adds fields often. Real tests
read `fixtures/frames/` through the replay source in `crates/hermes-client` and decode into
`hermes-contract`'s generated types; there, an event's `params` requires `type`, `session_id`,
`payload`, `replay_epoch` and `seq`. Test each real decoder this way on every event kind it handles.

```rust
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

// Stand-ins for teaching. Real code decodes into hermes-contract's generated types.
#[derive(Debug, Deserialize, Serialize)]
struct Envelope { jsonrpc: String, method: String, params: EventParams }
#[derive(Debug, Deserialize, Serialize)]
struct EventParams {
    #[serde(rename = "type")] kind: String,
    #[serde(skip_serializing_if = "Option::is_none")] session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] seq: Option<u64>,
    #[serde(default)] payload: Value,
    #[serde(flatten)] rest: Map<String, Value>,
}

// The frame value of reference-slices/rust/tests/fixtures/live_two_turns.jsonl, line 20.
let recorded = r#"{"jsonrpc": "2.0", "method": "event", "params": {"authority_epoch": 1, "execution_generation": 1, "admission_id": "id-0004", "type": "message.delta", "session_id": "local-session-1", "payload": {"text": null}, "replay_epoch": "id-0002", "seq": 12}}"#;
let frame: Envelope = serde_json::from_str(recorded).unwrap();
assert_eq!((frame.params.kind.as_str(), frame.params.seq), ("message.delta", Some(12)));
assert_eq!(frame.params.payload["text"], Value::Null);
assert!(frame.params.rest.contains_key("replay_epoch"));
let again = serde_json::to_value(&frame).unwrap();
assert_eq!(again, serde_json::from_str::<Value>(recorded).unwrap());
```

## Pattern 6: Adding an overlay

An overlay is a variant of an enum in `talaria-tui`'s view state, which holds no protocol or
session state. Intents that only open or close views stay in `talaria-tui`, never in
`talaria-core`'s `Event`. While open, an overlay gets every key first: Esc closes it and never
reaches the composer (where Esc interrupts a turn), other keys are dropped, and no effect is
returned. Its render function draws the whole area; entering and leaving the alternate screen is
the terminal module's job (`open_overlay` and `close_overlay` in `crates/talaria-tui/src/terminal.rs`).

```rust
use ratatui::{buffer::Buffer, layout::Rect, text::Line, widgets::{Block, Paragraph, Widget}};

// Stand-in for the real key type in crates/talaria-tui/src/keys.rs.
enum Key { Esc, Char(char) }
#[derive(Debug, PartialEq)]
enum Effect {}
#[derive(Debug, Clone, Copy, PartialEq)]
enum Overlay { About }
#[derive(Default)]
struct View { overlay: Option<Overlay>, composer: String }
impl View {
    fn open(&mut self, overlay: Overlay) { self.overlay = Some(overlay); }
    fn handle_key(&mut self, key: Key) -> Vec<Effect> {
        match (self.overlay, key) {
            (Some(_), Key::Esc) => self.overlay = None,
            (Some(_), _) => {} // an open overlay drops every other key
            (None, Key::Char(c)) => self.composer.push(c),
            (None, Key::Esc) => {} // the composer would interrupt the turn here
        }
        Vec::new()
    }
}
fn render_overlay(overlay: Overlay, area: Rect, buffer: &mut Buffer) {
    match overlay {
        Overlay::About => Paragraph::new(vec![Line::from("Talaria"), Line::from("Esc closes")])
            .block(Block::bordered().title("About"))
            .render(area, buffer),
    }
}

let mut view = View::default();
view.open(Overlay::About);
assert!(view.handle_key(Key::Char('x')).is_empty());
assert_eq!(view.composer, ""); // the key did not reach the composer
let area = Rect::new(0, 0, 40, 4);
let mut buffer = Buffer::empty(area);
if let Some(overlay) = view.overlay { render_overlay(overlay, area, &mut buffer); }
let rows: Vec<String> = (0..area.height).map(|y| (0..area.width).map(|x| buffer[(x, y)].symbol()).collect()).collect();
assert_eq!(rows, [
    format!("┌About{}┐", "─".repeat(33)),
    format!("│Talaria{}│", " ".repeat(31)),
    format!("│Esc closes{}│", " ".repeat(28)),
    format!("└{}┘", "─".repeat(38)),
]);
assert_eq!(view.handle_key(Key::Esc), []);
assert_eq!(view.overlay, None);
```

To add an overlay: add the variant; add the method that opens it; handle its keys while open; write
its render function; write a snapshot test of it open and a test that Esc closes it with no effect.

## Checklist for a new unit

1. Read your issue, then this guide.
2. Add dependencies as `{ workspace = true }`, never with a version of your own; a new external
   crate goes through the issue that needs it.
3. Write the tests first and watch them fail for the reason the issue names.
4. Run the project check: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and
   `cargo test --workspace`.
5. Record a non-obvious learning in `docs/engineering-journal/LEARNINGS.md`.
