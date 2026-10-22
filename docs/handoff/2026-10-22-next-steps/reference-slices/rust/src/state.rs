//! Pure state machine: frames in, committed lines + live region + outgoing requests out.
//! Knows nothing about sockets or the terminal, so replay and tests drive it directly.
use crate::proto::{decode, Approval, Event, EventFrame, Incoming};
use serde_json::{json, Value};
use std::collections::VecDeque;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub const TAIL_ROWS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Assistant,
    User,
    Tool,
    DiffAdd,
    DiffDel,
    DiffHunk,
    Dim,
    Error,
    Notice,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutLine {
    pub kind: Kind,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Connecting,
    Idle,
    Thinking,
    Streaming,
    Tool,
    WaitingApproval,
    Done,
    Failed,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Phase::Connecting => "connecting",
            Phase::Idle => "ready",
            Phase::Thinking => "thinking",
            Phase::Streaming => "streaming",
            Phase::Tool => "running tool",
            Phase::WaitingApproval => "waiting for approval",
            Phase::Done => "done",
            Phase::Failed => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outgoing {
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone)]
struct RunningTool {
    id: String,
    name: String,
    preview: String,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Metrics {
    pub ready_ms: Option<u64>,
    pub first_char_ms: Vec<u64>,
    pub turns: u32,
    pub frames: u64,
}

pub struct State {
    pub model: String,
    pub provider: String,
    pub cwd: String,
    pub width: u16,
    pub phase: Phase,
    pub running: bool,
    pub input: String,
    pub approval: Option<Approval>,
    pub approval_sel: usize,
    pub metrics: Metrics,
    pub notice: Option<String>,
    // session identity
    pub session_id: Option<String>,
    pub subscription_id: Option<String>,
    pub exec_gen: i64,
    pending_prompts: VecDeque<String>,
    committed: Vec<OutLine>,
    // streaming tail: finished wrapped lines not yet committed + the unfinished logical line
    pending: VecDeque<String>,
    partial: String,
    streamed_this_message: bool,
    segment_started: bool,
    tools: Vec<RunningTool>,
    // timing (all in ms on the caller's clock)
    turn_started_ms: Option<u64>,
    turn_elapsed_ms: u64,
    awaiting_first_char: bool,
    next_id: i64,
    describe: Option<Value>,
}

impl State {
    pub fn new(model: &str, provider: &str, cwd: &str, width: u16) -> Self {
        State {
            model: model.into(),
            provider: provider.into(),
            cwd: cwd.into(),
            width: width.max(20),
            phase: Phase::Connecting,
            running: false,
            input: String::new(),
            approval: None,
            approval_sel: 0,
            metrics: Metrics::default(),
            notice: None,
            session_id: None,
            subscription_id: None,
            exec_gen: 0,
            pending_prompts: VecDeque::new(),
            committed: vec![],
            pending: VecDeque::new(),
            partial: String::new(),
            streamed_this_message: false,
            segment_started: false,
            tools: vec![],
            turn_started_ms: None,
            turn_elapsed_ms: 0,
            awaiting_first_char: false,
            next_id: 10,
            describe: None,
        }
    }

    pub fn next_id(&mut self) -> i64 {
        self.next_id += 1;
        self.next_id
    }

    /// Drain lines that should go into the terminal's scrollback.
    pub fn take_committed(&mut self) -> Vec<OutLine> {
        std::mem::take(&mut self.committed)
    }

    pub fn elapsed_ms(&self, now_ms: u64) -> u64 {
        match (self.running, self.turn_started_ms) {
            (true, Some(s)) => now_ms.saturating_sub(s),
            _ => self.turn_elapsed_ms,
        }
    }

    /// Lines for the streaming-tail area: running tools, then unfinished assistant text.
    pub fn live_lines(&self) -> Vec<OutLine> {
        let mut v: Vec<OutLine> = vec![];
        for t in &self.tools {
            v.push(OutLine { kind: Kind::Tool, text: format!("⏺ {}: {} …", t.name, t.preview) });
        }
        for l in &self.pending {
            v.push(OutLine { kind: Kind::Assistant, text: l.clone() });
        }
        if !self.partial.is_empty() {
            v.push(OutLine { kind: Kind::Assistant, text: self.partial.clone() });
        }
        let n = v.len();
        if n > TAIL_ROWS {
            v.drain(..n - TAIL_ROWS);
        }
        v
    }

    fn commit(&mut self, kind: Kind, text: &str) {
        for l in text.split('\n') {
            for w in wrap(l, self.width as usize) {
                self.committed.push(OutLine { kind, text: w });
            }
        }
    }

