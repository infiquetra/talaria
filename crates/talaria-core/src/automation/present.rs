//! Presenting Hermes's own command replies.
//!
//! Behavior contract: issue 236.

use crate::commands::{CommandCatalog, CommandOutcome};

/// The commands whose replies render as cells, by canonical name.
pub const PRESENTED_COMMANDS: &[&str] = &[
    "plan",
    "goal",
    "subgoal",
    "loop",
    "heartbeat",
    "yolo",
    "fast",
    "reasoning",
    "busy",
    "verbose",
    "footer",
    "approvals",
    "personality",
    "codex-runtime",
];

/// The note after the first submitted `/plan` of a session.
pub const PLAN_MODE_NOTE: &str = "Plan mode is an instruction to the agent, not a sandbox: it \
    should only write a plan under .hermes/plans/ in the session folder, but nothing stops a \
    tool call, and approvals still apply.";

/// A presented command reply: the typed line and its cleaned body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HermesCommandCell {
    /// The canonical command name.
    pub name: String,
    /// The line exactly as typed.
    pub typed: String,
    /// The cleaned reply body.
    pub body: String,
    /// True when the body opens with a warning glyph.
    pub warning: bool,
}

/// What a command reply becomes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Presentation {
    /// A ran command: one cell.
    Output(HermesCommandCell),
    /// A queued turn: an optional cell plus the submission.
    Submit {
        /// The cell, when the notice is non-blank.
        cell: Option<HermesCommandCell>,
        /// The turn text.
        text: String,
        /// The display line.
        display: String,
    },
}

/// The canonical command name of a typed line, if it is one.
///
/// The name comes from the catalogue when there is one, so an alias such as `/proactive`
/// resolves to the command it points at. A name the catalogue does not know keeps its own
/// spelling, because the presented list is fixed. Only a presented command has a name here.
pub fn canonical_name(typed: &str, catalog: Option<&CommandCatalog>) -> Option<String> {
    let line = crate::commands::parse_slash_line(typed)?;
    let name = catalog
        .and_then(|catalog| catalog.canonical(&line.name))
        .map(|name| name.trim_start_matches('/').to_owned())
        .unwrap_or(line.name);
    if PRESENTED_COMMANDS.contains(&name.as_str()) {
        Some(name)
    } else {
        None
    }
}

/// What a command reply becomes, if this unit presents it.
///
/// A ran command becomes one cell. A queued turn becomes a cell when its notice says
/// something, plus the submission itself. Every other outcome keeps the text the command
/// path already shows.
pub fn present_command(
    canonical: &str,
    typed: &str,
    outcome: &CommandOutcome,
) -> Option<Presentation> {
    if !PRESENTED_COMMANDS.contains(&canonical) {
        return None;
    }
    match outcome {
        CommandOutcome::Output { text, .. } => {
            let body = clean_command_text(text);
            if body.trim().is_empty() {
                return None;
            }
            Some(Presentation::Output(HermesCommandCell {
                name: canonical.to_owned(),
                typed: typed.to_owned(),
                warning: opens_with_warning(&body),
                body,
            }))
        }
        CommandOutcome::Submit {
            text,
            display,
            notice,
        } => {
            let cell = notice.as_ref().map(|notice| {
                let body = clean_command_text(notice);
                HermesCommandCell {
                    name: canonical.to_owned(),
                    typed: typed.to_owned(),
                    warning: opens_with_warning(&body),
                    body,
                }
            });
            Some(Presentation::Submit {
                cell: cell.filter(|cell| !cell.body.trim().is_empty()),
                text: text.clone(),
                display: display.clone(),
            })
        }
        _ => None,
    }
}

/// Whether the body opens with a warning glyph.
fn opens_with_warning(body: &str) -> bool {
    body.trim_start()
        .chars()
        .next()
        .is_some_and(|ch| matches!(ch, '⚠' | '⛔' | '⏸'))
}

/// Clean a command reply for Markdown rendering.
///
/// Drops U+FE0F, normalises line endings, drops trailing blank lines, and escapes `<` and `>`
/// outside code spans so a placeholder such as `<prompt>` stays visible. Everything else is
/// kept; the renderer's sanitiser turns control characters into stand-ins.
pub fn clean_command_text(text: &str) -> String {
    let text = text
        .replace('\u{fe0f}', "")
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut text = text.as_str();
    while let Some(index) = text.rfind('\n') {
        if !text[index + 1..].trim().is_empty() {
            break;
        }
        text = &text[..index];
    }
    escape_angles(text)
}

/// Escape `<` and `>` outside CommonMark code spans: a run of N backticks closes at the next
/// run of exactly N; a run with no closer is literal text.
fn escape_angles(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(ch) = rest.chars().next() {
        match ch {
            '`' => {
                let run = backtick_run(rest);
                let span = closing_run(&rest[run..], run).map_or(run, |end| run + end);
                out.push_str(&rest[..span]);
                rest = &rest[span..];
            }
            '<' | '>' => {
                out.push('\\');
                out.push(ch);
                rest = &rest[1..];
            }
            _ => {
                out.push(ch);
                rest = &rest[ch.len_utf8()..];
            }
        }
    }
    out
}

fn backtick_run(text: &str) -> usize {
    text.bytes().take_while(|byte| *byte == b'`').count()
}

/// The byte offset just past the next run of exactly `length` backticks in `text`.
fn closing_run(text: &str, length: usize) -> Option<usize> {
    let mut offset = 0;
    while let Some(found) = text[offset..].find('`') {
        let start = offset + found;
        let run = backtick_run(&text[start..]);
        if run == length {
            return Some(start + run);
        }
        offset = start + run;
    }
    None
}
