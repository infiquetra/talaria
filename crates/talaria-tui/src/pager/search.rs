//! Literal search over one session's cells. Issue 222.
//!
//! A query is literal text, never a pattern. It is case-sensitive when it holds an uppercase
//! letter and folds otherwise. Offsets count characters, never bytes, so a cell that folds to a
//! different byte length still reports the position a reader sees.

use std::borrow::Cow;
use std::collections::HashMap;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use talaria_core::session::{Cell, CellId, PromptState, Settlement};

use super::render::PagerTheme;
use crate::history::{CellLines, sanitize};

/// The text and the folded text of one final cell.
#[derive(Debug)]
struct Entry {
    /// The text with each character folded.
    folded: String,
    /// Whether the folded text is plain ASCII, so a byte offset is a character offset.
    folded_ascii: bool,
}

/// Search text of final cells, kept by the application across openings of the transcript.
///
/// A final cell never changes, so its entry stays valid for the life of its epoch. A new epoch
/// drops the old entries, because cell ids restart with every session state.
#[derive(Debug, Default)]
pub struct SearchIndex {
    built: u64,
    epoch: u64,
    entries: HashMap<CellId, Entry>,
    uncached: Option<Entry>,
}

impl SearchIndex {
    /// An empty index.
    #[must_use]
    pub fn new() -> Self {
        Self {
            built: 0,
            epoch: 0,
            entries: HashMap::new(),
            uncached: None,
        }
    }

    /// Drop every stored cell.
    pub fn clear(&mut self) {
        self.built = 0;
        self.epoch = 0;
        self.entries.clear();
        self.uncached = None;
    }

    /// How many final-cell entries have been built since [`Self::new`].
    #[doc(hidden)]
    #[must_use]
    pub fn build_count(&self) -> u64 {
        self.built
    }

    /// The entry of one cell, built on first use.
    fn entry(&mut self, epoch: u64, cell: &Cell, other: &dyn CellLines) -> &Entry {
        if self.epoch != epoch {
            self.entries.clear();
            self.epoch = epoch;
        }
        let Some(id) = cell_id(cell) else {
            let folded = folded_search_text(cell, other);
            return self.uncached.insert(Entry {
                folded_ascii: folded.is_ascii(),
                folded,
            });
        };
        if let std::collections::hash_map::Entry::Vacant(slot) = self.entries.entry(id) {
            let folded = folded_search_text(cell, other);
            slot.insert(Entry {
                folded_ascii: folded.is_ascii(),
                folded,
            });
            self.built += 1;
        }
        self.entries
            .get(&id)
            .unwrap_or_else(|| unreachable!("the entry was just built"))
    }
}

/// The stable identity of a cell, when its variant carries one.
fn cell_id(cell: &Cell) -> Option<CellId> {
    match cell {
        Cell::User(inner) => Some(inner.id),
        Cell::Assistant(inner) => Some(inner.id),
        Cell::Reasoning(inner) => Some(inner.id),
        Cell::Tool(inner) => Some(inner.id),
        Cell::Notice(inner) => Some(inner.id),
        Cell::Prompt(inner) => Some(inner.id),
        Cell::Shell(_) | Cell::Side(_) | Cell::HermesCommand(_) | Cell::Background(_) => None,
    }
}

/// The text one cell contributes to search, with labels left out.
///
/// The fields are joined with a newline, in the order the specification gives. The labels the
/// screen draws are not part of the text, so a search for `args:` finds nothing.
#[must_use]
pub fn cell_search_text(cell: &Cell, other: &dyn CellLines) -> String {
    let parts = parts_of(cell, other);
    let mut fields = parts
        .iter()
        .map(|part| sanitize_field(part))
        .collect::<Vec<_>>();
    if fields.len() == 1 {
        return fields.remove(0).into_owned();
    }
    fields.join("\n")
}

