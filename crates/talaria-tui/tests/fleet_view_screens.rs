//! Fleet overlay screens. Expected rows are built here; the draw is compared to them.

mod support;

use std::time::Instant;

use hermes_contract::schemas::CanonicalSnapshot;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use serde_json::json;
use support::{ByteSink, Vt, live_view_sections, probe};
use talaria_core::directory::rows::{RowKey, format_age};
use talaria_core::fleet::{
    FleetRow, NeedDetail, NeedItem, NeedSummary, Outcome, ProfileStatus, RowState,
};
use talaria_core::fleet_view::{FleetViewInput, FleetViewState, update};
use talaria_core::session::{
    CellId, PromptCell, PromptKey, PromptKind, PromptState, SessionId, SessionState,
};
use talaria_tui::fleet_view::{FleetTheme, FleetView, PeekPanel};
use talaria_tui::history::sanitize;
use talaria_tui::live::{LiveSection, LiveView, PlainTextSection};
use talaria_tui::prompts::PromptStack;
use talaria_tui::terminal::{InlineTerminal, RestoreLink};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const NOW: f64 = 1_792_700_000.0;
const FOOTER: &str = "enter open \u{b7} space peek \u{b7} r reply \u{b7} x stop \u{b7} / filter \u{b7} esc close \u{b7} o new pane";
const FOOTER_ANSWER: &str = "enter open \u{b7} space peek \u{b7} r reply \u{b7} x stop \u{b7} a answer \u{b7} / filter \u{b7} esc close \u{b7} o new pane";

struct Marks {
    stale: Option<(u16, u16)>,
    fresh: Option<(u16, u16)>,
}

fn row(profile: &str, id: &str, title: &str, state: RowState, model: Option<&str>) -> FleetRow {
    FleetRow {
        key: RowKey {
            profile: profile.to_owned(),
            session_id: id.to_owned(),
        },
        title: title.to_owned(),
        model: model.map(str::to_owned),
        source: Some("tui".to_owned()),
        started_at: Some(NOW - 30.0),
        message_count: 2,
        state,
        state_since: NOW - 10.0,
        outcome: None,
        foreground: false,
        subscribed: false,
        stale: false,
        gone: false,
        unlisted: false,
        activity: None,
        cost_usd: Some(1.25),
    }
}

fn listed(names: &[&str]) -> Vec<(String, ProfileStatus)> {
    names
        .iter()
        .map(|name| {
            (
                (*name).to_owned(),
                ProfileStatus::Listed { truncated: false },
            )
        })
        .collect()
}

fn fit(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let clean = sanitize(text);
    let text = clean.as_ref();
    if UnicodeWidthStr::width(text) <= width {
        let mut out = text.to_owned();
        while UnicodeWidthStr::width(out.as_str()) < width {
            out.push(' ');
        }
        return out;
    }
    let limit = width - 1;
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > limit {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('\u{2026}');
    while UnicodeWidthStr::width(out.as_str()) < width {
        out.push(' ');
    }
    out
}

fn glyph(state: RowState) -> char {
    match state {
        RowState::NeedsYou => '\u{25cf}',
        RowState::Working => '\u{25d0}',
        RowState::Failed => '\u{2717}',
        RowState::Done => '\u{2713}',
        RowState::Idle => '\u{00b7}',
    }
}

fn group_name(state: RowState) -> &'static str {
    match state {
        RowState::NeedsYou => "NEEDS YOU",
        RowState::Working => "WORKING",
        RowState::Failed => "FAILED",
        RowState::Done => "DONE",
        RowState::Idle => "IDLE",
    }
}

fn title_text(row: &FleetRow) -> String {
    let base = if row.title.is_empty() {
        "(untitled)".to_owned()
    } else {
        sanitize(&row.title).into_owned()
    };
    let suffix = if row.foreground {
        " (here)"
    } else if row.gone {
        " (ended)"
    } else if row.unlisted {
        " (hidden)"
    } else {
        ""
    };
    format!("{base}{suffix}")
}

