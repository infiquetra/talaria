//! Static picker alternatives rendered in memory; no application or transport is started.
use std::io::{self, Write};

use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Clear, Widget},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pattern {
    A,
    B,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Surface {
    Models,
    Providers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Browse,
    NoLevels,
    Empty,
    Error,
    Setup,
}

#[derive(Clone, Copy)]
struct Screen {
    pattern: Pattern,
    surface: Surface,
    state: State,
}

fn fit(text: &str, width: u16) -> String {
    if text.width() <= usize::from(width) {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for piece in text.graphemes(true) {
        if used + piece.width() + 1 > usize::from(width) {
            break;
        }
        out.push_str(piece);
        used += piece.width();
    }
    if width > 0 {
        out.push('…');
    }
    out
}

fn paint(buf: &mut Buffer, area: Rect, y: u16, text: &str, style: Style) {
    if y < area.height && area.width > 0 {
        buf.set_string(area.x, area.y + y, fit(text, area.width), style);
    }
}

fn row(buf: &mut Buffer, area: Rect, y: u16, name: &str, badge: &str, selected: bool) {
    let style = if selected {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    };
    let suffix = fit(badge, area.width / 2);
    let width = area.width.saturating_sub(suffix.width() as u16 + 2);
    let name = fit(name, width);
    let gap = usize::from(area.width).saturating_sub(name.width() + suffix.width());
    paint(
        buf,
        area,
        y,
        &format!("{name}{}{suffix}", " ".repeat(gap)),
        style,
    );
}

fn details(surface: Surface, state: State) -> Vec<&'static str> {
    match surface {
        Surface::Models if state == State::NoLevels => vec![
            "Selected: OpenAI / chat-fast",
            "Requested: high (session setting)",
            "Catalogue: reasoning not advertised",
            "Applied: unknown; setting retained",
            "No effort edit here. Model remains selectable.",
            "F2 full details / Esc back",
        ],
        Surface::Models => vec![
            "Selected: OpenAI / reasoner-large",
            "Requested: high (session setting)",
            "Support: unverified for this route",
            "Applied: unknown; not a provider receipt",
            "Context: unknown / Price: unknown",
            "F2 full details / Left,Right request effort",
        ],
        Surface::Providers if state == State::Error => vec![
            "Selected: OpenAI / needs setup",
            "Status: stale / offline",
            "Setup changes disabled until reconnect",
            "Ctrl+R retry; no automatic write repeat",
            "Provider identity remains available",
            "F2 details / Esc close",
        ],
        Surface::Providers => vec![
            "Selected: OpenAI / needs setup",
            "Method: API key (other methods when offered)",
            "1 Enter to open a masked key field",
            "2 Check key / 3 Save / read back status",
            "Unreachable? Confirm save unchecked separately.",
            "Enter set up / F2 details",
        ],
    }
}

type StudyRow = (String, String, bool);

fn fixture_rows(surface: Surface) -> Vec<StudyRow> {
    let mut rows: Vec<StudyRow> = match surface {
        Surface::Models => [
            ("Starred", "", true),
            ("  * Anthropic / reasoner-small", "effort ?", false),
            ("OpenAI / configured", "", true),
            (">   ● reasoner-large", "effort ?", false),
            ("    chat-fast", "not offered", false),
            ("    reasoner-lite", "effort ?", false),
        ]
        .into_iter()
        .map(|(a, b, c)| (a.into(), b.into(), c))
        .collect(),
        Surface::Providers => [
            ("Configured", "", true),
            ("    Ollama Cloud", "listed ready", false),
            ("    Anthropic", "listed ready", false),
            ("    OpenAI Codex", "listed ready", false),
            ("Needs setup", "", true),
            (">   OpenAI", "no key", false),
            ("    DeepSeek", "no key", false),
            ("    Gemini", "sign in", false),
        ]
        .into_iter()
        .map(|(a, b, c)| (a.into(), b.into(), c))
        .collect(),
    };
    match surface {
        Surface::Models => {
            for n in 4..=22 {
                rows.push((
                    format!("    example-model-{n:02}"),
                    "effort ?".into(),
                    false,
                ));
            }
            rows.push(("OpenRouter / configured".into(), String::new(), true));
            for (name, badge) in [
                ("chat-plus", "unavailable"),
                (
                    "organization/team/reasoner-with-a-long-model-name",
                    "effort ?",
                ),
                ("multilingual-模型-extended", "effort ?"),
            ] {
                rows.push((format!("    {name}"), badge.into(), false));
            }
            for provider in 4..=19 {
                rows.push((
                    format!("Example provider {provider:02} / configured"),
                    String::new(),
                    true,
                ));
                let count = if provider <= 11 { 25 } else { 24 };
                for model in 1..=count {
                    rows.push((
                        format!("    example-{provider:02}-model-{model:02}"),
                        "effort ?".into(),
                        false,
                    ));
                }
            }
        }
        Surface::Providers => {
            for provider in 7..=32 {
                rows.push((
                    format!("    Example provider {provider:02}"),
                    "no key".into(),
                    false,
                ));
            }
            rows.push(("Custom endpoints".into(), String::new(), true));
            rows.push(("    Example endpoint".into(), "not tested".into(), false));
            rows.push(("    + Add an endpoint".into(), String::new(), false));
        }
    }
    rows
}

fn list(buf: &mut Buffer, area: Rect, screen: Screen) {
    let bold = Style::default().bold();
    let dim = Style::default().dim();
    if screen.state == State::Empty {
        paint(buf, area, 1, "No matches for \"zzzz\" in All.", bold);
        paint(
            buf,
            area,
            3,
            "Ctrl+U clears search. Tab changes scope.",
            dim,
        );
        paint(buf, area, 4, "Esc clears search, then closes.", dim);
        return;
    }
    let offset = if screen.state == State::Error {
        paint(
            buf,
            area,
            0,
            "! Could not reload. Showing stale results.",
            bold,
        );
        paint(
            buf,
            area,
            1,
            "Offline: changes disabled / Ctrl+R retry / Esc close",
            dim,
        );
        2
    } else {
        0
    };
    let mut rows = fixture_rows(screen.surface);
    let mut selected = match (screen.surface, screen.state) {
        (Surface::Models, State::NoLevels) => 4,
        (Surface::Models, _) => 3,
        (Surface::Providers, _) => 5,
    };
    if screen.pattern == Pattern::B && screen.surface == Surface::Models {
        rows = rows.into_iter().skip(2).take(23).collect();
        selected -= 2;
    }
    let visible = usize::from(area.height.saturating_sub(offset + 1));
    for (i, (name, badge, header)) in rows.iter().take(visible).enumerate() {
        if *header {
            paint(buf, area, offset + i as u16, name, bold);
        } else {
            let name = if i == selected && !name.starts_with('>') {
                format!("> {}", name.trim_start())
            } else if i != selected && name.starts_with('>') {
                name.replacen('>', " ", 1)
            } else {
                name.to_owned()
            };
            row(buf, area, offset + i as u16, &name, badge, i == selected);
        }
    }
    paint(
        buf,
        area,
        area.height.saturating_sub(1),
        &if rows.len() > visible {
            format!("More below / {} rows in view / PgDn scrolls", visible)
        } else {
            "End of list / headers are skipped".to_owned()
        },
        dim,
    );
}

fn detail(buf: &mut Buffer, area: Rect, screen: Screen, side: bool) {
    let borders = if side { Borders::LEFT } else { Borders::TOP };
    let block = Block::default().borders(borders).title("Details");
    let inner = block.inner(area);
    block.render(area, buf);
    if screen.state == State::Empty {
        paint(
            buf,
            inner,
            0,
            "No action until a valid row is selected.",
            Style::default().dim(),
        );
        return;
    }
    for (i, text) in details(screen.surface, screen.state).iter().enumerate() {
        paint(buf, inner, i as u16, text, Style::default());
    }
    if side {
        let extra = match screen.surface {
            Surface::Models => vec![
                "",
                "Full identity",
                "provider: openai",
                "model: reasoner-large",
                "",
                "Catalogue: reasoning advertised",
                "Exact levels: not supplied",
                "Evidence: catalogue only",
                "",
                "* starred / ● current",
                "Unavailable rows explain why on Enter.",
                "Unknown values stay unknown.",
            ],
            Surface::Providers => vec![
                "",
                "Setup methods",
                "[Key]  Account (if offered)",
                "Endpoint: separate form",
                "",
                "Listed ready means catalogue auth.",
                "It does not prove a key works now.",
                "",
                "No credential preview in this study.",
                "Removal always asks first.",
            ],
        };
        for (i, text) in extra.iter().enumerate() {
            paint(buf, inner, (i + 7) as u16, text, Style::default().dim());
        }
    }
}

fn draw(buf: &mut Buffer, screen: Screen) {
    let area = buf.area;
    Clear.render(area, buf);
    let title = match screen.surface {
        Surface::Models => "Models / this session / profile: example",
        Surface::Providers => "Providers / profile: example",
    };
    let block = Block::bordered().title(title);
    let inner = block.inner(area);
    block.render(area, buf);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(inner);
    let filter = if screen.state == State::Empty {
        "Filter: zzzz"
    } else {
        "Filter: type to search_"
    };
    paint(buf, chunks[0], 0, filter, Style::default());
    let chips = match (screen.pattern, screen.surface) {
        (Pattern::A, Surface::Models) => "[All]  Starred  Configured  Set up     Tab changes scope",
        (Pattern::A, Surface::Providers) => {
            "[All]  Keys  Accounts  Endpoints      Tab changes scope"
        }
        (Pattern::B, Surface::Models) => {
            "Provider rail / Ctrl+P focus rail / Tab return to results"
        }
        (Pattern::B, Surface::Providers) => {
            "Method rail / Ctrl+P focus rail / Tab return to results"
        }
    };
    paint(buf, chunks[0], 1, chips, Style::default());
    let count = match (screen.surface, screen.state) {
        (Surface::Models, State::Empty) => "0 matches / 418 loaded models (synthetic)",
        (Surface::Providers, State::Empty) => "0 matches / 32 providers (synthetic)",
        (Surface::Models, _) => "Synthetic: 418 models / 19 providers (cache-sized workload)",
        (Surface::Providers, _) => "Synthetic: 32 providers / illustrative states",
    };
    paint(buf, chunks[0], 2, count, Style::default().dim());
    let mut body = chunks[1];
    if screen.pattern == Pattern::B {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(19), Constraint::Min(1)])
            .split(body);
        let rail = match screen.surface {
            Surface::Models => vec![
                "Providers",
                "  All",
                "> OpenAI",
                "  Anthropic",
                "  OpenRouter",
                "  Example 04",
                "  +15 more",
            ],
            Surface::Providers => vec![
                "Methods",
                "> All",
                "  Keys",
                "  Accounts",
                "  Endpoints",
                "",
                "Status",
                "  Configured",
                "  Set up",
            ],
        };
        for (i, text) in rail.iter().enumerate() {
            paint(buf, split[0], i as u16, text, Style::default());
        }
        Block::default()
            .borders(Borders::RIGHT)
            .render(split[0], buf);
        body = split[1];
    }
    let wide = area.width >= 112 && area.height >= 28;
    let parts = if wide {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(body)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(7)])
            .split(body)
    };
    list(buf, parts[0], screen);
    detail(buf, parts[1], screen, wide);
    let hints = if screen.state == State::Empty {
        [
            "No selection / Enter disabled / Esc clear,close",
            "Ctrl+U clear / Tab scope / Ctrl+R reload",
        ]
    } else if screen.state == State::Error {
        [
            "Offline / changes disabled / Ctrl+R retry / Esc close",
            "F2 details / Ctrl+U clear / Tab scope",
        ]
    } else {
        match screen.surface {
            Surface::Models => [
                "Up,Down move / Enter choose / Esc clear,close / F2 details",
                "Ctrl+F star / Ctrl+G save scope / Ctrl+R reload / Ctrl+U clear",
            ],
            Surface::Providers => [
                "Up,Down move / Enter set up / Esc clear,close / F2 details",
                "Ctrl+R reload / Ctrl+U clear / Ctrl+D remove (confirmation)",
            ],
        }
    };
    for (i, hint) in hints.iter().enumerate() {
        paint(buf, chunks[2], i as u16, hint, Style::default().dim());
    }
    if screen.state == State::Setup {
        Clear.render(chunks[1], buf);
        Clear.render(chunks[2], buf);
        let rect = Rect::new(area.x + 4, area.y + 5, area.width.saturating_sub(8), 13);
        Clear.render(rect, buf);
        let card = Block::bordered().title("OpenAI / API key / step 1 of 3");
        let inner = card.inner(rect);
        card.render(rect, buf);
        for (i, text) in [
            "After pasting an illustrative value (not a real key):",
            "Key: ••••••••_",
            "",
            "Enter checks before saving.",
            "If unreachable: another Enter saves unchecked.",
            "A rejected key remains here for correction.",
            "",
            "Ctrl+U clear / Esc cancel and erase input",
        ]
        .iter()
        .enumerate()
        {
            paint(buf, inner, i as u16, text, Style::default());
        }
    }
}