    /// The user typed a prompt (or the command line supplied one).
    pub fn submit_prompt(&mut self, text: &str, now_ms: u64) -> Option<Outgoing> {
        self.commit(Kind::User, &format!("› {text}"));
        if self.session_id.is_none() {
            self.pending_prompts.push_back(text.to_owned());
            return None;
        }
        Some(self.begin_turn(text, now_ms))
    }

    fn begin_turn(&mut self, text: &str, now_ms: u64) -> Outgoing {
        self.running = true;
        self.phase = Phase::Thinking;
        self.turn_started_ms = Some(now_ms);
        self.turn_elapsed_ms = 0;
        self.awaiting_first_char = true;
        self.metrics.turns += 1;
        Outgoing {
            method: "prompt.submit".into(),
            params: json!({"session_id": self.session_id, "text": text,
                           "input_id": format!("slice-{}-{}", std::process::id(), self.metrics.turns)}),
        }
    }

    pub fn interrupt(&self) -> Option<Outgoing> {
        let sid = self.session_id.as_ref()?;
        Some(Outgoing {
            method: "session.interrupt".into(),
            params: json!({"session_id": sid, "execution_generation": self.exec_gen}),
        })
    }

    pub fn detach(&self) -> Option<Outgoing> {
        Some(Outgoing {
            method: "session.detach".into(),
            params: json!({"session_id": self.session_id.as_ref()?, "subscription_id": self.subscription_id.as_ref()?}),
        })
    }

    // ---- approvals -------------------------------------------------------------------

    pub fn approval_move(&mut self, delta: i32) {
        if let Some(a) = &self.approval {
            let n = a.choices.len() as i32;
            self.approval_sel = ((self.approval_sel as i32 + delta).rem_euclid(n)) as usize;
        }
    }

    /// Answer the pending approval with choice index `idx` (None -> highlighted row).
    pub fn approval_answer(&mut self, idx: Option<usize>) -> Option<Outgoing> {
        let a = self.approval.as_ref()?;
        let i = idx.unwrap_or(self.approval_sel);
        let choice = a.choices.get(i)?.clone();
        let gen = a.execution_generation.unwrap_or(self.exec_gen);
        let out = Outgoing {
            method: "approval.respond".into(),
            params: json!({"session_id": self.session_id, "execution_generation": gen,
                           "prompt_id": a.prompt_id, "choice": choice}),
        };
        self.commit(Kind::Notice, &format!("  approval: {choice}"));
        self.approval = None;
        self.phase = Phase::Thinking;
        Some(out)
    }

    // ---- frames ----------------------------------------------------------------------

    /// Feed one raw received frame. Returns follow-up requests to send.
    pub fn on_frame(&mut self, raw: &str, now_ms: u64) -> Vec<Outgoing> {
        self.metrics.frames += 1;
        match decode(raw) {
            Incoming::Event(f) => self.on_event(f, now_ms),
            Incoming::Response { result, error, .. } => self.on_response(result, error, now_ms),
            Incoming::Other(_) => vec![],
        }
    }

    fn on_response(&mut self, result: Option<Value>, error: Option<Value>, now_ms: u64) -> Vec<Outgoing> {
        if let Some(e) = error {
            let msg = e.get("message").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(|| e.to_string());
            self.commit(Kind::Error, &format!("✗ request failed: {msg}"));
            self.running = false;
            self.phase = Phase::Failed;
            return vec![];
        }
        let Some(r) = result else { return vec![] };
        if r.get("session_create").is_some() {
            // runtime.describe result: now create the session.
            self.describe = Some(r.clone());
            let sources = r["session_create"]["sources"].as_array().cloned().unwrap_or_default();
            let source = if sources.iter().any(|s| s == "tui") { "tui" } else { "cli" };
            return vec![Outgoing {
                method: "session.create".into(),
                params: json!({"source": source, "model": self.model, "provider": self.provider, "cwd": self.cwd}),
            }];
        }
        if r.get("subscription_id").is_some() && r.get("stored_session_id").is_some() {
            // CanonicalSnapshot from session.create
            self.session_id = r["session_id"].as_str().map(str::to_owned);
            self.subscription_id = r["subscription_id"].as_str().map(str::to_owned);
            self.exec_gen = r["execution_generation"].as_i64().unwrap_or(0);
            if let Some(m) = r["info"].get("model").and_then(Value::as_str) {
                if !m.is_empty() {
                    self.model = m.to_owned();
                }
            }
            self.phase = Phase::Idle;
            if let Some(p) = self.pending_prompts.pop_front() {
                return vec![self.begin_turn(&p, now_ms)];
            }
            return vec![];
        }
        if r.get("admission_id").is_some() && !self.running {
            // Admission receipt for a prompt (also when replaying a recording): the turn is live.
            self.running = true;
            self.phase = Phase::Thinking;
            self.turn_started_ms = Some(now_ms);
            self.turn_elapsed_ms = 0;
            self.awaiting_first_char = true;
            self.metrics.turns += 1;
        }
        if let Some(g) = r.get("execution_generation").and_then(Value::as_i64) {
            self.exec_gen = g;
        }
        vec![]
    }

