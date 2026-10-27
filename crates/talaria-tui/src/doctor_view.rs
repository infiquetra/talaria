//! The doctor report and restart confirmation overlay.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};
pub use talaria_core::doctor::DoctorKey;
use talaria_core::doctor::{
    DoctorView, RestartState, check_name, confirmation_lines, outcome_fix, outcome_text,
    severity_text,
};

const FOOTER: &str = "For provider keys, tools and dependencies, run hermes doctor.";
const HINT: &str = "↑↓ select · Enter details · r check again · R restart gateway · Esc close";

/// Draw a doctor report and its current restart state.
pub fn draw_doctor(view: &DoctorView, restart: &RestartState, area: Rect, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(report) = &view.report {
        let (ok, warnings, failures) = report.counts();
        lines.push(Line::raw(format!(
            "Doctor: {ok} ok, {warnings} {}, {failures} {}",
            plural(warnings, "warning"),
            plural(failures, "failure")
        )));
        for (index, check) in report.checks.iter().enumerate() {
            let prefix = if area.width < 40 {
                format!("{:<6}", severity_text(check.severity))
            } else {
                format!(
                    "{:<6}{:<16}",
                    severity_text(check.severity),
                    check_name(check.id)
                )
            };
            let style = if index == view.selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            push_wrapped(&mut lines, &prefix, &check.summary, area.width, style);
            if let Some(fix) = &check.fix {
                push_wrapped_with_indent(
                    &mut lines,
                    "      ",
                    &format!("Fix: {fix}"),
                    area.width,
                    Style::default(),
                    if area.width < 40 { 6 } else { 22 },
                );
            }
            if view.expanded.contains(&check.id) {
                for detail in &check.details {
                    push_wrapped(&mut lines, "        ", detail, area.width, Style::default());
                }
            }
        }
    } else {
        lines.push(Line::raw("Checking the gateway…"));
    }
    let mut status = Vec::new();
    match restart {
        RestartState::Idle => {}
        RestartState::Asking { .. } => {
            status.push(Line::raw("Asking the gateway what is running…"))
        }
        RestartState::Confirming {
            profiles,
            work,
            restart_selected,
            ..
        } => {
            for line in confirmation_lines(profiles, work.as_ref()) {
                if usize::from(area.width) < line.len()
                    && line.contains("will be interrupted:")
                    && let Some((intro, counts)) = line.split_once(": ")
                {
                    push_wrapped(
                        &mut status,
                        "",
                        &format!("{intro}:"),
                        area.width,
                        Style::default(),
                    );
                    push_wrapped(&mut status, "", counts, area.width, Style::default());
                } else {
                    push_wrapped(&mut status, "", &line, area.width, Style::default());
                }
            }
            let selected = Style::default().add_modifier(Modifier::REVERSED);
            status.push(Line::from(vec![
                Span::styled(
                    "[ Cancel ]",
                    if *restart_selected {
                        Style::default()
                    } else {
                        selected
                    },
                ),
                Span::raw("  "),
                Span::styled(
                    "[ Restart ]",
                    if *restart_selected {
                        selected
                    } else {
                        Style::default()
                    },
                ),
            ]));
        }
        RestartState::Requesting { .. } => push_progress(&mut status, 0, area.width),
        RestartState::Waiting { elapsed, .. } => {
            push_progress(&mut status, elapsed.as_secs(), area.width);
        }
        RestartState::Done(outcome) => {
            push_wrapped(
                &mut status,
                "",
                &outcome_text(outcome),
                area.width,
                Style::default(),
            );
            if let Some(fix) = outcome_fix(outcome) {
                push_wrapped(
                    &mut status,
                    "",
                    &format!("Fix: {fix}"),
                    area.width,
                    Style::default(),
                );
            }
        }
    }
    let mut footer = Vec::new();
    if usize::from(area.width) < FOOTER.len() {
        let intro = FOOTER
            .strip_suffix("hermes doctor.")
            .unwrap_or(FOOTER)
            .trim_end();
        push_wrapped(&mut footer, "", intro, area.width, Style::default());
        push_wrapped(
            &mut footer,
            "",
            "hermes doctor.",
            area.width,
            Style::default(),
        );
    } else {
        push_wrapped(&mut footer, "", FOOTER, area.width, Style::default());
    }
    push_wrapped(&mut footer, "", HINT, area.width, Style::default());
    let footer_height = area
        .height
        .min(u16::try_from(footer.len()).unwrap_or(u16::MAX));
    let available = usize::from(area.height - footer_height);
    let report_height = available.saturating_sub(status.len());
    lines.truncate(report_height);
    lines.extend(status.into_iter().take(available));
    Paragraph::new(lines).wrap(Wrap { trim: false }).render(
        Rect {
            height: area.height - footer_height,
            ..area
        },
        buf,
    );
    let skip = footer.len().saturating_sub(usize::from(footer_height));
    Paragraph::new(footer.into_iter().skip(skip).collect::<Vec<_>>()).render(
        Rect::new(
            area.x,
            area.bottom() - footer_height,
            area.width,
            footer_height,
        ),
        buf,
    );
}

