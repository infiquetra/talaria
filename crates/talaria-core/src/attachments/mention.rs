//! The `@` query at the caret and the list that follows it.

use std::time::{Duration, Instant};

use hermes_contract::schemas::CompletionItem;

/// The `@` word under the caret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionQuery {
    /// Byte offset of `@`.
    pub start: usize,
    /// The word, including `@`.
    pub word: String,
}

/// Tab completes; Enter inserts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accept {
    /// Tab.
    Complete,
    /// Enter.
    Insert,
}

/// The draft after one acceptance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accepted {
    /// The draft with the query replaced.
    pub draft: String,
    /// Caret byte, at the end of the insert.
    pub caret: usize,
    /// Whether the list stays open.
    pub keep_open: bool,
}

/// One completion the list is waiting on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionRequest {
    /// The word sent as `word`.
    pub word: String,
    /// Generation of this edit.
    pub generation: u64,
}

/// Where the list is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MentionPhase {
    /// No query.
    Closed,
    /// Waiting out the debounce.
    Waiting { since: Instant },
    /// A request has been fired.
    Requested { since: Instant },
    /// Rows are showing.
    Items,
    /// The gateway returned nothing.
    Empty,
    /// The gateway failed. The string is the plain reason.
    Unavailable(String),
    /// Esc dismissed this word.
    Dismissed { word: String },
}

/// List state. Time is only the `Instant` arguments.
pub struct MentionState {
    phase: MentionPhase,
    query: Option<MentionQuery>,
    generation: u64,
    items: Vec<CompletionItem>,
    selected: usize,
    offset: usize,
    searching_shown: bool,
}

impl Default for MentionState {
    fn default() -> Self {
        Self::new()
    }
}