/// The fields of one cell, in the order the search reads them.
fn parts_of<'a>(cell: &'a Cell, other: &'a dyn CellLines) -> Vec<Cow<'a, str>> {
    let mut parts: Vec<Cow<'_, str>> = Vec::new();
    // The wildcard arm keeps a cell kind added later from breaking the build.
    #[allow(unreachable_patterns)]
    match cell {
        Cell::User(inner) => parts.push(Cow::Borrowed(inner.text.as_str())),
        Cell::Assistant(inner) => parts.push(Cow::Borrowed(inner.text.as_str())),
        Cell::Reasoning(inner) => parts.push(Cow::Borrowed(inner.text.as_str())),
        Cell::Tool(inner) => {
            parts.push(Cow::Borrowed(inner.name.as_str()));
            if let Some(context) = &inner.context {
                parts.push(Cow::Borrowed(context.as_str()));
            }
            if let Some(args) = &inner.args
                && !args.as_object().is_some_and(serde_json::Map::is_empty)
            {
                parts.push(Cow::Owned(pretty_json(args)));
            }
            if let Some(summary) = &inner.summary {
                parts.push(Cow::Borrowed(summary.as_str()));
            }
            if let Some(result) = &inner.result {
                parts.push(Cow::Borrowed(result.as_str()));
            }
            if let Some(diff) = &inner.diff {
                parts.push(Cow::Borrowed(diff.as_str()));
            }
        }
        Cell::Notice(inner) => parts.push(Cow::Borrowed(inner.text.as_str())),
        Cell::Prompt(inner) => {
            if let Some(command) = &inner.command {
                parts.push(Cow::Borrowed(command.as_str()));
            }
            if let Some(description) = &inner.description {
                parts.push(Cow::Borrowed(description.as_str()));
            }
            if let Some(question) = &inner.question {
                parts.push(Cow::Borrowed(question.as_str()));
            }
            for choice in &inner.choices {
                parts.push(Cow::Borrowed(choice.as_str()));
            }
            match &inner.state {
                PromptState::Answering(answer) => parts.push(Cow::Borrowed(answer.as_str())),
                PromptState::Settled(Settlement::Answered(answer)) => {
                    parts.push(Cow::Borrowed(answer.as_str()));
                }
                PromptState::Open | PromptState::Settled(Settlement::Elsewhere) => {}
            }
        }
        _ => {
            for line in other.lines(cell, u16::MAX) {
                parts.push(Cow::Owned(plain_text(&line)));
            }
        }
    }
    parts
}

/// The folded search text of one cell, built in one pass.
///
/// The fields are kept as they are. The screen shows a stand-in for a control character, but a
/// query can never hold one, so the match does not depend on it, and the highlight is placed by
/// the rendered lines rather than by these offsets.
fn folded_search_text(cell: &Cell, other: &dyn CellLines) -> String {
    let parts = parts_of(cell, other);
    let total: usize =
        parts.iter().map(|part| part.len()).sum::<usize>() + parts.len().saturating_sub(1);
    let mut out = String::with_capacity(total);
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        if part.is_ascii() {
            let start = out.len();
            out.push_str(part);
            out[start..].make_ascii_lowercase();
        } else {
            out.extend(part.chars().map(fold));
        }
    }
    out
}

/// The raw search text of one cell, used by a case-sensitive query.
fn raw_search_text(cell: &Cell, other: &dyn CellLines) -> String {
    parts_of(cell, other).join("\n")
}

/// The arguments of a tool call, as the screen shows them.
fn pretty_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// The text of one line, with the span contents joined.
fn plain_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// One field of a cell, with the control characters made visible. Newlines stay newlines.
///
/// Clean text is borrowed, so a long transcript is not copied twice.
fn sanitize_field(text: &str) -> Cow<'_, str> {
    if is_clean(text) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    for (index, piece) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&sanitize(piece));
    }
    Cow::Owned(out)
}

/// Whether the text holds no control character and no direction mark.
fn is_clean(text: &str) -> bool {
    text.is_ascii() && !text.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
}

