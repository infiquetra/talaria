//! The card stack and its transient input state.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use ratatui::{buffer::Buffer, layout::Rect, text::Line};
use talaria_core::session::{PromptCell, PromptKey, PromptKind, PromptState};

use super::{
    KeyOutcome, MaskedValue, MaskedView, PromptIntent, display_choices, multi_select_answer,
};
use crate::history::{sanitize, wrap_lines};
use crate::keys::{Key, KeyCode, KeyKind};
use crate::live::LiveSection;

const LIMIT: usize = 16_384;

#[derive(Clone, PartialEq, Eq)]
enum Identity {
    Prompt(PromptKey),
    Masked(String),
}

enum Content {
    Prompt(PromptCell),
    Masked(MaskedView),
}

struct Card {
    id: Identity,
    content: Content,
    selected: usize,
    field: bool,
    text: String,
    ticks: Vec<usize>,
    secret: MaskedValue,
    secret_chars: usize,
    too_long: Option<usize>,
    sent: bool,
}

impl Card {
    fn new(id: Identity, content: Content) -> Self {
        let field = matches!(&content, Content::Prompt(cell) if cell.kind == PromptKind::Clarify && cell.choices.is_empty());
        Self {
            id,
            content,
            selected: 0,
            field,
            text: String::new(),
            ticks: Vec::new(),
            secret: MaskedValue::new(),
            secret_chars: 0,
            too_long: None,
            sent: false,
        }
    }

    fn session(&self) -> Option<&str> {
        match &self.content {
            Content::Prompt(cell) => Some(cell.key.session_id.as_str()),
            Content::Masked(view) => view.session_id.as_deref(),
        }
    }

    fn answering(&self) -> bool {
        self.sent
            || matches!(&self.content, Content::Prompt(cell) if !matches!(cell.state, PromptState::Open))
    }

    fn insert(&mut self, text: &str) {
        let text = text.split(['\r', '\n']).next().unwrap_or_default();
        if matches!(self.content, Content::Masked(_)) {
            for ch in text.chars() {
                if self.secret_chars == LIMIT {
                    self.too_long = Some(LIMIT);
                    break;
                }
                self.secret.push(ch);
                self.secret_chars += 1;
            }
        } else if self.field {
            self.text.push_str(text);
        }
    }

    fn masked_answer(&mut self, decline: bool) -> PromptIntent {
        let value = std::mem::replace(&mut self.secret, MaskedValue::new());
        self.secret_chars = 0;
        self.sent = true;
        let request_id = match &self.content {
            Content::Masked(view) => view.request_id.clone(),
            _ => String::new(),
        };
        PromptIntent::Masked {
            request_id,
            value: if decline {
                drop(value);
                MaskedValue::new()
            } else {
                value
            },
        }
    }

    fn answer(&mut self, value: String) -> Option<PromptIntent> {
        let Content::Prompt(cell) = &self.content else {
            return None;
        };
        if cell.kind == PromptKind::Approval {
            let note = (!self.text.trim().is_empty()).then(|| self.text.clone());
            self.sent = true;
            Some(PromptIntent::Approve {
                key: cell.key.clone(),
                choice: value,
                note,
            })
        } else {
            let length = value.chars().count();
            if length > LIMIT {
                self.too_long = Some(length);
                return None;
            }
            self.sent = true;
            Some(PromptIntent::Clarify {
                key: cell.key.clone(),
                answer: value,
            })
        }
    }
}

/// Retained presentation state; protocol state remains in the supplied prompt cells.
pub struct PromptStack {
    cards: Vec<Card>,
    hidden: HashSet<PromptKey>,
    foreground: Option<String>,
    draft_empty: bool,
    focused_at: Option<Instant>,
    stopping: HashSet<Option<String>>,
}

impl Default for PromptStack {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptStack {
    pub fn new() -> Self {
        Self {
            cards: Vec::new(),
            hidden: HashSet::new(),
            foreground: None,
            draft_empty: false,
            focused_at: None,
            stopping: HashSet::new(),
        }
    }

