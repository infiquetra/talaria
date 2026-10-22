use anyhow::{anyhow, Result};
use crossterm::event::{Event as TermEvent, KeyCode, KeyEventKind, KeyModifiers};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use slice::net;
use slice::state::{Outgoing, Phase, State};
use slice::tui::{KeyThread, Tui};
use slice::ui;
use std::io::Write;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

#[derive(Default)]
struct Args {
    prompt: Option<String>,
    record: Option<String>,
    replay: Option<String>,
    metrics: Option<String>,
    model: String,
    provider: String,
    cwd: String,
    home: String,
    plain: bool,
    attach_only: bool,
    auto_approve: Option<String>,
    replay_delay_ms: u64,
    width: Option<u16>,
}

fn parse_args() -> Result<Args> {
    let mut a = Args {
        model: "minimax-m3.1-flash".into(),
        provider: "ollama-cloud".into(),
        cwd: std::env::current_dir()?.to_string_lossy().into_owned(),
        home: net::default_home(),
        ..Default::default()
    };
    let mut it = std::env::args().skip(1);
    let mut words = vec![];
    while let Some(x) = it.next() {
        let mut val = |n: &str| it.next().ok_or_else(|| anyhow!("{n} needs a value"));
        match x.as_str() {
            "--record" => a.record = Some(val("--record")?),
            "--replay" => a.replay = Some(val("--replay")?),
            "--metrics" => a.metrics = Some(val("--metrics")?),
            "--model" => a.model = val("--model")?,
            "--provider" => a.provider = val("--provider")?,
            "--cwd" => a.cwd = val("--cwd")?,
            "--home" => a.home = val("--home")?,
            "--auto-approve" => a.auto_approve = Some(val("--auto-approve")?),
            "--replay-delay-ms" => a.replay_delay_ms = val("--replay-delay-ms")?.parse()?,
            "--width" => a.width = Some(val("--width")?.parse()?),
            "--plain" => a.plain = true,
            "--attach-only" => {
                a.plain = true;
                a.attach_only = true;
            }
            "-h" | "--help" => {
                println!("slice [--record F] [--replay F] [--plain] [--metrics F] [--cwd DIR] [--auto-approve CHOICE] [PROMPT...]");
                std::process::exit(0);
            }
            _ => words.push(x),
        }
    }
    if !words.is_empty() {
        a.prompt = Some(words.join(" "));
    }
    Ok(a)
}

fn rpc_text(id: i64, o: &Outgoing) -> String {
    json!({"jsonrpc":"2.0","id":id,"method":o.method,"params":o.params}).to_string()
}

