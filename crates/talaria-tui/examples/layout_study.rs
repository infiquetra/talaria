//! Offline layout study. No input handlers, terminal modes, gateway or audio are changed.
use std::io::{self, Write};
use std::time::Duration;

use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::{Block, Clear, Widget},
};
use talaria_core::checklist::{Checklist, TodoItem, TodoStatus};
use talaria_tui::{
    checklist::{ChecklistSection, ChecklistStyle},
    composer::Composer,
    keys::KeyMap,
    live::{LiveSection, PlainTextSection},
    status::{Connection, StatusInputs, StatusPalette, StatusSection, Tone, VoiceSegment},
    voice_view,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OptionKind {
    A,
    B,
}

#[derive(Clone, Copy, Debug)]
enum Voice {
    Recording,
    Transcribing,
    Speaking,
}

impl Voice {
    fn name(self) -> &'static str {
        match self {
            Self::Recording => "recording",
            Self::Transcribing => "transcribing",
            Self::Speaking => "speaking",
        }
    }

    fn segment(self, target: &str) -> VoiceSegment {
        let mut segment = match self {
            Self::Recording => voice_view::recording_segment(Duration::from_secs(7), true),
            Self::Transcribing => voice_view::transcribing_segment(true),
            // Speaking is a study fixture for the proposed playback state, not a live capability.
            Self::Speaking => Some(VoiceSegment {
                text: "SPEAK +read".into(),
                tone: Tone::Accent,
            }),
        }
        .unwrap_or(VoiceSegment {
            text: "voice".into(),
            tone: Tone::Muted,
        });
        segment.text.push_str(&format!(" -> {target}"));
        segment
    }
}

#[derive(Clone, Copy)]
struct Screen {
    option: OptionKind,
    panel: bool,
    bar: bool,
    voice: Voice,
    target: &'static str,
    menu: bool,
    agents_tab: bool,
}

fn checklist() -> Checklist {
    Checklist {
        revision: 1,
        skipped: 0,
        items: [
            ("Read inputs", TodoStatus::Completed),
            ("Compare layouts", TodoStatus::InProgress),
            ("Review screens", TodoStatus::Pending),
        ]
        .into_iter()
        .enumerate()
        .map(|(id, (content, status))| TodoItem {
            id: id.to_string(),
            content: content.into(),
            status,
            parent: None,
        })
        .collect(),
    }
}

fn text(buf: &mut Buffer, area: Rect, content: &str) {
    let lines: Vec<_> = content.lines().map(Line::from).collect();
    for (y, line) in (area.y..area.bottom()).zip(lines) {
        buf.set_line(area.x, y, &line, area.width);
    }
}

fn inspector(buf: &mut Buffer, area: Rect, option: OptionKind, agents_tab: bool) {
    let heading = match (option, agents_tab) {
        (OptionKind::A, _) => "Work / 1 needs you",
        (OptionKind::B, false) => "[Tasks]  Agents (1 needs you)",
        (OptionKind::B, true) => "Tasks  [Agents] (1 needs you)",
    };
    text(buf, Rect::new(area.x, area.y, area.width, 1), heading);
    let tasks = "Goal: keep the interface light\nTurn: running / compare layouts\nVerdict: pending";
    if !agents_tab {
        text(buf, Rect::new(area.x, area.y + 2, area.width, 3), tasks);
        let list = checklist();
        let style = ChecklistStyle::plain();
        ChecklistSection::new(&list, true, 4, None, &style)
            .render(Rect::new(area.x, area.y + 6, area.width, 4), buf);
    }
    if option == OptionKind::A || agents_tab {
        let y = area.y + if agents_tab { 2 } else { 11 };
        text(
            buf,
            Rect::new(area.x, y, area.width, area.bottom().saturating_sub(y)),
            "Agents / 1 running, 1 waiting\n! review: needs you\n  model: unknown / effort: ?\n> layout: compare / running\n  GPT-6.1 Terra / medium\nAlt+S details / Alt+A next",
        );
    } else if area.height > 11 {
        text(
            buf,
            Rect::new(area.x, area.y + 11, area.width, 3),
            "Agents: 1 running, 1 needs you\nTab: Agents / Alt+S details",
        );
    }
}