    pub fn sync(
        &mut self,
        foreground: Option<&str>,
        prompts: &[&PromptCell],
        masked: &[MaskedView],
        draft_empty: bool,
        now: Instant,
    ) {
        let old_first = self.cards.first().map(|card| card.id.clone());
        self.hidden
            .retain(|key| prompts.iter().any(|cell| cell.key == *key));
        let mut old = std::mem::take(&mut self.cards);
        let incoming = masked
            .iter()
            .map(|view| {
                (
                    Identity::Masked(view.request_id.clone()),
                    Content::Masked(view.clone()),
                )
            })
            .chain(
                prompts
                    .iter()
                    .filter(|cell| !self.hidden.contains(&cell.key))
                    .map(|cell| {
                        (
                            Identity::Prompt(cell.key.clone()),
                            Content::Prompt((*cell).clone()),
                        )
                    }),
            );
        for (id, content) in incoming {
            let card = if let Some(index) = old.iter().position(|card| card.id == id) {
                let mut card = old.remove(index);
                // A failed answer reopens the same card without discarding typed state.
                if matches!((&card.content, &content), (Content::Prompt(before), Content::Prompt(after))
                    if matches!(before.state, PromptState::Answering(_)) && after.state == PromptState::Open)
                {
                    card.sent = false;
                }
                card.content = content;
                card
            } else {
                Card::new(id, content)
            };
            self.cards.push(card);
        }
        if draft_empty
            && (!self.draft_empty || old_first != self.cards.first().map(|card| card.id.clone()))
        {
            self.focused_at = Some(now);
        }
        self.draft_empty = draft_empty;
        self.foreground = foreground.map(str::to_owned);
        self.stopping.retain(|session| {
            self.cards
                .iter()
                .any(|card| card.session() == session.as_deref())
        });
    }

    fn ready(&self, now: Instant) -> bool {
        self.focused_at
            .is_some_and(|at| now.saturating_duration_since(at) >= Duration::from_millis(250))
    }

    pub fn handle_key(&mut self, key: Key, now: Instant) -> KeyOutcome {
        if !self.draft_empty || self.cards.is_empty() {
            return KeyOutcome::NotHandled;
        }
        let mut result = KeyOutcome::Consumed {
            intent: None,
            interrupt_requested: false,
            notice: None,
        };
        if key.kind == KeyKind::Release {
            return result;
        }
        let control_c = key.mods.ctrl && matches!(key.code, KeyCode::Char('c' | 'C'));
        let session = self.cards[0].session().map(str::to_owned);
        if self.stopping.contains(&session) {
            return result;
        }
        if control_c {
            let card = &mut self.cards[0];
            let intent = match &card.content {
                Content::Masked(_) if !card.sent => Some(card.masked_answer(true)),
                Content::Prompt(cell) if cell.kind == PromptKind::Clarify && !card.answering() => {
                    card.answer(String::new())
                }
                _ => {
                    card.text.clear();
                    None
                }
            };
            self.stopping.insert(session);
            return KeyOutcome::Consumed {
                intent,
                interrupt_requested: true,
                notice: None,
            };
        }
        if !self.ready(now) || self.cards[0].answering() {
            return result;
        }
        if key.mods.ctrl || key.mods.alt || key.mods.super_key || key.mods.other {
            return result;
        }
        let card = &mut self.cards[0];
        let intent = match &card.content {
            Content::Masked(_) => match key.code {
                KeyCode::Enter => Some(card.masked_answer(false)),
                KeyCode::Esc => Some(card.masked_answer(true)),
                KeyCode::Backspace => {
                    card.secret.pop();
                    card.secret_chars = card.secret_chars.saturating_sub(1);
                    card.too_long = None;
                    None
                }
                KeyCode::Char(ch) if !ch.is_control() => {
                    card.insert(ch.encode_utf8(&mut [0; 4]));
                    None
                }
                _ => None,
            },
            Content::Prompt(cell) if cell.kind == PromptKind::Approval => {
                let choices: Vec<String> = display_choices(&cell.choices)
                    .into_iter()
                    .map(str::to_owned)
                    .collect();
                if card.field {
                    match key.code {
                        KeyCode::Tab | KeyCode::Esc => {
                            card.field = false;
                            None
                        }
                        KeyCode::Enter => choices
                            .get(card.selected)
                            .cloned()
                            .and_then(|value| card.answer(value)),
                        KeyCode::Backspace => {
                            card.text.pop();
                            None
                        }
                        KeyCode::Char(ch) if !ch.is_control() => {
                            card.text.push(ch);
                            None
                        }
                        _ => None,
                    }
                } else {
                    match key.code {
                        KeyCode::Up => {
                            card.selected = card.selected.saturating_sub(1);
                            None
                        }
                        KeyCode::Down => {
                            card.selected =
                                (card.selected + 1).min(choices.len().saturating_sub(1));
                            None
                        }
                        KeyCode::Tab => {
                            card.field = true;
                            None
                        }
                        KeyCode::Enter => choices
                            .get(card.selected)
                            .cloned()
                            .and_then(|value| card.answer(value)),
                        KeyCode::Char(ch @ '1'..='9') => choices
                            .get(ch as usize - '1' as usize)
                            .cloned()
                            .and_then(|value| card.answer(value)),
                        KeyCode::Esc if choices.iter().any(|value| value == "deny") => {
                            card.answer("deny".into())
                        }
                        KeyCode::Esc => {
                            self.hidden.insert(cell.key.clone());
                            self.cards.remove(0);
                            result = KeyOutcome::Consumed {
                                intent: None,
                                interrupt_requested: false,
                                notice: Some(
                                    "closed without an answer: Hermes did not offer deny".into(),
                                ),
                            };
                            None
                        }
                        _ => None,
                    }
                }
            }
            Content::Prompt(_) => clarify_key(card, key.code),
        };
        if let KeyOutcome::Consumed { intent: output, .. } = &mut result {
            *output = intent;
        }
        result
    }