/// One source character, folded when the lowercase form is also one character.
#[must_use]
pub fn fold(c: char) -> char {
    let mut lowered = c.to_lowercase();
    match (lowered.next(), lowered.next()) {
        (Some(single), None) => single,
        _ => c,
    }
}

/// The text with every character folded, one character in and one character out.
fn fold_text(text: &str) -> String {
    if text.is_ascii() {
        text.to_ascii_lowercase()
    } else {
        text.chars().map(fold).collect()
    }
}

/// A literal query. An uppercase letter makes the match case-sensitive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    /// The query text, never containing a newline.
    pub text: String,
    /// True when `text` contains an uppercase letter.
    pub case_sensitive: bool,
}

impl Query {
    /// Smart case: uppercase in `text` turns case sensitivity on.
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            case_sensitive: text.chars().any(char::is_uppercase),
        }
    }
}

/// One match. Offsets count characters in the cell's search text, never bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Match {
    /// Index into the cell slice.
    pub cell_index: usize,
    /// Zero-based match inside that cell.
    pub ordinal_in_cell: usize,
    /// Character index where the match starts.
    pub char_start: usize,
    /// Character length of the match.
    pub char_len: usize,
}

/// Every non-overlapping match, ordered by cell then character offset.
#[must_use]
pub fn find_all(
    cells: &[Cell],
    epoch: u64,
    index: &mut SearchIndex,
    query: &Query,
    other: &dyn CellLines,
) -> Vec<Match> {
    let mut found = Vec::new();
    if query.text.is_empty() {
        return found;
    }
    let needle = if query.case_sensitive {
        query.text.clone()
    } else {
        fold_text(&query.text)
    };
    let needle_chars = query.text.chars().count();
    for (cell_index, cell) in cells.iter().enumerate() {
        if cell.is_final() {
            let entry = index.entry(epoch, cell, other);
            if query.case_sensitive {
                let raw = raw_search_text(cell, other);
                if raw.is_ascii() {
                    push_ascii(&mut found, cell_index, &raw, &needle, needle_chars);
                } else {
                    push_scanned(
                        &mut found,
                        cell_index,
                        scan(&raw, false, &needle),
                        needle_chars,
                    );
                }
            } else if entry.folded_ascii {
                push_ascii(&mut found, cell_index, &entry.folded, &needle, needle_chars);
            } else {
                push_scanned(
                    &mut found,
                    cell_index,
                    scan(&entry.folded, false, &needle),
                    needle_chars,
                );
            }
        } else {
            let raw = cell_search_text(cell, other);
            let text = if query.case_sensitive {
                raw
            } else {
                fold_text(&raw)
            };
            if text.is_ascii() {
                push_ascii(&mut found, cell_index, &text, &needle, needle_chars);
            } else {
                push_scanned(
                    &mut found,
                    cell_index,
                    scan(&text, false, &needle),
                    needle_chars,
                );
            }
        }
    }
    found
}

/// ASCII matches go straight into `found`. A byte offset is a character offset.
fn push_ascii(
    found: &mut Vec<Match>,
    cell_index: usize,
    text: &str,
    needle: &str,
    needle_chars: usize,
) {
    found.reserve(text.len() / needle.len().max(1));
    for (ordinal, (start, _)) in text.match_indices(needle).enumerate() {
        found.push(Match {
            cell_index,
            ordinal_in_cell: ordinal,
            char_start: start,
            char_len: needle_chars,
        });
    }
}

/// Non-ASCII matches, already converted to character offsets.
fn push_scanned(
    found: &mut Vec<Match>,
    cell_index: usize,
    hits: Vec<(usize, usize)>,
    needle_chars: usize,
) {
    found.reserve(hits.len());
    for (ordinal, (char_start, _)) in hits.into_iter().enumerate() {
        found.push(Match {
            cell_index,
            ordinal_in_cell: ordinal,
            char_start,
            char_len: needle_chars,
        });
    }
}

