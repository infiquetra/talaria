//! The profiles screen, drawn from the layout rules. Issue #253.
//!
//! The expected screens below are built from section 6.2 of the specification
//! by independent helpers: header hints, list columns, the wide split, panels,
//! wrapped notices and the cursor. They share no code with the renderer.

mod support;

use support::{ByteSink, Vt, live_view_sections};
use talaria_core::profiles::{
    DetailView, FormField, FormView, Panel, ProfilesView, RowTone, RowView,
};
use talaria_tui::history::sanitize;
use talaria_tui::live::{LiveSection, LiveView, PlainTextSection};
use talaria_tui::profiles::draw;
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

/// A terminal of the given size with a two-row viewport, its sink and a fed emulator.
fn open_terminal(cols: u16, rows: u16) -> (InlineTerminal<ByteSink>, ByteSink, Vt) {
    let sink = ByteSink::default();
    let term = InlineTerminal::new(
        sink.clone(),
        (cols, rows),
        2,
        support::probe(false, false, Some((0, rows - 1))),
        RestoreLink::detached(),
    )
    .expect("new terminal");
    let mut vt = Vt::new(cols, rows);
    vt.absorb(&sink);
    (term, sink, vt)
}

/// A live view that borrows the given sections.
fn view_of(sections: &[PlainTextSection]) -> LiveView<'_> {
    LiveView {
        sections: sections
            .iter()
            .map(|section| section as &dyn LiveSection)
            .collect(),
    }
}

fn s(text: &str) -> String {
    sanitize(text).into_owned()
}

fn chars(text: &str) -> usize {
    text.chars().count()
}

fn tail(text: &str, keep: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars[chars.len().saturating_sub(keep)..].iter().collect()
}

/// Cut to `width` characters, ending with `…` when cut.
fn cut(text: &str, width: usize) -> String {
    if chars(text) <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut out: String = text.chars().take(width - 1).collect();
    out.push('…');
    out
}

fn pad(text: &str, width: usize) -> String {
    let mut out = cut(text, width);
    while chars(&out) < width {
        out.push(' ');
    }
    out
}