    pub fn handle_paste(&mut self, text: &str, now: Instant) -> KeyOutcome {
        if !self.draft_empty || self.cards.is_empty() {
            return KeyOutcome::NotHandled;
        }
        if self.ready(now) {
            let card = &mut self.cards[0];
            if !card.answering() && !self.stopping.contains(&card.session().map(str::to_owned)) {
                card.insert(text);
            }
        }
        KeyOutcome::Consumed {
            intent: None,
            interrupt_requested: false,
            notice: None,
        }
    }

    pub fn stop_cleared(&mut self) {
        self.stopping.clear();
    }

    pub fn section(&self) -> Option<PromptSection<'_>> {
        (!self.cards.is_empty()).then_some(PromptSection { stack: self })
    }
}

fn clarify_key(card: &mut Card, key: KeyCode) -> Option<PromptIntent> {
    let Content::Prompt(cell) = &card.content else {
        return None;
    };
    let choices = cell.choices.clone();
    let multi = cell.multi_select && !choices.is_empty();
    let free = choices.is_empty();
    let answer = |card: &Card| {
        if multi {
            multi_select_answer(&choices, &card.ticks, Some(&card.text))
        } else {
            card.text.clone()
        }
    };
    if card.field {
        return match key {
            KeyCode::Esc if free => card.answer(String::new()),
            KeyCode::Esc => {
                card.field = false;
                None
            }
            KeyCode::Enter if multi || !card.text.trim().is_empty() => card.answer(answer(card)),
            KeyCode::Backspace => {
                card.text.pop();
                card.too_long = None;
                None
            }
            KeyCode::Char(ch) if !ch.is_control() => {
                card.text.push(ch);
                None
            }
            _ => None,
        };
    }
    match key {
        KeyCode::Up => {
            card.selected = card.selected.saturating_sub(1);
            None
        }
        KeyCode::Down => {
            card.selected = (card.selected + 1).min(choices.len());
            None
        }
        KeyCode::Esc => card.answer(String::new()),
        KeyCode::Enter if multi => card.answer(answer(card)),
        KeyCode::Enter => select_clarify(card, &choices, multi, card.selected),
        KeyCode::Char(' ') if multi => select_clarify(card, &choices, multi, card.selected),
        KeyCode::Char(ch @ '1'..='9') => {
            select_clarify(card, &choices, multi, ch as usize - '1' as usize)
        }
        _ => None,
    }
}