fn detail_text(row: &FleetRow, queue: &[NeedItem]) -> String {
    if row.state == RowState::NeedsYou {
        let item = queue.iter().find(|item| item.id.key == row.key);
        return match item.map(|item| &item.detail) {
            Some(NeedDetail::Known(NeedSummary::Approval { command, .. })) => {
                format!("approval: {command}")
            }
            Some(NeedDetail::Known(NeedSummary::Question(question))) => {
                format!("question: {question}")
            }
            Some(NeedDetail::Known(NeedSummary::Sudo)) => "sudo password".to_owned(),
            Some(NeedDetail::Known(NeedSummary::Secret)) => "secret".to_owned(),
            Some(
                NeedDetail::Known(NeedSummary::VaultCode | NeedSummary::VaultUnlock)
                | NeedDetail::Waiting,
            )
            | None => "waiting for an answer".to_owned(),
        };
    }
    match row.state {
        RowState::Working if row.message_count == 0 => "starting".to_owned(),
        RowState::Working => row.activity.clone().unwrap_or_else(|| "working".to_owned()),
        RowState::Failed => "failed".to_owned(),
        RowState::Done => match row.outcome {
            Some(Outcome::Completed) => "completed".to_owned(),
            Some(Outcome::Interrupted) => "interrupted".to_owned(),
            Some(Outcome::Unknown) | None => "finished".to_owned(),
            Some(Outcome::Failed) => "failed".to_owned(),
        },
        RowState::Idle => "idle".to_owned(),
        RowState::NeedsYou => "waiting for an answer".to_owned(),
    }
}

fn short_model(model: &Option<String>) -> String {
    match model {
        Some(model) if !model.is_empty() => model.rsplit('/').next().unwrap_or(model).to_owned(),
        _ => "-".to_owned(),
    }
}

fn ordered(rows: &[FleetRow]) -> Vec<&FleetRow> {
    let mut out = Vec::new();
    for state in [
        RowState::NeedsYou,
        RowState::Working,
        RowState::Failed,
        RowState::Done,
        RowState::Idle,
    ] {
        out.extend(rows.iter().filter(|row| row.state == state));
    }
    out
}

fn header_line(count: usize, link: Option<&str>, width: usize) -> String {
    let left = format!("Fleet \u{b7} {count} sessions");
    let Some(link) = link else {
        return left;
    };
    let note = sanitize(link).into_owned();
    let left_w = UnicodeWidthStr::width(left.as_str());
    let note_w = UnicodeWidthStr::width(note.as_str());
    if left_w + note_w >= width {
        return fit(&left, width).trim_end().to_owned();
    }
    let gap = width - left_w - note_w;
    format!("{left}{}{note}", " ".repeat(gap))
}

fn not_listed(profiles: &[(String, ProfileStatus)]) -> Vec<String> {
    let mut lines = Vec::new();
    for (name, status) in profiles {
        let reason = match status {
            ProfileStatus::Listed { .. } | ProfileStatus::Loading => continue,
            ProfileStatus::Failed { reason } | ProfileStatus::Parked { reason } => reason.clone(),
            ProfileStatus::OutOfScope => "out of scope".to_owned(),
        };
        lines.push(format!("not listed: {name} ({reason})"));
    }
    if lines.len() > 2 {
        let extra = lines.len() - 2;
        lines.truncate(2);
        lines.push(format!("\u{2026} and {extra} more profiles not listed"));
    }
    lines
}

struct Expect<'a> {
    rows: &'a [FleetRow],
    queue: &'a [NeedItem],
    profiles: &'a [(String, ProfileStatus)],
    scope_note: Option<&'a str>,
    link_note: Option<&'a str>,
    peek: &'a [String],
    answer: bool,
    snapshot: bool,
    width: usize,
    height: usize,
}