fn plural(count: usize, word: &'static str) -> String {
    if count == 1 {
        word.to_string()
    } else {
        format!("{word}s")
    }
}

fn push_progress(lines: &mut Vec<Line<'static>>, seconds: u64, width: u16) {
    let prefix = "Restart requested. Waiting for the gateway to come back";
    let elapsed = format!("({seconds} s)");
    let text = format!("{prefix} {elapsed}");
    if usize::from(width) < text.len() {
        push_wrapped(lines, "", prefix, width, Style::default());
        push_wrapped(lines, "", &elapsed, width, Style::default());
    } else {
        push_wrapped(lines, "", &text, width, Style::default());
    }
}

fn push_wrapped(
    lines: &mut Vec<Line<'static>>,
    prefix: &str,
    text: &str,
    width: u16,
    style: Style,
) {
    use unicode_width::UnicodeWidthStr;
    push_wrapped_with_indent(
        lines,
        prefix,
        text,
        width,
        style,
        UnicodeWidthStr::width(prefix),
    );
}

fn push_wrapped_with_indent(
    lines: &mut Vec<Line<'static>>,
    prefix: &str,
    text: &str,
    width: u16,
    style: Style,
    continuation: usize,
) {
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;
    let width = usize::from(width);
    let indent = " ".repeat(continuation.min(width.saturating_sub(1)));
    let mut current = prefix.to_owned();
    let mut has_text = false;
    for word in text.split_whitespace() {
        let current_width = UnicodeWidthStr::width(current.as_str());
        if has_text && current_width + 1 + UnicodeWidthStr::width(word) > width {
            lines.push(Line::styled(current, style));
            current = indent.clone();
            has_text = false;
        }
        if has_text {
            current.push(' ');
        }
        for grapheme in word.graphemes(true) {
            if has_text
                && UnicodeWidthStr::width(current.as_str()) + UnicodeWidthStr::width(grapheme)
                    > width
            {
                lines.push(Line::styled(current, style));
                current = indent.clone();
            }
            current.push_str(grapheme);
            has_text = true;
        }
    }
    lines.push(Line::styled(current, style));
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn long_words_wrap_at_display_width_with_continuation_indent() {
        for (width, prefix, continuation) in [(60, "warn  Runtime health  ", 22), (39, "warn  ", 6)]
        {
            let mut lines = Vec::new();
            let word = "界".repeat(50);
            push_wrapped_with_indent(
                &mut lines,
                prefix,
                &word,
                width,
                Style::default(),
                continuation,
            );
            assert!(lines.len() > 1);
            let text: Vec<_> = lines
                .iter()
                .map(|line| {
                    line.spans
                        .iter()
                        .map(|span| span.content.as_ref())
                        .collect::<String>()
                })
                .collect();
            assert!(
                text.iter()
                    .all(|line| UnicodeWidthStr::width(line.as_str()) <= usize::from(width))
            );
            assert!(text[0].starts_with(prefix));
            assert!(
                text[1..]
                    .iter()
                    .all(|line| line.starts_with(&" ".repeat(continuation)))
            );
            assert_eq!(
                text.iter()
                    .map(|line| line.matches('界').count())
                    .sum::<usize>(),
                50
            );
        }
    }
}
