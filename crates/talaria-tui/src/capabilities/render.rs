use crate::history::{sanitize, wrap_lines};
use crate::theme::surfaces::{ViewStyles, tokens as t};
use ratatui::{buffer::Buffer, layout::Rect, style::Style, text::Line};
use talaria_core::capabilities::{CapListRow, CapTab, CapView};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

fn cut(text: &str, width: usize) -> String {
    let text = sanitize(text);
    if text.width() <= width {
        return text.into_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for g in text.graphemes(true) {
        let w = g.width();
        if used + w > width - 1 {
            break;
        }
        out.push_str(g);
        used += w;
    }
    out.push('…');
    out
}
fn field(text: &str, width: usize) -> String {
    let s = cut(text, width.saturating_sub(2));
    format!("{s}{}", " ".repeat(width.saturating_sub(s.width())))
}
fn line(buf: &mut Buffer, area: Rect, y: u16, text: &str) {
    if y >= area.height {
        return;
    }
    buf.set_stringn(
        area.x,
        area.y + y,
        cut(text, usize::from(area.width)),
        usize::from(area.width),
        Style::default(),
    );
}
fn rule(buf: &mut Buffer, area: Rect, y: u16, views: Option<&ViewStyles>) {
    line(
        buf,
        area,
        y,
        &format!(
            " {}",
            views.map_or_else(
                || "─".repeat(usize::from(area.width.saturating_sub(1))),
                |v| v.divider_text(&"─".repeat(usize::from(area.width.saturating_sub(1))))
            )
        ),
    );
}
fn row_text(row: &CapListRow, tab: CapTab, catalog: bool, selected: bool, width: usize) -> String {
    if row.is_header {
        let text = if row.name.is_empty() {
            &row.label
        } else {
            &row.name
        };
        return format!(" {}", cut(text, width.saturating_sub(1)));
    }
    let mut s = format!(" {}", if selected { "> " } else { "  " });
    if !catalog && matches!(tab, CapTab::Skills | CapTab::Toolsets | CapTab::Plugins) {
        s.push_str(&format!("{} ", row.marker));
    }
    if width < 80 {
        s.push_str(&cut(&row.name, width.saturating_sub(s.width())));
        return s;
    }
    let name_width = if width >= 100 { 30 } else { 24 };
    if catalog {
        s.push_str(&field(&row.name, name_width));
        s.push_str(&field(&row.label, 12));
        s.push_str(&field(if row.installed { "installed" } else { "" }, 11));
        s.push_str(&cut(&row.description, width.saturating_sub(s.width())));
    } else {
        match tab {
            CapTab::Skills => {
                s.push_str(&field(&row.name, name_width));
                s.push_str(&field(&row.label, 9));
                if width >= 100 {
                    s.push_str(&field(&format!("used {}", row.usage), 10));
                }
                s.push_str(&cut(&row.description, width.saturating_sub(s.width())));
            }
            CapTab::Hub => {
                s.push_str(&field(&row.name, name_width));
                s.push_str(&field(
                    if row.trust.is_empty() {
                        "-"
                    } else {
                        &row.trust
                    },
                    11,
                ));
                s.push_str(&field(&row.source, 12));
                let suffix = if row.installed { "  installed" } else { "" };
                s.push_str(&cut(
                    &row.description,
                    width.saturating_sub(s.width() + suffix.width()),
                ));
                s.push_str(suffix);
            }
            CapTab::Toolsets => {
                let label = if row.label == row.name {
                    row.label.clone()
                } else {
                    format!("{} ({})", row.label, row.name)
                };
                s.push_str(&field(&label, if width >= 100 { 34 } else { 24 }));
                s.push_str(&field(&format!("{} tools", row.tool_count), 11));
                s.push_str(&field(if row.configured { "" } else { "needs setup" }, 13));
                if row.platform != "cli" && !row.platform_label.is_empty() {
                    s.push_str(&format!("· {}", row.platform_label));
                }
            }
            CapTab::Plugins => {
                s.push_str(&field(&row.name, name_width));
                s.push_str(&field(
                    if row.version.is_empty() {
                        "-"
                    } else {
                        &row.version
                    },
                    10,
                ));
                s.push_str(&field(&row.source, 9));
                s.push_str(&field(
                    &if row.server_count == 0 {
                        String::new()
                    } else {
                        format!("{} servers", row.server_count)
                    },
                    12,
                ));
                if row.update_available {
                    s.push_str("update available");
                }
            }
        }
    }
    cut(&s, width)
}

/// Draw the overlay without writing terminal escape sequences.
pub fn draw_capabilities(view: &CapView, area: Rect, buf: &mut Buffer) {
    draw_inner(view, area, buf, None);
}

/// Draw with resolved view styles.
pub fn draw_capabilities_with_theme(
    view: &CapView,
    area: Rect,
    buf: &mut Buffer,
    views: &ViewStyles,
) {
    draw_inner(view, area, buf, Some(views));
}

fn draw_inner(view: &CapView, area: Rect, buf: &mut Buffer, views: Option<&ViewStyles>) {
    if area.width < 40 || area.height < 10 {
        line(buf, area, 0, "Capabilities needs at least 40 by 10.");
        if let Some(v) = views {
            v.finish(area, buf, t::LIST_BACKGROUND, true);
        }
        return;
    }
    line(buf, area, 0, &view.title);
    if let Some(v) = views {
        buf.set_style(
            Rect::new(area.x, area.y, area.width, 1),
            v.style(t::LIST_HEADER, Style::default()),
        );
    }
    let active = match view.tab {
        CapTab::Skills => 0,
        CapTab::Hub => 1,
        CapTab::Toolsets => 2,
        CapTab::Plugins => 3,
    };
    let tabs = view
        .tabs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            if i == active {
                format!("[{t}]")
            } else {
                t.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("  ");
    line(buf, area, 1, &format!(" {tabs}"));
    rule(buf, area, 2, views);
    let width = usize::from(area.width.saturating_sub(1));
    let list_notice = view.rows.first().is_some_and(|row| {
        row.is_header && !row.label.is_empty() && row.label == view.message.join("\n")
    });
    let mut messages = wrap_lines(
        view.message
            .iter()
            .filter(|_| !list_notice)
            .map(|s| Line::from(sanitize(s).into_owned()))
            .collect(),
        area.width.saturating_sub(1),
    );
    let message_limit = 8.min(usize::from(area.height).saturating_sub(
        5 + usize::from(!view.footer.is_empty()) + usize::from(view.input.is_some()),
    ));
    if messages.len() > message_limit {
        messages.truncate(message_limit);
        if message_limit > 0 {
            let last = messages.pop().unwrap_or_default().to_string();
            messages.push(Line::from(format!(
                "{}…",
                cut(&last, width.saturating_sub(1))
            )));
        }
    }
    let bottom = 1
        + usize::from(!view.footer.is_empty())
        + usize::from(view.input.is_some())
        + messages.len();
    let bottom_start = usize::from(area.height).saturating_sub(bottom);
    let bottom_rule = bottom_start.saturating_sub(1);
    rule(buf, area, bottom_rule as u16, views);
    let mut y = bottom_start as u16;
    for message in messages {
        line(buf, area, y, &format!(" {message}"));
        y += 1;
    }
    if let Some(input) = &view.input {
        line(buf, area, y, &format!(" {input}"));
        y += 1;
    }
    if !view.footer.is_empty() {
        line(buf, area, y, &format!(" {}", view.footer));
        y += 1;
    }
    line(buf, area, y, &format!(" {}", view.hints));
    let wide = area.width >= 100;
    let list_height = if wide {
        bottom_rule.saturating_sub(3) * 60 / 100
    } else {
        bottom_rule.saturating_sub(3)
    };
    let detail_y = 3 + list_height + 1;
    if wide {
        rule(buf, area, (3 + list_height) as u16, views);
    }
    let detail = wrap_lines(
        view.detail
            .iter()
            .map(|s| Line::from(sanitize(s).into_owned()))
            .collect(),
        area.width.saturating_sub(1),
    );
    if !wide && view.detail_focus {
        for (i, text) in detail
            .iter()
            .skip(view.detail_scroll)
            .take(list_height)
            .enumerate()
        {
            line(buf, area, (3 + i) as u16, &format!(" {text}"));
        }
    } else {
        let start = view.selected.saturating_add(1).saturating_sub(list_height);
        for (i, row) in view.rows.iter().enumerate().skip(start).take(list_height) {
            line(
                buf,
                area,
                (3 + i - start) as u16,
                &row_text(
                    row,
                    view.tab,
                    view.catalog,
                    i == view.selected,
                    usize::from(area.width),
                ),
            );
        }
        if wide {
            let height = bottom_rule.saturating_sub(detail_y);
            let scroll = if view.detail_focus {
                view.detail_scroll
            } else {
                0
            };
            for (i, text) in detail.iter().skip(scroll).take(height).enumerate() {
                line(buf, area, (detail_y + i) as u16, &format!(" {text}"));
            }
        }
    }
    if let Some(v) = views {
        v.finish(area, buf, t::LIST_BACKGROUND, true);
    }
}