fn expect_screen(spec: Expect<'_>) -> (Vec<String>, Marks) {
    let mut marks = Marks {
        stale: None,
        fresh: None,
    };
    if spec.width < 40 || spec.height < 10 {
        let mut lines = vec![String::new(); spec.height];
        // The text is 48 columns; a narrower screen shows it on two rows.
        if spec.width < 48 {
            lines[0] = "Window too small for the fleet view.".to_owned();
            lines[1] = "Esc closes.".to_owned();
        } else {
            lines[0] = "Window too small for the fleet view. Esc closes.".to_owned();
        }
        return (lines, marks);
    }
    let mut body = Vec::new();
    if !spec.snapshot {
        body.push("Looking for sessions\u{2026}".to_owned());
    } else {
        body.push(header_line(spec.rows.len(), spec.link_note, spec.width));
        if let Some(note) = spec.scope_note {
            body.push(note.trim_end().to_owned());
        }
        let visible = ordered(spec.rows);
        if visible.is_empty() {
            body.push("No live sessions in the served profiles.".to_owned());
        }
        let show_profile = visible
            .iter()
            .map(|row| row.key.profile.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            > 1
            && spec.width >= 80;
        let show_model = spec.width >= 60;
        let show_cost = spec.width >= 100;
        let cursor = visible.first().map(|row| row.key.clone());
        for state in [
            RowState::NeedsYou,
            RowState::Working,
            RowState::Failed,
            RowState::Done,
            RowState::Idle,
        ] {
            let group: Vec<_> = visible
                .iter()
                .copied()
                .filter(|row| row.state == state)
                .collect();
            if group.is_empty() {
                continue;
            }
            body.push(format!("{} ({})", group_name(state), group.len()));
            for sample in group {
                let y = body.len() as u16;
                if sample.stale {
                    marks.stale = Some((6, y));
                } else if marks.fresh.is_none() {
                    marks.fresh = Some((6, y));
                }
                body.push(format_row(
                    sample,
                    spec.queue,
                    &cursor,
                    show_profile,
                    show_model,
                    show_cost,
                    spec.width,
                ));
            }
        }
        body.extend(spec.peek.iter().map(|line| line.trim_end().to_owned()));
        body.extend(not_listed(spec.profiles));
    }
    let mut lines = vec![String::new(); spec.height];
    let room = spec.height.saturating_sub(1);
    for (index, line) in body.into_iter().take(room).enumerate() {
        lines[index] = line.trim_end().to_owned();
    }
    let footer = if spec.answer { FOOTER_ANSWER } else { FOOTER };
    // A footer wider than the screen is cut with an ellipsis, like every other value.
    lines[spec.height - 1] = fit(footer, spec.width).trim_end().to_owned();
    (lines, marks)
}

fn format_row(
    row: &FleetRow,
    queue: &[NeedItem],
    cursor: &Option<RowKey>,
    show_profile: bool,
    show_model: bool,
    show_cost: bool,
    width: usize,
) -> String {
    let mut fixed = 1 + 1 + 28 + 4;
    let mut count = 5usize;
    if show_profile {
        fixed += 12;
        count += 1;
    }
    if show_model {
        fixed += 18;
        count += 1;
    }
    if show_cost {
        fixed += 7;
        count += 1;
    }
    let detail_width = width.saturating_sub(fixed + (count - 1) * 2);
    let mut parts = vec![
        fit(
            if cursor.as_ref() == Some(&row.key) {
                "\u{203a}"
            } else {
                " "
            },
            1,
        ),
        fit(&glyph(row.state).to_string(), 1),
        fit(&title_text(row), 28),
    ];
    if show_profile {
        parts.push(fit(&row.key.profile, 12));
    }
    if show_model {
        parts.push(fit(&short_model(&row.model), 18));
    }
    parts.push(fit(&detail_text(row, queue), detail_width));
    parts.push(fit(&format_age(Some(row.state_since), NOW), 4));
    if show_cost {
        let text = row
            .cost_usd
            .map(|value| format!("${value:.2}"))
            .unwrap_or_default();
        parts.push(fit(&text, 7));
    }
    parts.join("  ").trim_end().to_owned()
}

fn paint(
    state: &FleetViewState,
    peek: Option<PeekPanel<'_>>,
    width: u16,
    height: u16,
) -> (Vec<String>, Buffer) {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            FleetView::draw(
                state,
                peek,
                frame.area(),
                frame.buffer_mut(),
                &FleetTheme::default(),
            );
        })
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let rows = (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect();
    (rows, buffer)
}

fn hold(
    rows: Vec<FleetRow>,
    profiles: Vec<(String, ProfileStatus)>,
    scope: Option<String>,
    link: Option<String>,
) -> FleetViewState {
    let mut state = FleetViewState::new(None, NOW);
    update(
        &mut state,
        FleetViewInput::Snapshot {
            version: 1,
            rows,
            queue: Vec::new(),
            profiles,
            scope_note: scope,
            link_note: link,
        },
        8,
    );
    state
}

fn assert_drawn(actual: &[String], expected: &[String]) {
    assert!(
        expected.iter().any(|line| {
            line.contains("Fleet \u{b7}")
                || line.contains("Window too small")
                || line.contains("Looking for sessions")
        }),
        "the expected screen is blank"
    );
    assert_eq!(actual, expected);
}