fn capture(width: u16, height: u16, screen: Screen) -> io::Result<Vec<String>> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).map_err(io::Error::other)?;
    terminal
        .draw(|frame| draw(frame.buffer_mut(), screen))
        .map_err(io::Error::other)?;
    let buffer = terminal.backend().buffer();
    Ok((0..height)
        .map(|y| {
            let mut line = String::new();
            let mut x = 0;
            while x < width {
                let symbol = buffer[(x, y)].symbol();
                line.push_str(symbol);
                x += u16::try_from(symbol.width().max(1)).unwrap_or(1);
            }
            line
        })
        .collect())
}

fn write_capture(out: &mut impl Write, width: u16, height: u16, screen: Screen) -> io::Result<()> {
    writeln!(
        out,
        "\n### Option {:?} / {:?} / {width}x{height} / {:?}\n\n```text",
        screen.pattern, screen.surface, screen.state
    )?;
    for line in capture(width, height, screen)? {
        writeln!(out, "{}", line.trim_end())?;
    }
    writeln!(out, "```")
}

fn main() -> io::Result<()> {
    let mut out = io::BufWriter::new(io::stdout().lock());
    writeln!(
        out,
        "## Rendered screens\n\nAll screens below come directly from Ratatui TestBackend. Each block has exactly the named row count; trailing spaces are omitted. Names, route combinations, readiness, badges and setup state are synthetic; only the workload sizes are grounded in static counts. The two surfaces show independent illustrative states. Text captures omit colour and focus styling."
    )?;
    for pattern in [Pattern::A, Pattern::B] {
        for surface in [Surface::Models, Surface::Providers] {
            for (width, height) in [(80, 24), (120, 40)] {
                write_capture(
                    &mut out,
                    width,
                    height,
                    Screen {
                        pattern,
                        surface,
                        state: State::Browse,
                    },
                )?;
            }
        }
    }
    for (surface, state) in [
        (Surface::Models, State::NoLevels),
        (Surface::Models, State::Empty),
        (Surface::Providers, State::Error),
        (Surface::Providers, State::Setup),
    ] {
        write_capture(
            &mut out,
            80,
            24,
            Screen {
                pattern: Pattern::A,
                surface,
                state,
            },
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workload_counts_and_failed_refresh_preserve_real_rows() -> io::Result<()> {
        assert_eq!(
            fixture_rows(Surface::Models)
                .iter()
                .filter(|row| !row.2)
                .count(),
            418
        );
        assert_eq!(
            fixture_rows(Surface::Providers)
                .iter()
                .filter(|row| !row.2)
                .count()
                - 2,
            32
        );
        let text = capture(
            80,
            24,
            Screen {
                pattern: Pattern::A,
                surface: Surface::Providers,
                state: State::Error,
            },
        )?
        .join("\n");
        assert!(text.contains("Showing stale results"));
        assert!(text.contains(">   OpenAI"));
        assert!(text.contains("listed ready"));
        Ok(())
    }

    #[test]
    fn required_screens_keep_borders_actions_and_effort_truth_visible() -> io::Result<()> {
        for pattern in [Pattern::A, Pattern::B] {
            for surface in [Surface::Models, Surface::Providers] {
                for (w, h) in [(80, 24), (120, 40)] {
                    let lines = capture(
                        w,
                        h,
                        Screen {
                            pattern,
                            surface,
                            state: State::Browse,
                        },
                    )?;
                    assert_eq!(lines.len(), usize::from(h));
                    assert!(lines.iter().all(|line| line.width() == usize::from(w)));
                    assert!(lines[0].starts_with('┌') && lines[0].ends_with('┐'));
                    assert!(lines[usize::from(h - 1)].starts_with('└'));
                    let text = lines.join("\n");
                    assert!(text.contains("Enter") && text.contains("Esc"));
                    if surface == Surface::Models {
                        assert!(text.contains("Requested: high"));
                        assert!(text.contains("Applied: unknown"));
                        assert!(text.contains("not offered"));
                    } else {
                        assert!(text.contains("needs setup"));
                        assert!(text.contains("masked key field"));
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn long_names_preserve_status_and_no_levels_never_claims_application() -> io::Result<()> {
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 1));
        row(
            &mut buf,
            Rect::new(0, 0, 30, 1),
            0,
            "a-very-long-模型-model-identifier",
            "unavailable",
            true,
        );
        assert_eq!(buf[(29, 0)].symbol(), "e");
        assert!(buf.content.iter().any(|cell| cell.symbol() == "…"));
        let text = capture(
            80,
            24,
            Screen {
                pattern: Pattern::A,
                surface: Surface::Models,
                state: State::NoLevels,
            },
        )?
        .join("\n");
        assert!(text.contains("Catalogue: reasoning not advertised"));
        assert!(text.contains("Applied: unknown; setting retained"));
        assert!(text.contains("No effort edit here"));
        Ok(())
    }
}