fn select_clarify(
    card: &mut Card,
    choices: &[String],
    multi: bool,
    index: usize,
) -> Option<PromptIntent> {
    if index > choices.len() {
        return None;
    }
    if index == choices.len() {
        card.field = true;
        return None;
    }
    if !multi {
        return card.answer(choices[index].clone());
    }
    if let Some(position) = card.ticks.iter().position(|&selected| selected == index) {
        card.ticks.remove(position);
    } else {
        card.ticks.push(index);
    }
    None
}

pub struct PromptSection<'a> {
    stack: &'a PromptStack,
}

impl PromptSection<'_> {
    fn lines(&self, width: u16) -> Vec<Line<'static>> {
        let stack = self.stack;
        let card = &stack.cards[0];
        let mut rows = Vec::new();
        let (title, masked_detail) = match &card.content {
            Content::Prompt(cell) => (
                if cell.kind == PromptKind::Approval {
                    "Approval needed"
                } else {
                    "Question"
                }
                .to_owned(),
                None,
            ),
            Content::Masked(view) => {
                let field = |name| {
                    view.fields
                        .iter()
                        .find(|(key, _)| key == name)
                        .map(|(_, value)| value.as_str())
                        .unwrap_or_default()
                };
                let (title, detail) = match view.method.as_str() {
                    "sudo" => (
                        "sudo password".into(),
                        if field("command").is_empty() {
                            String::new()
                        } else {
                            format!("$ {}", field("command"))
                        },
                    ),
                    "secret" => (
                        format!("Secret for {}", field("env_var")),
                        field("prompt").into(),
                    ),
                    "vault.code" => (
                        if field("site").is_empty() {
                            "One-time code".into()
                        } else {
                            format!("One-time code for {}", field("site"))
                        },
                        field("hint").into(),
                    ),
                    _ => (
                        format!("Unlock {}", field("display_name")),
                        field("backend").into(),
                    ),
                };
                (title, Some(detail))
            }
        };
        let mut header = title;
        if stack.cards.len() > 1 {
            header.push_str(&format!(" (1 of {})", stack.cards.len()));
        }
        if let Some(session) = card
            .session()
            .filter(|session| Some(*session) != stack.foreground.as_deref())
        {
            header.push_str(&format!(
                " · session {}",
                session.chars().take(8).collect::<String>()
            ));
        }
        add(&mut rows, &header, width);
        let hint = match &card.content {
            Content::Masked(view) => {
                if let Some(detail) = masked_detail {
                    add(&mut rows, &detail, width);
                }
                if view.method == "secret" {
                    add(
                        &mut rows,
                        "Hermes stores this value in the profile's .env file.",
                        width,
                    );
                }
                add(
                    &mut rows,
                    &format!("value: {}", "•".repeat(card.secret_chars.min(32))),
                    width,
                );
                if card.too_long.is_some() {
                    add(
                        &mut rows,
                        "value too long (16,384 characters at most)",
                        width,
                    );
                }
                "Enter send · Esc decline · Ctrl+C decline and stop".to_owned()
            }
            Content::Prompt(cell) if cell.kind == PromptKind::Approval => {
                if let Some(command) = cell.command.as_deref().filter(|s| !s.is_empty()) {
                    let mut command_rows = Vec::new();
                    for source in command.replace("\r\n", "\n").split('\n') {
                        let text = sanitize(source);
                        let wrapped = wrap_lines(
                            vec![Line::from(text.into_owned())],
                            width.saturating_sub(2).max(1),
                        );
                        for line in wrapped {
                            let prefix = if command_rows.is_empty() { "$ " } else { "  " };
                            command_rows.push(Line::from(format!("{prefix}{line}")));
                        }
                    }
                    let hidden = command_rows.len().saturating_sub(10);
                    rows.extend(command_rows.into_iter().take(10));
                    if hidden > 0 {
                        add(&mut rows, &format!("… {hidden} more lines"), width);
                    }
                }
                if let Some(description) = &cell.description {
                    add(&mut rows, description, width);
                }
                if cell.edit.is_some() {
                    add(&mut rows, "This approval carries a proposed edit.", width);
                }
                if let Some(seconds) = cell.timeout_s {
                    add(
                        &mut rows,
                        &format!(
                            "Hermes denies this after {} s without an answer.",
                            seconds.floor()
                        ),
                        width,
                    );
                }
                let choices = display_choices(&cell.choices);
                for (index, label) in choices.iter().enumerate() {
                    add_choice(&mut rows, card.selected, index, label, width);
                }
                if card.field || !card.text.is_empty() {
                    add_field(&mut rows, "note: ", &card.text, width);
                }
                format!(
                    "1-{} choose · Enter confirm · Esc deny · Tab note · Ctrl+C deny and stop",
                    choices.len()
                )
            }
            Content::Prompt(cell) => {
                if let Some(question) = &cell.question {
                    add(&mut rows, question, width);
                }
                if let Some(seconds) = cell.timeout_s {
                    add(
                        &mut rows,
                        &format!("Hermes stops waiting after {} s.", seconds.floor()),
                        width,
                    );
                }
                for (index, label) in cell.choices.iter().enumerate() {
                    let label = if cell.multi_select {
                        format!(
                            "[{}] {label}",
                            if card.ticks.contains(&index) {
                                "x"
                            } else {
                                " "
                            }
                        )
                    } else {
                        label.clone()
                    };
                    add_choice(&mut rows, card.selected, index, &label, width);
                }
                if !cell.choices.is_empty() {
                    add_choice(
                        &mut rows,
                        card.selected,
                        cell.choices.len(),
                        "Other (type an answer)",
                        width,
                    );
                }
                if card.field {
                    add_field(&mut rows, "answer: ", &card.text, width);
                }
                if let Some(length) = card.too_long {
                    add(
                        &mut rows,
                        &format!("answer too long ({length} of 16,384 characters)"),
                        width,
                    );
                }
                if cell.choices.is_empty() {
                    "Enter send · Esc skip · Ctrl+C skip and stop".into()
                } else if cell.multi_select {
                    format!(
                        "Space or 1-{} tick · Enter send · Esc skip · Ctrl+C skip and stop",
                        cell.choices.len() + 1
                    )
                } else {
                    format!(
                        "1-{} choose · Enter confirm · Esc skip · Ctrl+C skip and stop",
                        cell.choices.len() + 1
                    )
                }
            }
        };
        let hint = if stack.stopping.contains(&card.session().map(str::to_owned)) {
            "stopping…".into()
        } else if !stack.draft_empty {
            if matches!(card.content, Content::Masked(_)) {
                "Clear your draft to type the value here".into()
            } else {
                "Send or clear your draft to answer".into()
            }
        } else if let Content::Prompt(cell) = &card.content {
            if let PromptState::Answering(choice) = &cell.state {
                format!("sending {choice}…")
            } else if card.sent {
                "sending…".into()
            } else {
                hint
            }
        } else {
            hint
        };
        add(&mut rows, &hint, width);
        rows
    }
}