fn wide_rows() -> Vec<FleetRow> {
    let mut needs = row(
        "fixture",
        "s-need",
        "deploy",
        RowState::NeedsYou,
        Some("ollama-cloud/minimax-m3.1-flash"),
    );
    needs.cost_usd = Some(1.25);
    let mut working = row(
        "default",
        "s-work",
        "compile",
        RowState::Working,
        Some("ollama-cloud/minimax-m3.1-flash"),
    );
    working.foreground = true;
    working.activity = Some("compiling".to_owned());
    working.cost_usd = Some(2.50);
    let mut done = row(
        "fixture",
        "s-done",
        "ship",
        RowState::Done,
        Some("ollama-cloud/minimax-m3.1-flash"),
    );
    done.gone = true;
    done.outcome = Some(Outcome::Completed);
    done.cost_usd = Some(0.50);
    let mut idle = row("fixture", "s-idle", "rest", RowState::Idle, None);
    idle.unlisted = true;
    idle.stale = true;
    idle.cost_usd = Some(0.0);
    vec![needs, working, done, idle]
}

#[test]
fn groups_and_columns_160x48() {
    let rows = wide_rows();
    let profiles = listed(&["fixture", "default"]);
    let state = hold(
        rows.clone(),
        profiles.clone(),
        None,
        Some("connecting\u{2026}".to_owned()),
    );
    let (expected, marks) = expect_screen(Expect {
        rows: &rows,
        queue: &[],
        profiles: &profiles,
        scope_note: None,
        link_note: Some("connecting\u{2026}"),
        peek: &[],
        answer: false,
        snapshot: true,
        width: 160,
        height: 48,
    });
    let (actual, buffer) = paint(&state, None, 160, 48);
    let stale = marks.stale.expect("a stale row");
    assert!(
        buffer[stale].modifier.contains(Modifier::DIM),
        "the stale title is not dim"
    );
    let fresh = marks.fresh.expect("a fresh row");
    assert!(
        !buffer[fresh].modifier.contains(Modifier::DIM),
        "a fresh title is dim"
    );
    let joined = expected.join("\n");
    assert!(joined.contains("(here)") && joined.contains("(ended)") && joined.contains("(hidden)"));
    assert!(joined.contains("NEEDS YOU (1)"));
    assert_drawn(&actual, &expected);
}

#[test]
fn narrow_widths_drop_columns() {
    let rows = wide_rows();
    let profiles = listed(&["fixture", "default"]);
    let state = hold(rows.clone(), profiles.clone(), None, None);
    for (width, banned) in [(99u16, "$"), (79, "fixture"), (59, "minimax")] {
        let (expected, _) = expect_screen(Expect {
            rows: &rows,
            queue: &[],
            profiles: &profiles,
            scope_note: None,
            link_note: None,
            peek: &[],
            answer: false,
            snapshot: true,
            width: usize::from(width),
            height: 24,
        });
        let (actual, _) = paint(&state, None, width, 24);
        assert!(
            !expected.join("\n").contains(banned),
            "the {width}-column screen still has {banned}"
        );
        assert_drawn(&actual, &expected);
    }
}

fn finished_session() -> SessionState {
    let value = json!({
        "session_id": "s-peek",
        "stored_session_id": "stored-1",
        "messages": [{"role": "assistant", "text": "peek-ok"}],
        "message_count": 1,
        "running": false,
        "authority_epoch": 1,
        "replay_epoch": "epoch-1",
        "last_sequence": 1,
        "subscription_id": "sub-1",
        "revision": 1,
        "execution_generation": 1,
        "pending": [],
        "prompts": [],
        "info": {"desktop_protocol": "1"}
    });
    let snapshot: CanonicalSnapshot = serde_json::from_value(value).expect("snapshot");
    SessionState::from_snapshot(snapshot)
}

fn approval_stack() -> PromptStack {
    let cell = PromptCell {
        id: CellId::new(1),
        key: PromptKey {
            session_id: SessionId::new("s-peek"),
            prompt_id: "prompt-1".to_owned(),
            execution_generation: 1,
        },
        kind: PromptKind::Approval,
        command: Some("rm -rf ./talaria-peek-probe".to_owned()),
        description: None,
        question: None,
        choices: vec!["once".to_owned(), "deny".to_owned()],
        multi_select: false,
        edit: None,
        timeout_s: None,
        state: PromptState::Open,
        sealed: false,
    };
    let mut stack = PromptStack::new();
    stack.sync(Some("s-peek"), &[&cell], &[], true, Instant::now());
    stack
}

fn peek_row(outcome: Outcome) -> FleetRow {
    let mut sample = row(
        "fixture",
        "s-peek",
        "compile",
        RowState::Done,
        Some("ollama-cloud/minimax-m3.1-flash"),
    );
    sample.outcome = Some(outcome);
    sample.cost_usd = None;
    sample
}