fn draw(buf: &mut Buffer, screen: Screen) {
    let area = buf.area;
    let top = if screen.bar {
        if screen.option == OptionKind::A { 1 } else { 2 }
    } else {
        0
    };
    if screen.bar {
        text(
            buf,
            Rect::new(0, 0, area.width, top),
            if screen.option == OptionKind::A {
                "Talaria   Session   Model   View   Voice   Settings   Help"
            } else {
                "Talaria / Session: Layout study\nSession   Model   View   Voice   Settings   Help   | Menu: Alt+I"
            },
        );
    }
    let body = Rect::new(0, top, area.width, area.height.saturating_sub(top + 4));
    let docked = screen.panel && area.width >= 112 && area.height >= 28;
    let rail_width = if screen.option == OptionKind::A {
        33
    } else {
        41
    };
    let main_width = if docked {
        area.width - rail_width
    } else {
        area.width
    };
    text(
        buf,
        Rect::new(0, body.y + 1, main_width, 1),
        "You: Keep the interface clean and light.",
    );
    let tail = PlainTextSection::new(
        "Talaria\nI am comparing two optional layouts.\nBoth leave the default conversation quiet.\n\nThe next step is your screen review.",
    );
    tail.render(Rect::new(0, body.y + 3, main_width, 6), buf);
    if screen.panel {
        if docked || screen.option == OptionKind::A {
            let x = if docked {
                main_width
            } else {
                area.width.saturating_sub(rail_width)
            };
            let rail = Rect::new(x, body.y, rail_width.min(area.width), body.height);
            // Narrow Option A is modal: clear the covered text rather than reflowing history.
            for y in rail.y..rail.bottom() {
                for x in rail.x..rail.right() {
                    buf[(x, y)].reset();
                }
                if y - rail.y < 17 {
                    buf.set_string(rail.x, y, "│", ratatui::style::Style::new().dim());
                }
            }
            inspector(
                buf,
                Rect::new(rail.x + 1, rail.y, rail.width - 1, rail.height),
                screen.option,
                screen.agents_tab,
            );
        } else {
            let height = 11.min(body.height);
            let y = body.bottom() - height;
            for row in y..body.bottom() {
                for x in 0..area.width {
                    buf[(x, row)].reset();
                }
            }
            inspector(
                buf,
                Rect::new(0, y, area.width, height),
                screen.option,
                screen.agents_tab,
            );
        }
    } else {
        let list = checklist();
        let style = ChecklistStyle::plain();
        ChecklistSection::new(&list, false, 1, Some("Ctrl+T".into()), &style)
            .render(Rect::new(0, body.bottom() - 1, area.width, 1), buf);
    }
    let status = StatusInputs {
        connection: Connection::Connected,
        model: Some("GPT-6.1 Terra".into()),
        reasoning_effort: Some("medium".into()),
        needs_you: 1,
        voice: Some(screen.voice.segment(screen.target)),
        ..Default::default()
    };
    StatusSection::new(&status, None, &StatusPalette::default())
        .render_into(Rect::new(0, area.height - 1, area.width, 1), buf);
    let mut composer = Composer::new(KeyMap::defaults());
    composer.load("Keep the defaults quiet.".into());
    composer
        .section(area.width, 3)
        .render(Rect::new(0, area.height - 4, area.width, 3), buf);
    if screen.menu {
        let x = 28.min(area.width.saturating_sub(32));
        let menu_area = Rect::new(x, top, 32, 8);
        Clear.render(menu_area, buf);
        let block = Block::bordered().title("View");
        let inner = block.inner(menu_area);
        block.render(menu_area, buf);
        text(
            buf,
            inner,
            "> [x] Right panel   Alt+D\n  [x] Top bar       Alt+I\n  Tasks            Ctrl+T\n  Agents           Alt+S\n  Transcript       Ctrl+O\nEsc: return to composer",
        );
    }
}

fn capture(width: u16, height: u16, screen: Screen) -> io::Result<Vec<String>> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).map_err(io::Error::other)?;
    terminal
        .draw(|frame| draw(frame.buffer_mut(), screen))
        .map_err(io::Error::other)?;
    let buf = terminal.backend().buffer();
    Ok((0..height)
        .map(|y| (0..width).map(|x| buf[(x, y)].symbol()).collect())
        .collect())
}