fn add(rows: &mut Vec<Line<'static>>, text: &str, width: u16) {
    if !text.is_empty() {
        rows.extend(wrap_lines(
            vec![Line::from(sanitize(text).into_owned())],
            width,
        ));
    }
}

fn add_choice(
    rows: &mut Vec<Line<'static>>,
    selected: usize,
    index: usize,
    label: &str,
    width: u16,
) {
    add(
        rows,
        &format!(
            "{} {}. {label}",
            if selected == index { "›" } else { " " },
            index + 1
        ),
        width,
    );
}

fn add_field(rows: &mut Vec<Line<'static>>, prefix: &str, text: &str, width: u16) {
    let room = usize::from(width).saturating_sub(prefix.len());
    let sanitized = sanitize(text);
    let mut suffix = String::new();
    for ch in sanitized.chars().rev() {
        let candidate = format!("{ch}{suffix}");
        if Line::from(candidate.as_str()).width() > room {
            break;
        }
        suffix = candidate;
    }
    add(rows, &format!("{prefix}{suffix}"), width);
}

impl LiveSection for PromptSection<'_> {
    fn height(&self, width: u16) -> u16 {
        u16::try_from(self.lines(width).len()).unwrap_or(u16::MAX)
    }
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let lines = self.lines(area.width);
        for (index, line) in lines
            .iter()
            .skip(lines.len().saturating_sub(usize::from(area.height)))
            .enumerate()
        {
            buf.set_line(area.x, area.y + index as u16, line, area.width);
        }
    }
}