#[test]
fn peek_panel_tail_card_and_outcome() {
    let session = finished_session();
    let completed = peek_row(Outcome::Completed);
    let profiles = listed(&["fixture"]);
    let tail = [
        "\u{2500}\u{2500} peek \u{2500}\u{2500}".to_owned(),
        "\u{2713} compile \u{b7} fixture \u{b7} ollama-cloud/minimax-m3.1-flash \u{b7} completed now".to_owned(),
        "peek-ok".to_owned(),
        "\u{2713} completed".to_owned(),
    ];
    let state = hold(vec![completed.clone()], profiles.clone(), None, None);
    let (expected, _) = expect_screen(Expect {
        rows: std::slice::from_ref(&completed),
        queue: &[],
        profiles: &profiles,
        scope_note: None,
        link_note: None,
        peek: &tail,
        answer: false,
        snapshot: true,
        width: 80,
        height: 24,
    });
    let (actual, _) = paint(
        &state,
        Some(PeekPanel {
            session: &session,
            stack: None,
            now: NOW,
        }),
        80,
        24,
    );
    assert_drawn(&actual, &expected);

    let card_lines = [
        "\u{2500}\u{2500} peek \u{2500}\u{2500}".to_owned(),
        "\u{2713} compile \u{b7} fixture \u{b7} ollama-cloud/minimax-m3.1-flash \u{b7} completed now".to_owned(),
        "Approval needed".to_owned(),
        "$ rm -rf ./talaria-peek-probe".to_owned(),
        "\u{203a} 1. once".to_owned(),
        "  2. deny".to_owned(),
        "1-2 choose \u{b7} Enter confirm \u{b7} Esc deny \u{b7} Tab note \u{b7} Ctrl+C deny and stop".to_owned(),
        "\u{2713} completed".to_owned(),
    ];
    let stack = approval_stack();
    let (expected, _) = expect_screen(Expect {
        rows: std::slice::from_ref(&completed),
        queue: &[],
        profiles: &profiles,
        scope_note: None,
        link_note: None,
        peek: &card_lines,
        answer: true,
        snapshot: true,
        width: 80,
        height: 24,
    });
    let (actual, _) = paint(
        &state,
        Some(PeekPanel {
            session: &session,
            stack: Some(&stack),
            now: NOW,
        }),
        80,
        24,
    );
    assert!(
        expected.iter().any(|line| line.contains("a answer")),
        "the card footer dropped a answer"
    );
    assert_drawn(&actual, &expected);

    let interrupted = peek_row(Outcome::Interrupted);
    let stopped = [
        "\u{2500}\u{2500} peek \u{2500}\u{2500}".to_owned(),
        "\u{2713} compile \u{b7} fixture \u{b7} ollama-cloud/minimax-m3.1-flash \u{b7} interrupted now".to_owned(),
        "peek-ok".to_owned(),
        "\u{25a0} interrupted".to_owned(),
    ];
    let state = hold(vec![interrupted.clone()], profiles.clone(), None, None);
    let (expected, _) = expect_screen(Expect {
        rows: &[interrupted],
        queue: &[],
        profiles: &profiles,
        scope_note: None,
        link_note: None,
        peek: &stopped,
        answer: false,
        snapshot: true,
        width: 80,
        height: 24,
    });
    let (actual, _) = paint(
        &state,
        Some(PeekPanel {
            session: &session,
            stack: None,
            now: NOW,
        }),
        80,
        24,
    );
    assert_drawn(&actual, &expected);
}