    fn on_event(&mut self, f: EventFrame, now_ms: u64) -> Vec<Outgoing> {
        if let Some(g) = f.execution_generation {
            self.exec_gen = g;
        }
        match f.event {
            Event::Ready => {
                if self.metrics.ready_ms.is_none() {
                    self.metrics.ready_ms = Some(now_ms);
                }
                self.phase = Phase::Idle;
                return vec![Outgoing { method: "runtime.describe".into(), params: json!({}) }];
            }
            Event::MessageStart => {
                self.streamed_this_message = false;
                if self.running {
                    self.phase = Phase::Thinking;
                }
            }
            Event::ReasoningDelta { .. } => {
                if self.running && self.phase != Phase::Streaming {
                    self.phase = Phase::Thinking;
                }
            }
            Event::MessageDelta { text } => match text {
                None => self.flush_tail(),
                Some(t) => {
                    self.first_char(now_ms, !t.is_empty());
                    self.phase = Phase::Streaming;
                    self.streamed_this_message = true;
                    self.append_text(&t);
                }
            },
            Event::MessageComplete { text, status, error, warning } => {
                if !self.streamed_this_message {
                    if let Some(t) = text.filter(|t| !t.is_empty()) {
                        self.first_char(now_ms, true);
                        self.append_text(&t);
                    }
                }
                self.flush_tail();
                if let Some(w) = warning.filter(|w| !w.is_empty()) {
                    self.commit(Kind::Notice, &format!("! {w}"));
                }
                let failed = status.as_deref() == Some("error") || error.is_some();
                if let Some(e) = error {
                    self.commit(Kind::Error, &format!("✗ {e}"));
                }
                if status.as_deref() == Some("interrupted") {
                    self.commit(Kind::Notice, "⎋ interrupted");
                }
                self.end_turn(now_ms, failed);
            }
            Event::ToolStart { id, name, preview } => {
                self.flush_tail();
                self.tools.push(RunningTool { id, name, preview });
                self.phase = Phase::Tool;
            }
            Event::ToolComplete { id, name, preview, duration_s, diff, failed } => {
                self.flush_tail();
                let started = self.tools.iter().position(|t| t.id == id).map(|i| self.tools.remove(i));
                let preview = if preview.is_empty() { started.map(|t| t.preview).unwrap_or_default() } else { preview };
                let mark = if failed { "✗" } else { "✓" };
                let dur = duration_s.map(|d| format!(" {d:.1}s")).unwrap_or_default();
                let head = if preview.is_empty() { format!("⏺ {name}") } else { format!("⏺ {name}: {}", one_line(&preview)) };
                let budget = (self.width as usize).saturating_sub(mark.width() + dur.width() + 2);
                let head = truncate(&head, budget);
                self.committed.push(OutLine { kind: Kind::Tool, text: format!("{head} {mark}{dur}") });
                if let Some(d) = diff {
                    self.commit_diff(&d);
                }
                if self.tools.is_empty() && self.running && self.phase == Phase::Tool {
                    self.phase = Phase::Thinking;
                }
            }
            Event::StatusUpdate { text, .. } => {
                self.notice = if text.is_empty() { None } else { Some(text) };
            }
            Event::SessionUsage { model, .. } => {
                if let Some(m) = model.filter(|m| !m.is_empty()) {
                    self.model = m;
                }
            }
            Event::Error { message } => {
                self.flush_tail();
                self.commit(Kind::Error, &format!("✗ {message}"));
                self.end_turn(now_ms, true);
            }
            Event::ApprovalRequest(a) => {
                self.flush_tail();
                self.approval = Some(a);
                self.approval_sel = 0;
                self.phase = Phase::WaitingApproval;
            }
            Event::ApprovalSettled { prompt_id } => {
                if self.approval.as_ref().map(|a| a.prompt_id == prompt_id).unwrap_or(false) {
                    self.approval = None;
                    if self.running {
                        self.phase = Phase::Thinking;
                    }
                }
            }
            Event::Other(_) => {}
        }
        vec![]
    }

