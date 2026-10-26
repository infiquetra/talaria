//! The fleet overlay with the pane action: the footer hint and the outcome
//! notices. Issue #246.
//!
//! The row builder mirrors the fleet view's own screens test row for row; the
//! footer hint `o new pane` and the notice line are this unit's additions.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use talaria_core::directory::rows::{RowKey, format_age};
use talaria_core::fleet::{FleetRow, Outcome, ProfileStatus, RowState};
use talaria_core::fleet_view::{FleetViewInput, FleetViewState, update};
use talaria_tui::fleet_view::{FleetTheme, FleetView};
use talaria_tui::history::sanitize;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const NOW: f64 = 1_792_700_000.0;
// The bound actions' hints in order, with this unit's hint last.
const FOOTER: &str = "enter open \u{b7} space peek \u{b7} r reply \u{b7} x stop \u{b7} v review \u{b7} / filter \u{b7} esc close \u{b7} o new pane";
const NOTICE_T2: &str = "Opened \"migrate\" in a new Herdr pane.";
const NOTICE_T3: &str = "Not inside Herdr or tmux. To watch \"migrate\" side by side, run this in another terminal: '/t/talaria' attach 'local-ab' --profile 'fixture'";

fn key(profile: &str, id: &str) -> RowKey {
    RowKey {
        profile: profile.to_owned(),
        session_id: id.to_owned(),
    }
}

fn row(profile: &str, id: &str, title: &str, state: RowState) -> FleetRow {
    FleetRow {
        key: key(profile, id),
        title: title.to_owned(),
        model: Some("ollama-cloud/minimax-m3.1-flash".to_owned()),
        source: Some("tui".to_owned()),
        started_at: Some(NOW - 60.0),
        message_count: 2,
        state,
        state_since: NOW - 20.0,
        outcome: None,
        foreground: false,
        subscribed: false,
        stale: false,
        gone: false,
        unlisted: false,
        activity: None,
        cost_usd: None,
    }
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

fn detail_text(row: &FleetRow) -> String {
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

fn format_row(
    row: &FleetRow,
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
    parts.push(fit(&detail_text(row), detail_width));
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

/// The expected screen: the body from the top, the notice above the footer.
/// The footer carries this unit's `o new pane` hint last.
fn expect_screen(
    rows: &[FleetRow],
    notice: Option<&str>,
    width: usize,
    height: usize,
) -> Vec<String> {
    let mut body = Vec::new();
    body.push(format!("Fleet \u{b7} {} sessions", rows.len()));
    let visible = ordered(rows);
    let show_profile = visible
        .iter()
        .map(|row| row.key.profile.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        > 1
        && width >= 80;
    let show_model = width >= 60;
    let show_cost = width >= 100;
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
            body.push(format_row(
                sample,
                &cursor,
                show_profile,
                show_model,
                show_cost,
                width,
            ));
        }
    }
    let mut lines = vec![String::new(); height];
    let room = height.saturating_sub(2);
    for (index, line) in body.into_iter().take(room).enumerate() {
        lines[index] = line.trim_end().to_owned();
    }
    if let Some(notice) = notice {
        lines[height - 2] = notice.trim_end().to_owned();
    }
    lines[height - 1] = fit(FOOTER, width).trim_end().to_owned();
    lines
}

fn paint(state: &FleetViewState, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| {
            FleetView::draw(
                state,
                None,
                frame.area(),
                frame.buffer_mut(),
                &FleetTheme::default(),
            );
        })
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

fn hold(rows: Vec<FleetRow>) -> FleetViewState {
    let mut state = FleetViewState::new(Some(key("fixture", "s-front")), NOW);
    update(
        &mut state,
        FleetViewInput::Snapshot {
            version: 1,
            rows,
            queue: Vec::new(),
            profiles: vec![(
                "fixture".to_owned(),
                ProfileStatus::Listed { truncated: false },
            )],
            scope_note: None,
            link_note: None,
        },
        8,
    );
    state
}

fn three_rows() -> Vec<FleetRow> {
    let mut foreground = row("fixture", "s-front", "compile", RowState::Working);
    foreground.foreground = true;
    let working = row("fixture", "s-work", "migrate", RowState::Working);
    let mut done = row("fixture", "s-done", "ship", RowState::Done);
    done.outcome = Some(Outcome::Completed);
    done.cost_usd = Some(1.25);
    vec![working, foreground, done]
}

#[test]
fn footer_hint_and_notices_snapshots() {
    let rows = three_rows();
    let mut state = hold(rows.clone());

    // Wide, without a notice: the footer already carries `o new pane`.
    let actual = paint(&state, 160, 48);
    let expected = expect_screen(&rows, None, 160, 48);
    assert_eq!(actual, expected);

    // Wide with the no-multiplexer notice: it fits the 160 columns whole.
    update(&mut state, FleetViewInput::Notice(NOTICE_T3.to_owned()), 8);
    let actual = paint(&state, 160, 48);
    let expected = expect_screen(&rows, Some(NOTICE_T3), 160, 48);
    assert_eq!(actual, expected);

    // Narrow with the opened notice.
    let mut state = hold(rows.clone());
    let actual = paint(&state, 80, 24);
    let expected = expect_screen(&rows, None, 80, 24);
    assert_eq!(actual, expected);
    update(&mut state, FleetViewInput::Notice(NOTICE_T2.to_owned()), 8);
    let actual = paint(&state, 80, 24);
    let expected = expect_screen(&rows, Some(NOTICE_T2), 80, 24);
    assert_eq!(actual, expected);
}

#[test]
fn pane_footer_uses_the_bound_key_and_omits_unbound_action() {
    use std::collections::BTreeMap;
    use talaria_tui::keys::KeyMap;
    let state = hold(three_rows());
    for (bindings, expected) in [
        (vec!["ctrl+o".to_owned()], Some("ctrl+o new pane")),
        (Vec::new(), None),
    ] {
        let (map, notices) = KeyMap::from_entries(&BTreeMap::from([(
            "fleet_open_in_pane".to_owned(),
            bindings,
        )]));
        assert!(notices.is_empty());
        let theme = FleetTheme::with_key_map(&map);
        let mut terminal = Terminal::new(TestBackend::new(160, 48)).expect("terminal");
        terminal
            .draw(|frame| FleetView::draw(&state, None, frame.area(), frame.buffer_mut(), &theme))
            .expect("draw");
        let footer = (0..160)
            .map(|x| terminal.backend().buffer()[(x, 47)].symbol())
            .collect::<String>();
        match expected {
            Some(text) => assert!(footer.contains(text), "{footer}"),
            None => assert!(!footer.contains("new pane"), "{footer}"),
        }
    }
}