fn write_capture(out: &mut impl Write, width: u16, height: u16, screen: Screen) -> io::Result<()> {
    writeln!(
        out,
        "\n### Option {:?} / {width}x{height} / panel {} / bar {} / {} / {}{}{}\n\n```text",
        screen.option,
        if screen.panel { "ON" } else { "OFF" },
        if screen.bar { "ON" } else { "OFF" },
        screen.voice.name(),
        screen.target,
        if screen.menu { " / menu focused" } else { "" },
        if screen.agents_tab {
            " / Agents tab"
        } else {
            ""
        },
    )?;
    for row in capture(width, height, screen)? {
        writeln!(out, "{}", row.trim_end())?;
    }
    writeln!(out, "```")
}

fn main() -> io::Result<()> {
    let mut out = io::BufWriter::new(io::stdout().lock());
    writeln!(
        out,
        "## Ratatui TestBackend captures\n\nEvery block has exactly the named number of rows. Trailing blank cells are omitted; pad each row with spaces to its named width. No other cell text is edited. All content is synthetic. Both options are rendered, including the recommended Option A. Color and cursor styling are not represented in text."
    )?;
    for option in [OptionKind::A, OptionKind::B] {
        for (width, height) in [(80, 24), (120, 40)] {
            for enabled in [false, true] {
                for voice in [Voice::Recording, Voice::Transcribing, Voice::Speaking] {
                    write_capture(
                        &mut out,
                        width,
                        height,
                        Screen {
                            option,
                            panel: enabled,
                            bar: enabled,
                            voice,
                            target: "Hermes:design",
                            menu: false,
                            agents_tab: false,
                        },
                    )?;
                }
            }
        }
    }
    // Mixed toggles, menu focus and other backend labels supplement the required matrix.
    for (width, height) in [(80, 24), (120, 40)] {
        for (panel, bar, menu, target) in [
            (true, false, false, "Herdr:review"),
            (false, true, false, "ACP:review"),
            (true, true, true, "Hermes:design"),
        ] {
            write_capture(
                &mut out,
                width,
                height,
                Screen {
                    option: OptionKind::A,
                    panel,
                    bar,
                    voice: Voice::Speaking,
                    target,
                    menu,
                    agents_tab: false,
                },
            )?;
        }
        write_capture(
            &mut out,
            width,
            height,
            Screen {
                option: OptionKind::B,
                panel: true,
                bar: true,
                voice: Voice::Speaking,
                target: "ACP:review",
                menu: false,
                agents_tab: true,
            },
        )?;
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn required_matrix_keeps_voice_target_and_composer_visible() -> io::Result<()> {
        for option in [OptionKind::A, OptionKind::B] {
            for (width, height) in [(80, 24), (120, 40)] {
                for enabled in [false, true] {
                    for voice in [Voice::Recording, Voice::Transcribing, Voice::Speaking] {
                        let rows = capture(
                            width,
                            height,
                            Screen {
                                option,
                                panel: enabled,
                                bar: enabled,
                                voice,
                                target: "Hermes:design",
                                menu: false,
                                agents_tab: false,
                            },
                        )?;
                        assert_eq!(rows.len(), usize::from(height));
                        assert!(rows.iter().all(|row| row.width() == usize::from(width)));
                        let status = &rows[usize::from(height - 1)];
                        assert!(status.contains("Hermes:design"));
                        assert!(status.contains(match voice {
                            Voice::Recording => "REC 0:07",
                            Voice::Transcribing => "transcribing…",
                            Voice::Speaking => "SPEAK +read",
                        }));
                        assert!(rows[usize::from(height - 3)].contains("Keep the defaults quiet."));
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn recommended_layout_changes_at_the_documented_boundary() -> io::Result<()> {
        let screen = Screen {
            option: OptionKind::A,
            panel: true,
            bar: true,
            voice: Voice::Speaking,
            target: "ACP:review",
            menu: false,
            agents_tab: false,
        };
        for (width, height, x) in [(111, 28, 78), (112, 27, 79), (112, 28, 79), (120, 40, 87)] {
            let rows = capture(width, height, screen)?;
            assert!(rows[1].contains("Work / 1 needs you"));
            assert_eq!(rows[1].chars().nth(x), Some('│'));
            assert!(rows[usize::from(height - 1)].contains("ACP:review"));
        }
        Ok(())
    }
}
