//! Rendering: scrollback lines and the live region (tail, status, input / approval box).
use crate::state::{Kind, OutLine, Phase, State};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

const SPIN: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn style_line(l: &OutLine) -> Line<'static> {
    let t = l.text.clone();
    match l.kind {
        Kind::Assistant => Line::raw(t),
        Kind::User => Line::styled(t, Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Kind::Dim => Line::styled(t, Style::new().add_modifier(Modifier::DIM)),
        Kind::Error => Line::styled(t, Style::new().fg(Color::Red)),
        Kind::Notice => Line::styled(t, Style::new().fg(Color::Yellow)),
        Kind::DiffAdd => Line::styled(t, Style::new().fg(Color::Green)),
        Kind::DiffDel => Line::styled(t, Style::new().fg(Color::Red)),
        Kind::DiffHunk => Line::styled(t, Style::new().fg(Color::Cyan)),
        Kind::Tool => tool_line(&t),
    }
}

fn tool_line(t: &str) -> Line<'static> {
    let mut spans = vec![];
    let body = t.strip_prefix('⏺').unwrap_or(t);
    spans.push(Span::styled("⏺", Style::new().fg(Color::Green)));
    // Colour the trailing status mark.
    if let Some(i) = body.rfind(" ✓") {
        spans.push(Span::raw(body[..i].to_owned()));
        spans.push(Span::styled(body[i..].to_owned(), Style::new().fg(Color::Green)));
    } else if let Some(i) = body.rfind(" ✗") {
        spans.push(Span::raw(body[..i].to_owned()));
        spans.push(Span::styled(body[i..].to_owned(), Style::new().fg(Color::Red)));
    } else {
        spans.push(Span::styled(body.to_owned(), Style::new().add_modifier(Modifier::DIM)));
    }
    Line::from(spans)
}

/// Draw lines (already wrapped to the width) into a scrollback insertion buffer.
pub fn draw_history(lines: &[OutLine], buf: &mut Buffer) {
    let ls: Vec<Line> = lines.iter().map(style_line).collect();
    Paragraph::new(ls).render(buf.area, buf);
}

/// Rows the live region needs for the present state.
pub fn live_height(s: &State) -> u16 {
    if let Some(a) = &s.approval {
        // title + up to 3 command rows + description + choices
        return 1 + cmd_rows(a.command.as_str(), s.width).len().min(3) as u16 + 1 + a.choices.len() as u16 + 2;
    }
    if s.running {
        return crate::state::TAIL_ROWS as u16 + 2;
    }
    2
}

fn cmd_rows(cmd: &str, width: u16) -> Vec<String> {
    let mut v = vec![];
    for l in cmd.lines() {
        v.extend(crate::state::wrap(l, (width as usize).saturating_sub(4)));
    }
    if v.is_empty() {
        v.push(String::new());
    }
    v
}

pub fn status_text(s: &State, now_ms: u64) -> String {
    let secs = s.elapsed_ms(now_ms) as f64 / 1000.0;
    let spin = if s.running { format!("{} ", SPIN[(now_ms / 90) as usize % SPIN.len()]) } else { String::new() };
    let mut t = format!("{spin}{} · {}", s.model, s.phase.label());
    if s.running || s.elapsed_ms(now_ms) > 0 {
        t.push_str(&format!(" · {secs:.1}s"));
    }
    if let Some(n) = &s.notice {
        t.push_str(&format!(" · {}", crate::state::truncate(n, 40)));
    }
    t
}

pub fn draw_live(f: &mut Frame, s: &State, now_ms: u64) {
    let area = f.area();
    let h = area.height;
    if h < 2 {
        return;
    }
    let status_y = area.y + h - 2;
    let input_y = area.y + h - 1;
    let top = Rect::new(area.x, area.y, area.width, h - 2);

    if let Some(a) = &s.approval {
        let mut ls: Vec<Line> = vec![Line::styled(
            "⏸ approval required",
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )];
        let rows = cmd_rows(&a.command, s.width);
        let n = rows.len();
        for (i, r) in rows.into_iter().take(3).enumerate() {
            let r = if i == 2 && n > 3 { format!("{r} …") } else { r };
            ls.push(Line::styled(format!("  $ {r}"), Style::new().add_modifier(Modifier::BOLD)));
        }
        ls.push(Line::styled(
            format!("  {}", crate::state::truncate(&a.description, (s.width as usize).saturating_sub(4))),
            Style::new().add_modifier(Modifier::DIM),
        ));
        for (i, c) in a.choices.iter().enumerate() {
            let sel = i == s.approval_sel;
            let st = if sel { Style::new().fg(Color::Black).bg(Color::Yellow) } else { Style::new() };
            ls.push(Line::styled(format!(" {} {}. {c}", if sel { "›" } else { " " }, i + 1), st));
        }
        Paragraph::new(ls).render(top, f.buffer_mut());
    } else {
        let live = s.live_lines();
        let ls: Vec<Line> = live.iter().map(style_line).collect();
        Paragraph::new(ls).render(top, f.buffer_mut());
    }

    let st = status_text(s, now_ms);
    let style = match s.phase {
        Phase::Failed => Style::new().fg(Color::Red),
        Phase::WaitingApproval => Style::new().fg(Color::Yellow),
        _ => Style::new().add_modifier(Modifier::DIM),
    };
    Paragraph::new(Line::styled(st, style)).render(Rect::new(area.x, status_y, area.width, 1), f.buffer_mut());

    let prompt = if s.approval.is_some() {
        "  1-4 or ↑/↓ + Enter to answer".to_owned()
    } else if s.running {
        format!("  {}  (Ctrl+C to interrupt)", s.input)
    } else {
        format!("› {}", s.input)
    };
    Paragraph::new(Line::raw(prompt.clone())).render(Rect::new(area.x, input_y, area.width, 1), f.buffer_mut());
    if s.approval.is_none() && !s.running {
        let x = (area.x + 2 + s.input.width() as u16).min(area.x + area.width.saturating_sub(1));
        f.set_cursor_position((x, input_y));
    }
}