impl MentionState {
    /// Closed, generation 0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            phase: MentionPhase::Closed,
            query: None,
            generation: 0,
            items: Vec::new(),
            selected: 0,
            offset: 0,
            searching_shown: false,
        }
    }

    /// Start or restart the wait after an edit.
    pub fn on_edit(&mut self, draft: &str, caret: usize, now: Instant) {
        self.generation = self.generation.wrapping_add(1);
        let query = mention_at(draft, caret);
        let dismissed = matches!((&self.phase, &query),
            (MentionPhase::Dismissed { word }, Some(query)) if word == &query.word);
        self.items.clear();
        self.selected = 0;
        self.offset = 0;
        self.searching_shown = false;
        self.phase = match &query {
            Some(query) if dismissed => MentionPhase::Dismissed {
                word: query.word.clone(),
            },
            Some(_) => MentionPhase::Waiting { since: now },
            None => MentionPhase::Closed,
        };
        self.query = query;
    }

    /// Fire the request when the wait is over.
    pub fn tick(&mut self, now: Instant) -> Option<CompletionRequest> {
        match self.phase {
            MentionPhase::Waiting { since }
                if now.saturating_duration_since(since) >= Duration::from_millis(60) =>
            {
                self.request_now(now)
            }
            MentionPhase::Requested { since }
                if now.saturating_duration_since(since) >= Duration::from_millis(150) =>
            {
                self.searching_shown = true;
                None
            }
            _ => None,
        }
    }

    /// When the loop should tick next.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Instant> {
        match self.phase {
            MentionPhase::Waiting { since } => since.checked_add(Duration::from_millis(60)),
            MentionPhase::Requested { since } if !self.searching_shown => {
                since.checked_add(Duration::from_millis(150))
            }
            _ => None,
        }
    }

    /// Store a reply for `generation`. A stale generation is ignored.
    pub fn on_items(&mut self, generation: u64, items: Vec<CompletionItem>) {
        if generation != self.generation || !self.is_open() {
            return;
        }
        self.items = filter_items(items);
        self.selected = 0;
        self.offset = 0;
        self.phase = if self.items.is_empty() {
            MentionPhase::Empty
        } else {
            MentionPhase::Items
        };
    }

    /// Record a failure for `generation`.
    pub fn on_failed(&mut self, generation: u64, reason: String) {
        if generation != self.generation || !self.is_open() {
            return;
        }
        self.items.clear();
        self.phase = MentionPhase::Unavailable(super::sanitize(&reason));
    }

    /// A query is present and this word was not dismissed.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.query.is_some()
            && !matches!(
                self.phase,
                MentionPhase::Closed | MentionPhase::Dismissed { .. }
            )
    }

    /// The list is showing rows.
    #[must_use]
    pub fn has_items(&self) -> bool {
        self.is_open() && !self.items.is_empty() && matches!(self.phase, MentionPhase::Items)
    }

    /// Move the selection down, wrapping, and keep it in the window.
    pub fn next(&mut self) {
        if self.has_items() {
            self.selected = (self.selected + 1) % self.items.len();
            self.scroll();
        }
    }

    /// Move the selection up, wrapping, and keep it in the window.
    pub fn prev(&mut self) {
        if self.has_items() {
            self.selected = if self.selected == 0 {
                self.items.len() - 1
            } else {
                self.selected - 1
            };
            self.scroll();
        }
    }

    /// Dismiss the list for the current word.
    pub fn dismiss(&mut self) {
        if let Some(query) = &self.query {
            self.phase = MentionPhase::Dismissed {
                word: query.word.clone(),
            };
            self.items.clear();
        }
    }

    /// The selected row, when the list has rows.
    #[must_use]
    pub fn selected(&self) -> Option<&CompletionItem> {
        self.has_items()
            .then(|| self.items.get(self.selected))
            .flatten()
    }

    /// Rows, status and footer for one draw.
    #[must_use]
    pub fn view(&self, now: Instant) -> MentionView {
        let rows = if self.has_items() {
            self.items
                .iter()
                .skip(self.offset)
                .take(8)
                .map(|item| {
                    (
                        super::sanitize(if item.display.is_empty() {
                            &item.text
                        } else {
                            &item.display
                        }),
                        super::sanitize(&item.meta),
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        let status = match &self.phase {
            MentionPhase::Requested { since }
                if now.saturating_duration_since(*since) >= Duration::from_millis(150) =>
            {
                Some("Searching…".to_string())
            }
            MentionPhase::Empty => self
                .query
                .as_ref()
                .map(|query| format!("No matches for {}", super::sanitize(&query.word))),
            MentionPhase::Unavailable(reason) => Some(format!("Completion unavailable: {reason}")),
            _ => None,
        };
        MentionView {
            rows,
            selected: self.has_items().then_some(self.selected - self.offset),
            status,
            footer: "tab complete · enter insert · esc close",
        }
    }

    fn scroll(&mut self) {
        if self.selected < self.offset {
            self.offset = self.selected;
        }
        if self.selected >= self.offset + 8 {
            self.offset = self.selected + 1 - 8;
        }
    }

    /// Close the list and invalidate replies already in flight.
    pub fn close(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.phase = MentionPhase::Closed;
        self.query = None;
        self.items.clear();
    }

    /// Request the current waiting query immediately after completing a prefix.
    pub fn request_now(&mut self, now: Instant) -> Option<CompletionRequest> {
        if !matches!(self.phase, MentionPhase::Waiting { .. }) {
            return None;
        }
        let query = self.query.as_ref()?;
        self.phase = MentionPhase::Requested { since: now };
        self.searching_shown = false;
        Some(CompletionRequest {
            word: query.word.clone(),
            generation: self.generation,
        })
    }

    /// The word currently associated with the list.
    #[must_use]
    pub fn query(&self) -> Option<&MentionQuery> {
        self.query.as_ref()
    }
}

/// What the list section draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionView {
    /// Display and meta of each visible row.
    pub rows: Vec<(String, String)>,
    /// Selected index inside `rows`.
    pub selected: Option<usize>,
    /// Status line, when one is showing.
    pub status: Option<String>,
    /// The fixed footer.
    pub footer: &'static str,
}

/// The `@` query at `caret`, when the caret ends the word.
#[must_use]
pub fn mention_at(draft: &str, caret: usize) -> Option<MentionQuery> {
    if draft.starts_with('/') || !draft.is_char_boundary(caret) {
        return None;
    }
    let before = draft.get(..caret)?;
    if draft
        .get(caret..)?
        .chars()
        .next()
        .is_some_and(|ch| !ch.is_whitespace())
    {
        return None;
    }
    let start = before
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_whitespace())
        .map_or(0, |(at, ch)| at + ch.len_utf8());
    let word = &before[start..];
    word.starts_with('@').then(|| MentionQuery {
        start,
        word: word.to_string(),
    })
}

/// Drop profile rows. Keep the gateway's order.
#[must_use]
pub fn filter_items(items: Vec<CompletionItem>) -> Vec<CompletionItem> {
    items
        .into_iter()
        .filter(|item| item.text.contains(':') || matches!(item.text.as_str(), "@diff" | "@staged"))
        .collect()
}

/// Replace the query with `item`, quoting the value.
#[must_use]
pub fn accept(draft: &str, query: &MentionQuery, item: &CompletionItem, how: Accept) -> Accepted {
    let end = query.start.saturating_add(query.word.len());
    if draft.get(query.start..end) != Some(query.word.as_str()) {
        return Accepted {
            draft: draft.to_string(),
            caret: end.min(draft.len()),
            keep_open: false,
        };
    }
    let prefix = item.text.ends_with(':');
    let folder = item
        .text
        .strip_prefix("@folder:")
        .filter(|value| !value.is_empty());
    let drill = how == Accept::Complete
        && folder.is_some_and(|value| !value.chars().any(char::is_whitespace));
    let mut insert = if drill {
        format!("@{}/", folder.unwrap_or_default().trim_end_matches('/'))
    } else if let Some((kind, value)) = item.text.split_once(':') {
        format!("{kind}:{}", format_ref_value(value))
    } else {
        item.text.clone()
    };
    let keep_open = prefix || drill;
    if !keep_open && !draft[end..].chars().next().is_some_and(char::is_whitespace) {
        insert.push(' ');
    }
    let caret = query.start + insert.len();
    let mut result = draft.to_string();
    result.replace_range(query.start..end, &insert);
    Accepted {
        draft: result,
        caret,
        keep_open,
    }
}

/// Quote a reference value the way the gateway expects.
#[must_use]
pub fn format_ref_value(value: &str) -> String {
    if value
        .chars()
        .any(|ch| ch.is_whitespace() || "()[]{}<>\"'`".contains(ch))
    {
        for quote in ['`', '"', '\''] {
            if !value.contains(quote) {
                return format!("{quote}{value}{quote}");
            }
        }
    }
    value.to_string()
}

/// Built-in `@` references in `text`.
#[must_use]
pub fn count_references(text: &str) -> usize {
    let mut count = 0;
    let mut consumed = 0;
    for (at, ch) in text.char_indices() {
        if at < consumed
            || ch != '@'
            || text[..at]
                .chars()
                .next_back()
                .is_some_and(|ch| word_char(ch) || ch == '/')
        {
            continue;
        }
        let tail = &text[at + 1..];
        if let Some(simple) = ["diff", "staged"].into_iter().find(|simple| {
            tail.starts_with(simple) && !tail[simple.len()..].chars().next().is_some_and(word_char)
        }) {
            count += 1;
            consumed = at + 1 + simple.len();
            continue;
        }
        let Some((kind, value)) = tail.split_once(':') else {
            continue;
        };
        if !matches!(kind, "file" | "folder" | "git" | "url")
            || value.is_empty()
            || value.starts_with(char::is_whitespace)
        {
            continue;
        }
        let mut value_len = value.find(char::is_whitespace).unwrap_or(value.len());
        if let Some(quote) = value
            .chars()
            .next()
            .filter(|ch| ['`', '"', '\''].contains(ch))
            && let Some(end) = value[1..]
                .find(quote)
                .map(|end| end + 1)
                .filter(|end| *end > 1 && !value[1..*end].contains('\n'))
        {
            value_len = end + 1;
            let suffix = &value[value_len..];
            if let Some(range) = suffix.strip_prefix(':') {
                let digits = range.bytes().take_while(u8::is_ascii_digit).count();
                if digits > 0 {
                    value_len += 1 + digits;
                    if let Some(last) = range[digits..].strip_prefix('-') {
                        let more = last.bytes().take_while(u8::is_ascii_digit).count();
                        if more > 0 {
                            value_len += 1 + more;
                        }
                    }
                }
            }
        }
        count += 1;
        consumed = at + 1 + kind.len() + 1 + value_len;
    }
    count
}

fn word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// The notice when a message has more references than the gateway expands.
#[must_use]
pub fn reference_notice(count: usize) -> Option<&'static str> {
    (count > 16).then_some(
        "Hermes expands at most 16 @ references per message; the rest arrive as plain text.",
    )
}