    /// Diff body under a tool cell: coloured by sign, wrapped with a hanging indent, capped.
    fn commit_diff(&mut self, d: &str) {
        const MAX_ROWS: usize = 24;
        let total = d.lines().count();
        let mut rows = 0;
        for (idx, l) in d.lines().enumerate() {
            let body = l.trim_end();
            let kind = if body.trim_start().starts_with('┊') {
                Kind::Dim
            } else if body.starts_with("+++") || body.starts_with("---") || body.starts_with("@@") || (body.starts_with("a/") && body.contains(" → ")) {
                Kind::DiffHunk
            } else if body.starts_with('+') {
                Kind::DiffAdd
            } else if body.starts_with('-') {
                Kind::DiffDel
            } else {
                Kind::Dim
            };
            let width = (self.width as usize).saturating_sub(4).max(8);
            for (i, w) in wrap(body, width).into_iter().enumerate() {
                if rows >= MAX_ROWS {
                    let left = total - idx;
                    self.committed.push(OutLine { kind: Kind::Dim, text: format!("  … {left} more diff lines") });
                    return;
                }
                self.committed.push(OutLine { kind, text: format!("{}{w}", if i == 0 { "  " } else { "    " }) });
                rows += 1;
            }
        }
    }

    fn end_turn(&mut self, now_ms: u64, failed: bool) {
        if let Some(s) = self.turn_started_ms.take() {
            self.turn_elapsed_ms = now_ms.saturating_sub(s);
        }
        self.running = false;
        self.tools.clear();
        self.approval = None;
        self.phase = if failed { Phase::Failed } else { Phase::Done };
        self.committed.push(OutLine { kind: Kind::Dim, text: String::new() });
    }

    fn first_char(&mut self, now_ms: u64, real: bool) {
        if real && self.awaiting_first_char {
            self.awaiting_first_char = false;
            if let Some(s) = self.turn_started_ms {
                self.metrics.first_char_ms.push(now_ms.saturating_sub(s));
            }
        }
    }

    // ---- streaming tail --------------------------------------------------------------

    fn append_text(&mut self, t: &str) {
        let w = self.width as usize;
        // A segment (text after a tool call or boundary) may open with blank lines: drop them.
        let t = if self.segment_started { t } else { t.trim_start_matches('\n') };
        if t.is_empty() {
            return;
        }
        self.segment_started = true;
        let mut parts = t.split('\n');
        if let Some(first) = parts.next() {
            self.partial.push_str(first);
        }
        for next in parts {
            let done = std::mem::take(&mut self.partial);
            for l in wrap(&done, w) {
                self.pending.push_back(l);
            }
            self.partial = next.to_owned();
        }
        // A long unfinished line: its leading wrapped rows are stable.
        let mut rows = wrap(&self.partial, w);
        if rows.len() > 1 {
            let last = rows.pop().unwrap_or_default();
            for r in rows {
                self.pending.push_back(r);
            }
            self.partial = last;
        }
        while self.pending.len() + usize::from(!self.partial.is_empty()) > TAIL_ROWS - 1 {
            match self.pending.pop_front() {
                Some(l) => self.committed.push(OutLine { kind: Kind::Assistant, text: l }),
                None => break,
            }
        }
    }

    fn flush_tail(&mut self) {
        self.segment_started = false;
        while let Some(l) = self.pending.pop_front() {
            self.committed.push(OutLine { kind: Kind::Assistant, text: l });
        }
        if !self.partial.is_empty() {
            let p = std::mem::take(&mut self.partial);
            for l in wrap(&p, self.width as usize) {
                self.committed.push(OutLine { kind: Kind::Assistant, text: l });
            }
        }
    }
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.width() <= max {
        return s.to_owned();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if w + cw + 1 > max {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}

/// Word wrap to `width` display columns. Breaks at a space when possible (the space is
/// dropped), otherwise mid-word. An empty input yields one empty line.
pub fn wrap(s: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = vec![];
    let mut rest = s;
    loop {
        if rest.width() <= width {
            out.push(rest.to_owned());
            return out;
        }
        let mut w = 0;
        let mut cut = 0; // byte index where the line must end (exclusive)
        let mut last_space: Option<usize> = None;
        for (i, c) in rest.char_indices() {
            let cw = c.width().unwrap_or(0);
            if w + cw > width {
                break;
            }
            if c == ' ' {
                last_space = Some(i);
            }
            w += cw;
            cut = i + c.len_utf8();
        }
        let space_at_edge = rest[cut..].starts_with(' ');
        match last_space {
            _ if space_at_edge && cut > 0 => {
                out.push(rest[..cut].to_owned());
                rest = &rest[cut + 1..];
            }
            Some(i) if i > 0 => {
                out.push(rest[..i].to_owned());
                rest = &rest[i + 1..];
            }
            _ => {
                if cut == 0 {
                    cut = rest.chars().next().map(char::len_utf8).unwrap_or(1);
                }
                out.push(rest[..cut].to_owned());
                rest = &rest[cut..];
            }
        }
        if rest.is_empty() {
            return out;
        }
    }
}
