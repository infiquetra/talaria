//! Slash palette pictures. Issue #223. The draw stub leaves the buffer empty.

mod support;

use std::path::PathBuf;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};
use support::Alac;
use talaria_core::commands::{
    CommandSource, PaletteRow, PaletteStage, PaletteState, RowKind, SkillOrigin,
};
use talaria_tui::live::LiveSection;
use talaria_tui::palette::PaletteSection;

fn command(
    name: &str,
    description: &str,
    source: CommandSource,
    takes_argument: bool,
) -> PaletteRow {
    PaletteRow {
        kind: RowKind::Command,
        name: name.into(),
        description: description.into(),
        source: Some(source),
        refused: false,
        takes_argument,
        group: None,
    }
}

fn header(name: &str) -> PaletteRow {
    PaletteRow {
        kind: RowKind::Header,
        name: name.into(),
        description: String::new(),
        source: None,
        refused: false,
        takes_argument: false,
        group: None,
    }
}

fn status(text: &str) -> PaletteRow {
    PaletteRow {
        kind: RowKind::Status,
        name: text.into(),
        description: String::new(),
        source: None,
        refused: false,
        takes_argument: false,
        group: None,
    }
}

fn hint(text: &str) -> PaletteRow {
    PaletteRow {
        kind: RowKind::Hint,
        name: text.into(),
        description: String::new(),
        source: None,
        refused: false,
        takes_argument: false,
        group: None,
    }
}

fn hermes(name: &str, description: &str, takes_argument: bool) -> PaletteRow {
    command(
        name,
        description,
        CommandSource::Hermes {
            category: "Info".into(),
        },
        takes_argument,
    )
}

fn browse_rows() -> Vec<PaletteRow> {
    vec![
        header("Talaria"),
        command(
            "/quit",
            "Leave Talaria; the session keeps running in Hermes",
            CommandSource::Talaria,
            false,
        ),
        header("Session"),
        hermes("/new", "Start a new session (usage: /new [name])", true),
        hermes("/branch", "Show the git branch (usage: /branch)", false),
        hermes("/resume", "Resume a session (usage: /resume <id>)", true),
        hermes("/title", "Set the title (usage: /title <text>)", true),
        header("Info"),
        hermes(
            "/help",
            "Show available commands (usage: /help [filter])",
            true,
        ),
        hermes("/version", "Show the Hermes version", false),
        hermes("/plan", "Plan a task (usage: /plan <task>)", true),
        hermes("/usage", "Show token usage", false),
        header("Skills"),
        command(
            "/fixture-skill-1",
            "Fixture skill 1",
            CommandSource::Skill {
                origin: SkillOrigin::Local,
            },
            true,
        ),
        command(
            "/fixture-skill-2",
            "Fixture skill 2",
            CommandSource::Skill {
                origin: SkillOrigin::Hub,
            },
            true,
        ),
        command(
            "/fixture-user-1",
            "Fixture user command 1",
            CommandSource::UserCommand,
            false,
        ),
    ]
}

fn state_with(rows: Vec<PaletteRow>, selected: usize, total: usize) -> PaletteState {
    PaletteState {
        open: true,
        rows,
        selected,
        total,
        ..PaletteState::closed()
    }
}

fn browse(selected: usize) -> PaletteState {
    let rows = browse_rows();
    state_with(rows, selected, 12)
}

fn tenth_selectable(rows: &[PaletteRow]) -> usize {
    let mut seen = 0;
    for (index, row) in rows.iter().enumerate() {
        if row.kind == RowKind::Command {
            seen += 1;
            if seen == 10 {
                return index;
            }
        }
    }
    panic!("fewer than 10 commands");
}

fn query_rows() -> Vec<PaletteRow> {
    vec![
        hermes("/plan", "Plan a task (usage: /plan <task>)", true),
        hermes(
            "/plan-fast",
            "A faster plan (usage: /plan-fast <task>)",
            true,
        ),
        command(
            "/fixture-skill-1",
            "Fixture skill 1",
            CommandSource::Skill {
                origin: SkillOrigin::Local,
            },
            true,
        ),
    ]
}

fn picture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots/palette")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing snapshot {name}"))
}