fn frame_from_replay_line(line: &str) -> Option<String> {
    let v: Value = serde_json::from_str(line).ok()?;
    match v.get("frame") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
        None => Some(line.to_owned()),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args()?;
    let t0 = Instant::now();
    let now = move || t0.elapsed().as_millis() as u64;

    let tty = !args.plain;
    let width = args.width.unwrap_or_else(|| crossterm::terminal::size().map(|s| s.0).unwrap_or(100));
    let mut state = State::new(&args.model, &args.provider, &args.cwd, width);

    // ---- frame source: live socket or replay file ----
    let (tx, mut rx) = mpsc::unbounded_channel::<Option<String>>();
    let mut sink: Option<net::WsSink> = None;
    let replaying = args.replay.is_some();
    if let Some(path) = &args.replay {
        let text = std::fs::read_to_string(path)?;
        let delay = args.replay_delay_ms;
        tokio::spawn(async move {
            for l in text.lines().filter(|l| !l.trim().is_empty()) {
                if let Some(f) = frame_from_replay_line(l) {
                    if delay > 0 {
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    }
                    let _ = tx.send(Some(f));
                }
            }
            let _ = tx.send(None);
        });
    } else {
        let att = net::attach(&args.home).await.map_err(|e| anyhow!("attach failed: {e}"))?;
        sink = Some(att.sink);
        let mut read = att.read;
        tokio::spawn(async move {
            while let Some(m) = read.next().await {
                match m {
                    Ok(Message::Text(t)) => {
                        let _ = tx.send(Some(t.to_string()));
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
            let _ = tx.send(None);
        });
    }
    let mut record = match &args.record {
        Some(p) if !replaying => Some(std::fs::File::create(p)?),
        _ => None,
    };

    let mut tui = if tty { Some(Tui::new(2)?) } else { None };
    let (ktx, mut krx) = mpsc::unbounded_channel::<TermEvent>();
    let keythread = if tty { Some(KeyThread::spawn(ktx)) } else { None };
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let mut req_id: i64 = 100;
    let mut interrupt_sent = false;
    let mut exit = false;
    let mut source_closed = false;

    if let Some(p) = &args.prompt {
        state.submit_prompt(p, now()); // queued until the session exists
    }

    macro_rules! send_all {
        ($outs:expr) => {
            for o in $outs {
                if let Some(s) = sink.as_mut() {
                    req_id += 1;
                    s.send(Message::text(rpc_text(req_id, &o))).await.ok();
                }
            }
        };
    }

    let result: Result<()> = async {
        loop {
            tokio::select! {
                f = rx.recv() => {
                    match f {
                        Some(Some(raw)) => {
                            if let Some(file) = record.as_mut() {
                                let line = json!({"t_ms": now(), "frame": raw}).to_string();
                                writeln!(file, "{line}")?;
                            }
                            let was_running = state.running;
                            let outs = state.on_frame(&raw, now());
                            send_all!(outs);
                            if was_running && !state.running { interrupt_sent = false; }
                        }
                        _ => source_closed = true,
                    }
                }
                k = krx.recv(), if tty => {
                    match k {
                        Some(TermEvent::Key(key)) if key.kind != KeyEventKind::Release => {
                            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                            match key.code {
                                KeyCode::Char('c') if ctrl => {
                                    if state.running && !interrupt_sent {
                                        interrupt_sent = true;
                                        if let Some(o) = state.interrupt() { send_all!([o]); }
                                    } else { exit = true; }
                                }
                                KeyCode::Char('d') if ctrl && state.input.is_empty() => exit = true,
                                _ if state.approval.is_some() => match key.code {
                                    KeyCode::Char(c @ '1'..='9') => {
                                        if let Some(o) = state.approval_answer(Some(c as usize - '1' as usize)) { send_all!([o]); }
                                    }
                                    KeyCode::Up => state.approval_move(-1),
                                    KeyCode::Down => state.approval_move(1),
                                    KeyCode::Enter => {
                                        if let Some(o) = state.approval_answer(None) { send_all!([o]); }
                                    }
                                    _ => {}
                                },
                                KeyCode::Enter => {
                                    if !state.running && state.session_id.is_some() && !state.input.trim().is_empty() {
                                        let text = std::mem::take(&mut state.input);
                                        if let Some(o) = state.submit_prompt(text.trim(), now()) { send_all!([o]); }
                                    }
                                }
                                KeyCode::Backspace => { state.input.pop(); }
                                KeyCode::Char(c) if !ctrl => state.input.push(c),
                                _ => {}
                            }
                        }
                        Some(TermEvent::Resize(w, h)) => {
                            state.width = w.max(20);
                            if let Some(t) = tui.as_mut() { t.resized(w, h)?; }
                        }
                        Some(TermEvent::Paste(p)) => state.input.push_str(&p.replace('\n', " ")),
                        Some(_) => {}
                        _ => exit = true,
                    }
                }
                _ = tick.tick() => {}
            }

            // plain mode: auto answers and printing
            if let Some(choice) = &args.auto_approve {
                if let Some(a) = &state.approval {
                    if let Some(i) = a.choices.iter().position(|c| c == choice) {
                        if let Some(o) = state.approval_answer(Some(i)) { send_all!([o]); }
                    }
                }
            }
            let committed = state.take_committed();
            match tui.as_mut() {
                Some(t) => {
                    let h = ui::live_height(&state);
                    if t.height() != h {
                        t.set_height(h)?;
                    }
                    t.insert(&committed)?;
                    t.draw(&state, now())?;
                }
                None => for l in &committed { println!("{}", l.text); },
            }
            let finished = !state.running && state.phase != Phase::Connecting && state.metrics.turns > 0 && args.prompt.is_some();
            if exit || (source_closed && (replaying || sink.is_some())) { break; }
            if !tty && finished && !replaying { break; }
            if args.attach_only && state.metrics.ready_ms.is_some() { break; }
        }
        Ok(())
    }.await;

    // flush any last lines, then leave the terminal clean.
    if replaying { let _ = state.take_committed(); }
    if let Some(o) = state.detach() {
        if let Some(s) = sink.as_mut() {
            s.send(Message::text(rpc_text(req_id + 1, &o))).await.ok();
            // give the server a moment to answer; record whatever arrives
            let deadline = Instant::now() + Duration::from_millis(400);
            while let Ok(Some(Some(raw))) = tokio::time::timeout(deadline.saturating_duration_since(Instant::now()), rx.recv()).await {
                if let Some(file) = record.as_mut() {
                    writeln!(file, "{}", json!({"t_ms": now(), "frame": raw}))?;
                }
            }
            s.close().await.ok();
        }
    }
    if let Some(k) = &keythread { k.stop(); }
    if let Some(t) = tui.as_mut() { t.restore()?; }
    if let Some(p) = &args.metrics {
        let m = &state.metrics;
        std::fs::write(p, serde_json::to_string_pretty(&json!({
            "attach_ms": m.ready_ms, "first_char_ms": m.first_char_ms, "turns": m.turns, "frames": m.frames
        }))?)?;
    }
    result
}