#[test]
fn empty_loading_scope_and_failure_states() {
    let empty_state = FleetViewState::new(None, NOW);
    let (expected, _) = expect_screen(Expect {
        rows: &[],
        queue: &[],
        profiles: &[],
        scope_note: None,
        link_note: None,
        peek: &[],
        answer: false,
        snapshot: false,
        width: 80,
        height: 24,
    });
    let (actual, _) = paint(&empty_state, None, 80, 24);
    assert_drawn(&actual, &expected);

    let profiles = listed(&["fixture"]);
    let state = hold(Vec::new(), profiles.clone(), None, None);
    let (expected, _) = expect_screen(Expect {
        rows: &[],
        queue: &[],
        profiles: &profiles,
        scope_note: None,
        link_note: None,
        peek: &[],
        answer: false,
        snapshot: true,
        width: 80,
        height: 24,
    });
    let (actual, _) = paint(&state, None, 80, 24);
    assert_drawn(&actual, &expected);

    let scope = "Showing profile fixture only: the gateway refused a host-scoped ticket.";
    let link = "fleet connection failed: refused";
    let profiles = vec![
        (
            "fixture".to_owned(),
            ProfileStatus::Failed {
                reason: "connection refused".to_owned(),
            },
        ),
        (
            "default".to_owned(),
            ProfileStatus::Parked {
                reason: "home is busy".to_owned(),
            },
        ),
        ("<profile>".to_owned(), ProfileStatus::OutOfScope),
    ];
    let state = hold(
        Vec::new(),
        profiles.clone(),
        Some(scope.to_owned()),
        Some(link.to_owned()),
    );
    let (expected, _) = expect_screen(Expect {
        rows: &[],
        queue: &[],
        profiles: &profiles,
        scope_note: Some(scope),
        link_note: Some(link),
        peek: &[],
        answer: false,
        snapshot: true,
        width: 160,
        height: 24,
    });
    let (actual, _) = paint(&state, None, 160, 24);
    assert_drawn(&actual, &expected);

    let (expected, _) = expect_screen(Expect {
        rows: &[],
        queue: &[],
        profiles: &[],
        scope_note: None,
        link_note: None,
        peek: &[],
        answer: false,
        snapshot: true,
        width: 39,
        height: 24,
    });
    let (actual, _) = paint(&state, None, 39, 24);
    assert_drawn(&actual, &expected);

    let (expected, _) = expect_screen(Expect {
        rows: &[],
        queue: &[],
        profiles: &[],
        scope_note: None,
        link_note: None,
        peek: &[],
        answer: false,
        snapshot: true,
        width: 80,
        height: 9,
    });
    let (actual, _) = paint(&state, None, 80, 9);
    assert_drawn(&actual, &expected);
}

#[test]
fn untrusted_text_is_sanitized() {
    let hostile = "\u{1b}[31mhidden\u{202e}";
    let mut sample = row("fixture", "s-bad", hostile, RowState::Idle, None);
    sample.cost_usd = None;
    let profiles = listed(&["fixture"]);
    let state = hold(vec![sample.clone()], profiles.clone(), None, None);
    let (expected, _) = expect_screen(Expect {
        rows: &[sample],
        queue: &[],
        profiles: &profiles,
        scope_note: None,
        link_note: None,
        peek: &[],
        answer: false,
        snapshot: true,
        width: 80,
        height: 24,
    });
    let (actual, _) = paint(&state, None, 80, 24);
    let joined = actual.join("\n");
    assert!(
        joined.contains('\u{241b}'),
        "the escape was not drawn as a stand-in"
    );
    assert!(
        joined.contains("\u{2039}U+202E\u{203a}"),
        "the direction mark was not drawn as a stand-in"
    );
    assert!(!joined.contains('\u{1b}'));
    assert!(!joined.contains('\u{202e}'));
    assert_drawn(&actual, &expected);
}

fn view_of(sections: &[PlainTextSection]) -> LiveView<'_> {
    LiveView {
        sections: sections
            .iter()
            .map(|section| section as &dyn LiveSection)
            .collect(),
    }
}

#[test]
fn overlay_leaves_nothing_behind() {
    let sample = row("fixture", "s-one", "compile", RowState::Working, None);
    let state = hold(vec![sample], listed(&["fixture"]), None, None);
    let sink = ByteSink::default();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (80, 24),
        2,
        probe(false, false, Some((0, 23))),
        RestoreLink::detached(),
    )
    .expect("terminal");
    let mut vt = Vt::new(80, 24);
    vt.feed(&sink.take());
    let sections = live_view_sections(2);
    term.draw_live(&view_of(&sections)).expect("main screen");
    vt.absorb(&sink);
    let before = vt.rows();
    assert!(!before.join("\n").contains("Fleet \u{b7}"));

    term.open_overlay().expect("open overlay");
    vt.absorb(&sink);
    term.draw_overlay(|area: Rect, buf: &mut Buffer| {
        FleetView::draw(&state, None, area, buf, &FleetTheme::default());
    })
    .expect("overlay frame");
    vt.absorb(&sink);
    let during = vt.rows().join("\n");
    assert!(
        during.contains("Fleet \u{b7}"),
        "the overlay did not show the fleet view"
    );

    term.close_overlay().expect("close overlay");
    vt.absorb(&sink);
    term.draw_live(&view_of(&sections))
        .expect("restored screen");
    vt.absorb(&sink);
    assert_eq!(
        vt.rows(),
        before,
        "the main screen changed while the overlay was open"
    );
}