fn draw(state: &PaletteState, width: u16, height: u16, terminal_rows: u16) -> (String, Buffer) {
    let section = PaletteSection::new(state, terminal_rows);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    terminal
        .draw(|frame| section.render(frame.area(), frame.buffer_mut()))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let rows: Vec<String> = (0..height)
        .map(|y| {
            let mut line: String = (0..width).map(|x| buffer[(x, y)].symbol()).collect();
            while line.ends_with(' ') {
                line.pop();
            }
            line
        })
        .collect();
    (format!("{}\n", rows.join("\n")), buffer)
}

fn expect_picture(
    name: &str,
    state: &PaletteState,
    width: u16,
    height: u16,
    terminal_rows: u16,
) -> Buffer {
    let (text, buffer) = draw(state, width, height, terminal_rows);
    assert_eq!(
        text.trim_end_matches('\n'),
        picture(name).trim_end_matches('\n'),
        "{name}"
    );
    buffer
}

fn row_text(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn row_starting(buffer: &Buffer, prefix: &str) -> u16 {
    (0..buffer.area.height)
        .find(|y| row_text(buffer, *y).starts_with(prefix))
        .unwrap_or_else(|| panic!("no row starting {prefix:?}"))
}

fn assert_reversed(buffer: &Buffer, y: u16) {
    let mut saw = false;
    for x in 0..buffer.area.width {
        let cell = &buffer[(x, y)];
        if cell.symbol() == " " {
            continue;
        }
        saw = true;
        assert!(cell.modifier.contains(Modifier::REVERSED), "column {x}");
    }
    assert!(saw);
}

fn assert_bold_dim(buffer: &Buffer, y: u16) {
    let cell = &buffer[(2, y)];
    assert!(cell.modifier.contains(Modifier::BOLD));
    assert!(cell.modifier.contains(Modifier::DIM));
}

fn assert_cyan_tag(buffer: &Buffer, y: u16) {
    let line = row_text(buffer, y);
    let start = line.find("talaria").expect("talaria tag");
    let cell = &buffer[(u16::try_from(start).unwrap_or(0), y)];
    assert_eq!(cell.fg, Color::Cyan);
}

#[test]
fn browse_snapshot_100_columns() {
    let top = browse(1);
    let buffer = expect_picture("browse_100.txt", &top, 100, 30, 30);
    assert_reversed(&buffer, row_starting(&buffer, "  /quit"));
    assert_bold_dim(&buffer, row_starting(&buffer, "  Talaria"));
    assert_cyan_tag(&buffer, row_starting(&buffer, "  /quit"));

    let rows = browse_rows();
    let selected = tenth_selectable(&rows);
    let mut scrolled = state_with(rows, selected, 12);
    scrolled.explicit_selection = true;
    let buffer = expect_picture("browse_100_scrolled.txt", &scrolled, 100, 30, 30);
    assert_reversed(&buffer, row_starting(&buffer, "  /fixture-skill-1"));
}

#[test]
fn query_snapshot_60_and_45_columns() {
    let state = state_with(query_rows(), 0, 12);
    let wide = expect_picture("query_60.txt", &state, 60, 24, 24);
    assert_reversed(&wide, row_starting(&wide, "  /plan"));
    assert!(!row_text(&wide, 0).contains("hermes"));

    let narrow = expect_picture("query_45.txt", &state, 45, 24, 24);
    assert_reversed(&narrow, row_starting(&narrow, "  /plan"));
    assert!(!row_text(&narrow, 0).contains("Plan a task"));
}

#[test]
fn browse_snapshot_160_columns() {
    let buffer = expect_picture("browse_160.txt", &browse(1), 160, 30, 30);
    assert_reversed(&buffer, row_starting(&buffer, "  /quit"));
    assert_cyan_tag(&buffer, row_starting(&buffer, "  /quit"));
    assert_bold_dim(&buffer, row_starting(&buffer, "  Session"));
    let skill = row_starting(&buffer, "  /help");
    let cell = &buffer[(2, skill)];
    assert!(cell.modifier.contains(Modifier::DIM) || !row_text(&buffer, skill).contains("skill"));
}

#[test]
fn status_rows_snapshots() {
    status_loading_failed_and_warnings();
    status_plugin_none_and_refused();
    status_argument_narrow_and_short();
}

fn quit_row() -> PaletteRow {
    command(
        "/quit",
        "Leave Talaria; the session keeps running in Hermes",
        CommandSource::Talaria,
        false,
    )
}

fn status_loading_failed_and_warnings() {
    let quit = quit_row();
    let mut loading = state_with(
        vec![
            header("Talaria"),
            quit_row(),
            header("Loading Hermes commands…"),
        ],
        1,
        1,
    );
    let buffer = expect_picture("status_loading.txt", &loading, 80, 24, 24);
    assert_bold_dim(&buffer, row_starting(&buffer, "  Loading"));

    loading
        .rows
        .push(status("Hermes commands unavailable: gateway timed out"));
    loading
        .rows
        .retain(|row| row.name != "Loading Hermes commands…");
    let buffer = expect_picture("status_failed.txt", &loading, 80, 24, 24);
    let failed = row_starting(&buffer, "  Hermes commands unavailable");
    assert!(buffer[(2, failed)].modifier.contains(Modifier::DIM));

    let mut warning = state_with(vec![header("Talaria"), quit], 1, 1);
    warning.footer = Some("Hermes: fixture warning".into());
    let buffer = expect_picture("status_warning.txt", &warning, 80, 24, 24);
    let footer = row_starting(&buffer, "Hermes:");
    assert!(buffer[(0, footer)].modifier.contains(Modifier::DIM));

    warning.footer = Some("completion unavailable: gateway timed out".into());
    let buffer = expect_picture("status_completion.txt", &warning, 80, 24, 24);
    assert!(
        row_text(&buffer, row_starting(&buffer, "completion unavailable"))
            .contains("gateway timed out")
    );
}

fn status_plugin_none_and_refused() {
    let plugin = state_with(
        vec![
            header("Talaria"),
            quit_row(),
            status("Plugin commands unknown: fixture warning"),
        ],
        1,
        1,
    );
    let buffer = expect_picture("status_plugin.txt", &plugin, 80, 24, 24);
    assert!(
        buffer[(2, row_starting(&buffer, "  Plugin"))]
            .modifier
            .contains(Modifier::DIM)
    );

    let none = state_with(
        vec![status(
            "No matching commands. Enter sends /zzz to Hermes as typed.",
        )],
        0,
        12,
    );
    let buffer = expect_picture("status_none.txt", &none, 80, 24, 24);
    assert!(buffer[(2, 0)].modifier.contains(Modifier::DIM));

    let mut refused = command(
        "/worktree",
        "Open a fresh worktree",
        CommandSource::Hermes {
            category: "Session".into(),
        },
        false,
    );
    refused.refused = true;
    let buffer = expect_picture(
        "status_refused.txt",
        &state_with(vec![refused], 0, 12),
        80,
        24,
        24,
    );
    assert!(buffer[(2, 0)].modifier.contains(Modifier::DIM));
}

fn status_argument_narrow_and_short() {
    let argument = PaletteState {
        open: true,
        stage: PaletteStage::Argument {
            replace_from_bytes: Some(6),
        },
        rows: vec![hint("usage: /plan <task>")],
        total: 12,
        ..PaletteState::closed()
    };
    let buffer = expect_picture("status_argument.txt", &argument, 80, 24, 24);
    assert!(buffer[(2, 0)].modifier.contains(Modifier::DIM));

    let query = state_with(query_rows(), 0, 12);
    let _narrow = expect_picture("status_narrow.txt", &query, 45, 24, 24);
    let _short = expect_picture("status_short.txt", &browse(1), 80, 7, 7);
}

#[test]
fn closing_palette_leaves_nothing_in_scrollback() {
    let open = browse(1);
    let section = PaletteSection::new(&open, 24);
    assert!(
        section.height(80) >= 3,
        "an open palette is at least three rows"
    );
    let closed = PaletteSection::new(&PaletteState::closed(), 24);
    assert_eq!(closed.height(80), 0);
    let mut alac = Alac::with_shell_history(80, 24);
    alac.feed(b"committed cell\r\n");
    let rows = alac.history_and_screen();
    assert!(rows.iter().any(|row| row.contains("committed cell")));
    assert!(rows.iter().all(|row| !row.contains("Leave Talaria")));
    assert!(!alac.alternate());
}

#[test]
fn untrusted_description_rendered_literally() {
    let description = "clear \u{1b}[2J then \u{202e}end";
    let row = hermes("/help", description, false);
    let state = state_with(vec![row], 0, 1);
    let (text, buffer) = draw(&state, 80, 8, 24);
    assert!(text.contains("␛[2J"), "{text}");
    assert!(text.contains("‹U+202E›"), "{text}");
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let symbol = buffer[(x, y)].symbol();
            assert_ne!(symbol, "\u{1b}");
            assert_ne!(symbol, "\u{202e}");
        }
    }
}