/// Greedy word wrap; a word longer than the width is broken.
fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let mut rest = word;
        while chars(rest) > width {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            let at = rest
                .char_indices()
                .map(|(i, _)| i)
                .nth(width)
                .unwrap_or(rest.len());
            lines.push(rest[..at].to_owned());
            rest = &rest[at..];
        }
        if current.is_empty() {
            current.push_str(rest);
        } else if chars(&current) + 1 + chars(rest) <= width {
            current.push(' ');
            current.push_str(rest);
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(rest);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn row(label: &str, tone: RowTone, model: &str, next: &str, name: &str) -> RowView {
    RowView {
        name_cell: name.to_owned(),
        label: label.to_owned(),
        tone,
        model_cell: model.to_owned(),
        next_step: next.to_owned(),
    }
}

fn main_view() -> ProfilesView {
    ProfilesView {
        loading: false,
        header: "Profiles (7 on disk · 3 served)".to_owned(),
        rows: vec![
            row(
                "served",
                RowTone::Muted,
                "minimax-m3.1-flash · ollama-cloud",
                "talaria --profile default",
                "default",
            ),
            row(
                "served",
                RowTone::Muted,
                "-",
                "talaria --profile fixture",
                "fixture",
            ),
            row(
                "served",
                RowTone::Muted,
                "-",
                "talaria --profile fixture-a",
                "fixture-a",
            ),
            row(
                "parked",
                RowTone::Warning,
                "-",
                "fix the reason, then rescan with R in /profiles",
                "fixture-b",
            ),
            row(
                "stopped",
                RowTone::Warning,
                "-",
                "hermes -p fixture-c gateway start",
                "fixture-c",
            ),
            row(
                "own gateway",
                RowTone::Warning,
                "-",
                "hermes -p fixture-d gateway stop, then rescan with R in /profiles",
                "fixture-d",
            ),
            row(
                "not discovered",
                RowTone::Warning,
                "-",
                "rescan with R in /profiles",
                "fixture-e",
            ),
        ],
        selected: Some(0),
        filter: None,
        panel: Panel::Detail(DetailView {
            title: "default · served".to_owned(),
            state_text:
                "Served by this gateway. Start a session on it with: talaria --profile default"
                    .to_owned(),
            lines: vec![
                "Model: ollama-cloud · minimax-m3.1-flash".to_owned(),
                "Skills: 1 of 1 on".to_owned(),
                "Toolsets: Terminal".to_owned(),
                "MCP servers: files (off, stdio)".to_owned(),
                "Description: Throwaway profile".to_owned(),
                "Soul:".to_owned(),
                "You are a careful reviewer.".to_owned(),
            ],
        }),
        notice: None,
        confirm: None,
    }
}

fn name_width(rows: &[RowView]) -> usize {
    rows.iter()
        .map(|row| chars(&s(&row.name_cell)))
        .max()
        .unwrap_or(0)
        .min(24)
}

fn model_width(rows: &[RowView]) -> usize {
    rows.iter()
        .map(|row| chars(&s(&row.model_cell)))
        .max()
        .unwrap_or(0)
        .min(34)
}

fn row_line(row: &RowView, selected: bool, name_w: usize, model_w: usize, cols: usize) -> String {
    let marker = if selected { "▸ " } else { "  " };
    let name = pad(&s(&row.name_cell), name_w);
    let label = pad(&s(&row.label), 14);
    let mut line = format!("{marker}{name}  {label}");
    if cols >= 40 {
        if cols >= 80 {
            line.push_str("  ");
            line.push_str(&pad(&s(&row.model_cell), model_w));
        }
        let rest = cols.saturating_sub(chars(&line) + 2);
        line.push_str("  ");
        line.push_str(&cut(&s(&row.next_step), rest));
    }
    line
}

fn header_line(view: &ProfilesView, cols: usize) -> String {
    let left = s(&view.header);
    let right = if let Some(filter) = &view.filter {
        let text = s(&format!("/{filter}"));
        let max = cols.saturating_sub(chars(&left) + 2);
        if chars(&text) > max {
            format!("…{}", tail(&text, max.saturating_sub(1)))
        } else {
            text
        }
    } else if cols < 40 {
        String::new()
    } else {
        let hints = ["r refresh", "R rescan", "n new", "e describe", "s soul"];
        let mut keep = hints.len();
        loop {
            let text = hints[..keep].join(" · ");
            if chars(&left) + 2 + chars(&text) <= cols || keep == 0 {
                break text;
            }
            keep -= 1;
        }
    };
    if right.is_empty() {
        return left;
    }
    let gap = cols - chars(&left) - chars(&right);
    format!("{left}{}{right}", " ".repeat(gap))
}

fn paint_list(
    screen: &mut [String],
    top: usize,
    height: usize,
    view: &ProfilesView,
    cols: usize,
    covered: usize,
) {
    let name_w = name_width(&view.rows);
    let model_w = model_width(&view.rows);
    let avail = height.saturating_sub(covered);
    let mut start = 0;
    if let Some(selected) = view.selected
        && selected >= start + avail
        && avail > 0
    {
        start = selected + 1 - avail;
    }
    for (i, row) in view.rows.iter().skip(start).take(avail).enumerate() {
        let selected = Some(start + i) == view.selected;
        screen[top + i] = row_line(row, selected, name_w, model_w, cols);
    }
}

fn paint_detail(screen: &mut [String], top: usize, detail: &DetailView, cols: usize) {
    let mut row = top;
    let put = |screen: &mut [String], row: &mut usize, text: String| {
        if *row < screen.len() {
            screen[*row] = cut(&text, cols);
            *row += 1;
        }
    };
    put(screen, &mut row, s(&detail.title));
    for line in wrap_words(&s(&detail.state_text), cols) {
        put(screen, &mut row, line);
    }
    for line in &detail.lines {
        put(screen, &mut row, s(line));
    }
}

/// Paint a panel; returns the cursor the panel asks for.
fn paint_panel(
    screen: &mut [String],
    top: usize,
    panel: &Panel,
    cols: usize,
) -> Option<(u16, u16)> {
    let mut put = |row: usize, text: String| {
        if row < screen.len() {
            screen[row] = cut(&text, cols);
        }
    };
    match panel {
        Panel::Form(form) => {
            put(top, "New profile".to_owned());
            let mut row = top + 1;
            let field = |focused: bool, text: String| {
                format!("{}{text}", if focused { "▸ " } else { "  " })
            };
            let mut cursor = None;
            put(
                row,
                field(
                    form.focus == FormField::Name,
                    format!("Name: {}", s(&form.name)),
                ),
            );
            if form.focus == FormField::Name {
                cursor = Some((8 + form.name_cursor as u16, row as u16));
            }
            row += 1;
            if let Some(error) = &form.name_error {
                put(row, format!("    {}", s(error)));
                row += 1;
            }
            put(
                row,
                field(
                    form.focus == FormField::Description,
                    format!("Description: {}", s(&form.description)),
                ),
            );
            if form.focus == FormField::Description {
                cursor = Some((8 + form.description_cursor as u16, row as u16));
            }
            row += 1;
            let choice = form
                .start_from
                .get(form.start_index)
                .cloned()
                .unwrap_or_default();
            put(
                row,
                field(
                    form.focus == FormField::StartFrom,
                    format!("Start from: ‹ {} ›", s(&choice)),
                ),
            );
            row += 1;
            put(
                row,
                field(
                    form.focus == FormField::CopyKeys,
                    format!(
                        "[{}] Copy provider keys from {}",
                        if form.copy_keys { "x" } else { " " },
                        s(&form.copy_from)
                    ),
                ),
            );
            row += 1;
            put(row, "Enter next · Tab move · Esc cancel".to_owned());
            cursor
        }
        Panel::Review(lines) => {
            put(top, "Review".to_owned());
            for (i, line) in lines.iter().enumerate() {
                put(top + 1 + i, s(line));
            }
            put(top + 1 + lines.len(), "Enter apply · Esc back".to_owned());
            None
        }
        Panel::DescriptionInput {
            name,
            text,
            cursor: at,
            error,
        } => {
            put(top, format!("Description of {}", s(name)));
            put(top + 1, format!("> {}", s(text)));
            let mut row = top + 2;
            if let Some(error) = error {
                put(row, s(error));
                row += 1;
            }
            put(row, "Enter review · Esc cancel".to_owned());
            Some((2 + *at as u16, (top + 1) as u16))
        }
        _ => None,
    }
}

fn notice_block(view: &ProfilesView, cols: usize, rows: usize) -> Vec<String> {
    let Some(text) = view.confirm.as_ref().or(view.notice.as_ref()) else {
        return Vec::new();
    };
    let mut wrapped = wrap_words(&s(text), cols);
    let max = rows.saturating_sub(2).max(1);
    if wrapped.len() > max {
        wrapped.truncate(max);
        let last = wrapped.len() - 1;
        let shortened = cut(&wrapped[last], cols.saturating_sub(1));
        wrapped[last] = format!("{shortened}…");
    }
    wrapped
}

fn expected(view: &ProfilesView, cols: usize, rows: usize) -> (Vec<String>, Option<(u16, u16)>) {
    let mut screen = vec![String::new(); rows];
    let wide = cols >= 100;
    let notice = notice_block(view, cols, rows);
    let mut cursor = None;
    if let Panel::NarrowDetail(detail) = &view.panel {
        paint_detail(&mut screen, 0, detail, cols);
    } else {
        screen[0] = header_line(view, cols);
        if view.loading {
            screen[1] = "Loading profiles...".to_owned();
        } else if matches!(
            view.panel,
            Panel::Form(_) | Panel::Review(_) | Panel::DescriptionInput { .. }
        ) {
            if wide {
                let count = view.rows.len();
                let list_len = (count.min((rows - 2) / 2)).max(1);
                paint_list(&mut screen, 1, list_len, view, cols, 0);
                screen[1 + list_len] = "─".repeat(cols);
                cursor = paint_panel(&mut screen, 2 + list_len, &view.panel, cols);
            } else {
                cursor = paint_panel(&mut screen, 1, &view.panel, cols);
            }
        } else {
            if view.filter.is_some() && view.rows.is_empty() {
                let text = view.filter.as_deref().unwrap_or_default();
                screen[1] = format!("No profile matches \"{}\".", s(text));
            } else if wide {
                let count = view.rows.len();
                let list_len = (count.min((rows - 2) / 2)).max(1);
                paint_list(&mut screen, 1, list_len, view, cols, 0);
                screen[1 + list_len] = "─".repeat(cols);
                if let Panel::Detail(detail) = &view.panel {
                    paint_detail(&mut screen, 2 + list_len, detail, cols);
                }
            } else {
                paint_list(&mut screen, 1, rows - 1, view, cols, notice.len());
            }
        }
    }
    if !notice.is_empty() {
        let start = rows - notice.len();
        for (i, line) in notice.iter().enumerate() {
            screen[start + i] = line.clone();
        }
    }
    for line in &mut screen {
        *line = line.trim_end().to_owned();
    }
    (screen, cursor)
}

/// What the emulator shows: rows, the returned cursor, the vt cursor, hidden or not.
type Drawn = (Vec<String>, Option<(u16, u16)>, (u16, u16), bool);

fn drawn(view: &ProfilesView, cols: u16, rows: u16) -> Drawn {
    let (mut term, sink, mut vt) = open_terminal(cols, rows);
    term.open_overlay().expect("open");
    vt.absorb(&sink);
    let mut pos = None;
    term.draw_overlay_with_cursor(|area, buf| {
        pos = draw(view, area, buf);
        pos
    })
    .expect("draw");
    vt.absorb(&sink);
    (vt.rows(), pos, vt.cursor(), vt.cursor_hidden())
}

fn check(view: &ProfilesView, cols: u16, rows: u16) {
    let (want, want_cursor) = expected(view, cols as usize, rows as usize);
    let (got, got_pos, vt_cursor, vt_hidden) = drawn(view, cols, rows);
    assert_eq!(got.len(), want.len());
    for (i, (got_line, want_line)) in got.iter().zip(want.iter()).enumerate() {
        assert_eq!(got_line, want_line, "row {i} at {cols}x{rows}");
    }
    assert_eq!(got_pos, want_cursor, "the returned cursor at {cols}x{rows}");
    match want_cursor {
        Some((col, row)) => {
            assert!(!vt_hidden, "the cursor shows at {cols}x{rows}");
            assert_eq!(vt_cursor, (col, row));
        }
        None => assert!(vt_hidden, "the cursor hides at {cols}x{rows}"),
    }
}

fn form_view() -> ProfilesView {
    let mut view = main_view();
    view.selected = Some(3);
    view.panel = Panel::Form(FormView {
        name: "Bad Name!".to_owned(),
        name_cursor: 9,
        description: String::new(),
        description_cursor: 0,
        start_from: vec!["fresh".to_owned(), "default".to_owned()],
        start_index: 0,
        copy_keys: true,
        copy_from: "default".to_owned(),
        focus: FormField::Name,
        name_error: Some(
            "Use lower-case letters, digits, - or _, starting with a letter or digit, up to 64 characters.".to_owned(),
        ),
    });
    view
}

#[test]
fn snapshots_at_four_sizes() {
    let view = main_view();
    check(&view, 160, 48);
    check(&view, 100, 30);
    check(&view, 80, 24);
    check(&view, 38, 20);

    // Panels and states at a wide and a narrow size.
    let form = form_view();
    check(&form, 100, 30);
    check(&form, 38, 20);

    let mut review = main_view();
    review.panel = Panel::Review(vec![
        "name: fixture-new".to_owned(),
        "description: \"Scratch work\"".to_owned(),
        "start from: copy of fixture".to_owned(),
        "copy provider keys from default: off".to_owned(),
        "Hermes also adds a shell command named fixture-new when that name is free.".to_owned(),
    ]);
    check(&review, 100, 30);
    check(&review, 38, 20);

    let mut input = main_view();
    input.panel = Panel::DescriptionInput {
        name: "fixture".to_owned(),
        text: "plant food".to_owned(),
        cursor: 10,
        error: None,
    };
    check(&input, 100, 30);
    check(&input, 38, 20);

    let mut filtered = main_view();
    filtered.rows.clear();
    filtered.selected = None;
    filtered.filter = Some("zzz".to_owned());
    filtered.panel = Panel::None;
    check(&filtered, 100, 30);
    check(&filtered, 38, 20);

    let mut confirm = main_view();
    confirm.confirm = Some(
        "Ask the gateway to rescan profiles now? It starts serving new profiles and retries parked ones, including their messaging bots. Press y to confirm, any other key to cancel.".to_owned(),
    );
    check(&confirm, 100, 30);
    check(&confirm, 38, 20);

    let mut narrow_detail = main_view();
    narrow_detail.panel = match main_view().panel {
        Panel::Detail(detail) => Panel::NarrowDetail(detail),
        other => other,
    };
    check(&narrow_detail, 38, 20);

    let mut loading = main_view();
    loading.loading = true;
    loading.rows.clear();
    loading.selected = None;
    loading.panel = Panel::None;
    loading.header = "Profiles (- on disk · 0 served)".to_owned();
    check(&loading, 100, 30);
}

#[test]
fn hostile_text_cannot_break_the_screen() {
    let mut view = main_view();
    view.rows[3].name_cell = "evil \x1b[2J goal".to_owned();
    view.rows[3].next_step = "end \u{202e} start".to_owned();
    view.rows[4].name_cell = "tab\there".to_owned();
    if let Panel::Detail(detail) = &mut view.panel {
        detail.state_text = "line one\nline two\r\nline three".to_owned();
        detail.lines = vec!["Soul: \x07 bell".to_owned()];
    }
    view.notice = Some("note \x1b]8;;http://example.invalid\x07 link".to_owned());
    let (want, want_cursor) = expected(&view, 100, 30);
    let (got, got_pos, _, hidden) = drawn(&view, 100, 30);
    assert_eq!(got.len(), want.len());
    for (i, (got_line, want_line)) in got.iter().zip(want.iter()).enumerate() {
        assert_eq!(got_line, want_line, "row {i}");
        assert!(!got_line.contains('\x1b'), "no escape survives on row {i}");
        assert!(!got_line.contains('\u{7}'), "no bell survives on row {i}");
    }
    assert_eq!(got_pos, want_cursor);
    assert!(hidden);
    let screen = got.join("\n");
    assert!(screen.contains('␛'), "the escape shows as a picture");
    assert!(screen.contains("‹U+202E›"), "the bidi mark shows as a code");
    assert!(screen.contains('⏊'), "the newline shows as a picture");
}

#[test]
fn keystroke_budget_holds_for_two_hundred_rows() {
    #[cfg(debug_assertions)]
    {
        println!("SKIPPED (debug build)");
    }
    #[cfg(not(debug_assertions))]
    {
        use std::time::{Duration, Instant};
        use talaria_core::profiles::{
            GatewayView, HostTopology, KeyAction, ProfilesInput, ProfilesState, RosterEntry,
        };

        let roster: Vec<RosterEntry> = (0..200)
            .map(|i| RosterEntry {
                name: format!("fixture-{i:03}"),
                display_name: format!("fixture-{i:03}"),
                is_default: false,
                model: String::new(),
                provider: String::new(),
                description: String::new(),
                skill_count: 0,
            })
            .collect();
        let mut machine = ProfilesState::new("default".to_owned());
        let _ = machine.update(ProfilesInput::Open, 0);
        let _ = machine.update(ProfilesInput::Discovered(0, Ok(GatewayView::default())), 0);
        let _ = machine.update(ProfilesInput::Listed(0, Ok(roster)), 0);
        let _ = machine.update(
            ProfilesInput::Topology(
                0,
                Ok(HostTopology {
                    stopped: Vec::new(),
                    own_gateway: Some(Vec::new()),
                    gateway_mode: Some("multiplex".to_owned()),
                    single_reason: None,
                }),
            ),
            0,
        );
        let area = ratatui::layout::Rect::new(0, 0, 100, 30);
        // Best of three rounds; each round times 200 selection moves.
        let mut best_p95 = Duration::MAX;
        let mut best_worst = Duration::MAX;
        for _ in 0..3 {
            let mut samples = Vec::with_capacity(200);
            for _ in 0..200 {
                let start = Instant::now();
                let _ = machine.update(ProfilesInput::Key(KeyAction::Down), 0);
                let view = machine.view();
                let mut buf = ratatui::buffer::Buffer::empty(area);
                let _ = draw(&view, area, &mut buf);
                samples.push(start.elapsed());
            }
            samples.sort();
            let p95 = samples[189];
            let worst = samples[199];
            best_p95 = best_p95.min(p95);
            best_worst = best_worst.min(worst);
        }
        assert!(
            best_p95 < Duration::from_millis(16),
            "p95 {best_p95:?} exceeds a 60 Hz frame"
        );
        assert!(
            best_worst < Duration::from_millis(50),
            "worst {best_worst:?} exceeds the allowance"
        );
    }
}

#[test]
fn closing_restores_the_inline_view() {
    const ROWS: u16 = 24;
    const COLS: u16 = 100;
    let (mut term, sink, mut vt) = open_terminal(COLS, ROWS);
    let sections = live_view_sections(2);
    term.draw_live(&view_of(&sections)).expect("live");
    vt.absorb(&sink);
    let before = vt.rows();

    term.open_overlay().expect("open");
    vt.absorb(&sink);
    let view = main_view();
    let mut pos = None;
    term.draw_overlay_with_cursor(|area, buf| {
        pos = draw(&view, area, buf);
        pos
    })
    .expect("draw");
    vt.absorb(&sink);
    assert!(vt.alternate());
    assert_ne!(vt.rows(), before, "the overlay covers the live view");

    term.close_overlay().expect("close");
    vt.absorb(&sink);
    assert!(!vt.alternate());
    assert_eq!(vt.rows(), before, "closing restores every row");
}
