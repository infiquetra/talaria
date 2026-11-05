//! Fixed, offline view fixtures shared by the catalogue migration contracts.
#![allow(dead_code)]

#[path = "view_colours_legacy.rs"]
mod legacy;
#[allow(unused_imports)]
pub use legacy::{Fixture, fixtures, wide_profile_divider};

use ratatui::{buffer::Buffer, style::Color};
use serde::{Deserialize, Serialize};
use talaria_app::theme_file::{self, ThemeFile};
use talaria_tui::theme::{self, *};

pub fn inputs(depth: (ColorDepth, DepthSource)) -> ThemeInputs<'static> {
    ThemeInputs {
        skin: None,
        skipped: &[],
        follow_hermes_skin: true,
        polarity: (Polarity::Dark, PolaritySource::Config),
        depth,
        motion: (Motion::Reduced, MotionSource::Config),
        terminal: TerminalColors::default(),
    }
}

pub fn resolve(file: Option<&ThemeFile>) -> ThemeFileResolution {
    theme::resolve_with_theme_file(&inputs((ColorDepth::TrueColor, DepthSource::Config)), file)
}

pub fn warm_syntax() {
    talaria_tui::markdown::warm_up();
    let started = std::time::Instant::now();
    while !talaria_tui::markdown::highlight::is_loaded() {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(15),
            "syntax resources did not load"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

pub fn full_file() -> ThemeFile {
    theme_file::parse(include_str!(
        "../../../talaria-app/tests/data/themes/ember.toml"
    ))
    .unwrap()
}

pub fn text(buffer: &Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect()
        })
        .collect()
}

pub fn at_text<'a>(buffer: &'a Buffer, needle: &str) -> &'a ratatui::buffer::Cell {
    for (y, row) in text(buffer).iter().enumerate() {
        if let Some(index) = row.find(needle) {
            let x = unicode_width::UnicodeWidthStr::width(&row[..index]);
            return &buffer[(u16::try_from(x).unwrap(), u16::try_from(y).unwrap())];
        }
    }
    panic!("missing {needle:?} in {:?}", text(buffer));
}

/// The recordings include blank and continuation cells, not only visible text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellRecord(pub String, pub String, pub String, pub String, pub u16);

pub fn cells(buffer: &Buffer) -> Vec<CellRecord> {
    buffer
        .content
        .iter()
        .map(|cell| {
            CellRecord(
                cell.symbol().into(),
                format!("{:?}", cell.fg),
                format!("{:?}", cell.bg),
                format!("{:?}", cell.underline_color),
                cell.modifier.bits(),
            )
        })
        .collect()
}

/// Run-length encoding is lossless: each entry expands to whole consecutive cells.
#[derive(Debug, Serialize, Deserialize)]
struct Recording {
    width: u16,
    height: u16,
    runs: Vec<(usize, CellRecord)>,
}

pub fn assert_recording(name: &str, buffer: &Buffer) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots/view_colours")
        .join(format!("{name}.json"));
    let actual = cells(buffer);
    if std::env::var("TALARIA_RECORD_VIEW_COLOURS").as_deref() == Ok("1") {
        let mut runs: Vec<(usize, CellRecord)> = Vec::new();
        for cell in &actual {
            match runs.last_mut() {
                Some((count, previous)) if previous == cell => *count += 1,
                _ => runs.push((1, cell.clone())),
            }
        }
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let recording = Recording {
            width: buffer.area.width,
            height: buffer.area.height,
            runs,
        };
        let runs = recording
            .runs
            .iter()
            .map(|run| serde_json::to_string(run).unwrap())
            .collect::<Vec<_>>()
            .join(",\n    ");
        let json = format!(
            "{{\n  \"width\": {}, \"height\": {},\n  \"runs\": [\n    {}\n  ]\n}}\n",
            recording.width, recording.height, runs
        );
        std::fs::write(&path, json).unwrap();
    }
    let recorded: Recording = serde_json::from_str(
        &std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("recording {}: {error}", path.display())),
    )
    .unwrap();
    assert_eq!((recorded.width, recorded.height), (80, 24), "{name}");
    assert_eq!(
        buffer.area,
        ratatui::layout::Rect::new(0, 0, 80, 24),
        "{name}"
    );
    let expected: Vec<_> = recorded
        .runs
        .into_iter()
        .flat_map(|(count, cell)| std::iter::repeat_n(cell, count))
        .collect();
    assert_eq!(expected.len(), 80 * 24, "{name}: incomplete recording");
    for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(
            actual,
            expected,
            "{name}: cell ({}, {})",
            index % 80,
            index / 80
        );
    }
}

pub fn no_colour(buffer: &Buffer) -> Option<String> {
    buffer.content.iter().enumerate().find_map(|(index, cell)| {
        [cell.fg, cell.bg, cell.underline_color]
            .iter()
            .any(|colour| *colour != Color::Reset)
            .then(|| format!("cell ({}, {}): {cell:?}", index % 80, index / 80))
    })
}

pub fn is_chrome(symbol: &str) -> bool {
    symbol
        .chars()
        .any(|ch| ('\u{2500}'..='\u{257f}').contains(&ch))
}

/// Foreground samples are text chosen by the fixtures, never frame padding.
pub const PROBES: &[(&str, &str, &str)] = &[
    ("automation", "⊙ goal", "automation.accent"),
    (
        "background",
        "The background task failed",
        "background.error",
    ),
    (
        "background-processes",
        "Background processes",
        "background.text",
    ),
    ("capabilities", "Capabilities", "list.header"),
    ("checklist", "[x]", "work.task.done"),
    ("context", "Context ·", "context.header"),
    ("cron-list", "Scheduled jobs", "cron.heading"),
    ("cron-history", "Recent runs", "cron.accent"),
    ("cron-form", "Choose a unique name", "cron.error"),
    ("launch", "Could not launch", "launch.error"),
    ("markdown", "Heading", "transcript.heading.1"),
    ("mcp", "MCP servers", "dialog.title"),
    ("pager", "Transcript", "pager.header"),
    ("palette", "talaria", "palette.origin"),
    ("profiles", "▸ fixture", "profiles.warning"),
    ("picker", "Switch model", "picker.title"),
    ("providers", "Providers", "providers.title"),
    ("review-labels", "#1 open passing", "review.label.success"),
    ("side", "exit 3", "side.error"),
    ("status", "2 need you", "status.attention"),
    ("subagent-dock", "Subagents", "agents.header"),
    ("tool", "A synthetic failure", "tool.error"),
    ("workflow", "fixture-review", "workflow.text"),
    ("work-panel", "Work /", "work.title"),
    ("work-panel", "2 need you", "work.attention"),
    ("work-panel", "unknown", "work.verdict.pending"),
    ("work-panel", "Read the inputs", "work.task.text"),
    ("work-panel", "[x]", "work.task.done"),
    ("work-panel", "Inspect the diff", "work.agent.name"),
    ("menu", "Talaria", "menu.brand"),
    ("menu", "Session", "menu.item"),
    ("workspace", "No messages yet.", "workspace.empty"),
    ("rewind-actions", "Uncommitted edits", "rewind.warning"),
    (
        "rewind-actions",
        "Continue from this prompt",
        "rewind.accent",
    ),
];