/// The non-overlapping matches of `needle` in `text`, as character offsets.
fn scan(text: &str, ascii: bool, needle: &str) -> Vec<(usize, usize)> {
    let mut bytes: Vec<usize> = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find(needle) {
        let start = from + found;
        bytes.push(start);
        from = start + needle.len();
    }
    if bytes.is_empty() {
        return Vec::new();
    }
    let starts = to_char_offsets(text, ascii, &bytes);
    let length = text[bytes[0]..bytes[0] + needle.len()].chars().count();
    starts.into_iter().map(|start| (start, length)).collect()
}

/// The character offsets of the given byte offsets, in one pass over the text.
fn to_char_offsets(text: &str, ascii: bool, bytes: &[usize]) -> Vec<usize> {
    if ascii {
        return bytes.to_vec();
    }
    let mut out = Vec::with_capacity(bytes.len());
    let mut chars = 0usize;
    let mut at = 0usize;
    for target in bytes {
        while at < *target {
            let piece = text[at..].chars().next().unwrap_or(' ');
            at += piece.len_utf8();
            chars += 1;
        }
        out.push(chars);
    }
    out
}

/// The occurrences of the query in one rendered line, as character offsets.
#[must_use]
pub fn find_in_line(line: &Line<'_>, query: &Query) -> Vec<(usize, usize)> {
    if query.text.is_empty() {
        return Vec::new();
    }
    let text = plain_text(line);
    let needle = if query.case_sensitive {
        query.text.clone()
    } else {
        fold_text(&query.text)
    };
    let hay = if query.case_sensitive {
        text
    } else {
        fold_text(&text)
    };
    let ascii = hay.is_ascii();
    scan(&hay, ascii, &needle)
}

/// Copy `line`, painting matches. The text is unchanged.
///
/// `current` is the character offset of the occurrence that is the current one. An occurrence
/// that is not the current one is painted with the ordinary match style.
#[must_use]
pub fn highlight_line(
    line: &Line<'static>,
    query: &Query,
    current: Option<usize>,
    theme: &PagerTheme,
) -> Line<'static> {
    let text = plain_text(line);
    if query.text.is_empty() {
        return line.clone();
    }
    let needle = if query.case_sensitive {
        query.text.clone()
    } else {
        fold_text(&query.text)
    };
    let hay = if query.case_sensitive {
        text.clone()
    } else {
        fold_text(&text)
    };
    let ascii = hay.is_ascii();
    let hits = scan(&hay, ascii, &needle);
    if hits.is_empty() {
        return line.clone();
    }

    // The base style of every character, taken from the span it came from.
    let mut base: Vec<Style> = Vec::with_capacity(text.chars().count());
    for span in &line.spans {
        for _ in span.content.chars() {
            base.push(span.style);
        }
    }
    let mut hit_at: Vec<Option<usize>> = vec![None; base.len()];
    for (index, (start, length)) in hits.iter().enumerate() {
        for offset in *start..*start + *length {
            if offset < hit_at.len() {
                hit_at[offset] = Some(index);
            }
        }
    }

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut buffer = String::new();
    let mut style: Option<Style> = None;
    for (offset, character) in text.chars().enumerate() {
        let painted = match hit_at[offset] {
            Some(index) => {
                let start = hits[index].0;
                if current == Some(start) {
                    theme.search_current
                } else {
                    theme.search_match
                }
            }
            None => base[offset],
        };
        if style != Some(painted) {
            if !buffer.is_empty() {
                spans.push(Span::styled(
                    std::mem::take(&mut buffer),
                    style.unwrap_or_default(),
                ));
            }
            style = Some(painted);
        }
        buffer.push(character);
    }
    if !buffer.is_empty() {
        spans.push(Span::styled(buffer, style.unwrap_or_default()));
    }
    let mut out = Line::from(spans);
    out.style = line.style;
    out.alignment = line.alignment;
    out
}
