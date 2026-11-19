//! The card stack and its transient input state.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use ratatui::{buffer::Buffer, layout::Rect, text::Line};
use talaria_core::session::{
    ConnectionCard, ConnectionChoice, PromptCell, PromptKey, PromptKind, PromptState,
    is_answerable, recovered_limit_seconds,
};
use unicode_segmentation::UnicodeSegmentation;

use super::{
    KeyOutcome, MaskedTarget, MaskedValue, MaskedView, PromptIntent, display_choices,
    multi_select_answer,
};
use crate::history::{sanitize, wrap_lines};
use crate::keys::{Key, KeyCode, KeyKind};
use crate::live::LiveSection;

const LIMIT: usize = 16_384;
const DRAWN_LINE_CAP: usize = 10;
const MAX_RENDERED_CHOICES: usize = 16;

#[derive(Clone, PartialEq, Eq)]
enum Identity {
    Connection(String),
    Prompt(PromptKey),
    Masked(String),
}

#[derive(Clone)]
enum Content {
    Connection(ConnectionContent),
    Prompt(PromptCell),
    Masked(MaskedView),
}

#[derive(Clone)]
struct ConnectionContent {
    card: ConnectionCard,
    session_id: String,
    now_unix: f64,
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
    sent_at: Option<Instant>,
}

impl Card {
    fn masked(&self) -> bool {
        match &self.content {
            Content::Connection(_) => false,
            Content::Masked(_) => true,
            Content::Prompt(cell) => {
                cell.kind == PromptKind::ServerRequest
                    && is_answerable(cell.command.as_deref().unwrap_or_default())
            }
        }
    }

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
            sent_at: None,
        }
    }

    fn session(&self) -> Option<&str> {
        match &self.content {
            Content::Connection(connection) => Some(&connection.session_id),
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
        if self.masked() {
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

    fn masked_answer(&mut self, decline: bool) -> Option<PromptIntent> {
        let value = std::mem::replace(&mut self.secret, MaskedValue::new());
        self.secret_chars = 0;
        let target = match &self.content {
            Content::Connection(_) => return None,
            Content::Masked(view) => {
                self.sent = true;
                MaskedTarget::Live(view.target.clone())
            }
            Content::Prompt(cell) => MaskedTarget::Recovered {
                request_id: cell.key.prompt_id.clone(),
            },
        };
        Some(PromptIntent::Masked {
            target,
            value: if decline {
                drop(value);
                MaskedValue::new()
            } else {
                value
            },
        })
    }

    fn expire_connection_sent(&mut self, now: Instant) {
        if matches!(self.content, Content::Connection(_))
            && self
                .sent_at
                .is_some_and(|sent| now.saturating_duration_since(sent) >= Duration::from_secs(5))
        {
            self.sent = false;
            self.sent_at = None;
        }
    }

    fn connection_answer(
        &mut self,
        choice: ConnectionChoice,
        now: Instant,
    ) -> Option<PromptIntent> {
        let Content::Connection(connection) = &self.content else {
            return None;
        };
        let choices = connection_choices(&connection.card);
        self.selected = choices.iter().position(|offered| *offered == choice)?;
        self.sent = true;
        self.sent_at = Some(now);
        Some(PromptIntent::Connect {
            session_id: connection.session_id.clone(),
            op_id: connection.card.op_id.clone(),
            choice,
        })
    }

    fn answer(&mut self, value: String) -> Option<PromptIntent> {
        let Content::Prompt(cell) = &self.content else {
            return None;
        };
        if cell.kind == PromptKind::ServerRequest {
            return None;
        }
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

/// The fields that decide whether the top card is still the same prompt.
fn same_focus(left: &Content, right: &Content) -> bool {
    match (left, right) {
        (Content::Masked(before), Content::Masked(after)) => {
            before.request_id == after.request_id
                && before.session_id == after.session_id
                && before.method == after.method
                && before.fields == after.fields
                && before.target == after.target
        }
        (Content::Prompt(before), Content::Prompt(after)) => {
            before.key == after.key
                && before.kind == after.kind
                && before.command == after.command
                && before.description == after.description
                && before.question == after.question
                && before.choices == after.choices
                && before.state == after.state
        }
        (Content::Connection(before), Content::Connection(after)) => {
            before.session_id == after.session_id && before.card == after.card
        }
        _ => false,
    }
}

/// A failed answer reopening the same prompt keeps what was typed.
fn same_draft(left: &Content, right: &Content) -> bool {
    match (left, right) {
        (Content::Prompt(before), Content::Prompt(after)) => {
            before.key == after.key
                && before.kind == after.kind
                && before.command == after.command
                && before.description == after.description
                && before.question == after.question
                && before.choices == after.choices
        }
        _ => same_focus(left, right),
    }
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

    pub fn show_connection(
        &mut self,
        card: Option<&ConnectionCard>,
        session_id: &str,
        draft_empty: bool,
        now: Instant,
        now_unix: f64,
    ) {
        let old_first = self.cards.first().map(|card| card.id.clone());
        let old_connection = self
            .cards
            .iter()
            .position(|card| matches!(card.id, Identity::Connection(_)))
            .map(|index| self.cards.remove(index));
        if let Some(connection) = card {
            let id = Identity::Connection(connection.op_id.clone());
            let mut card = match old_connection {
                Some(card) if card.id == id => card,
                _ => Card::new(
                    id,
                    Content::Connection(ConnectionContent {
                        card: connection.clone(),
                        session_id: session_id.to_owned(),
                        now_unix,
                    }),
                ),
            };
            let selected = match &card.content {
                Content::Connection(old) => {
                    connection_choices(&old.card).get(card.selected).copied()
                }
                _ => None,
            };
            card.content = Content::Connection(ConnectionContent {
                card: connection.clone(),
                session_id: session_id.to_owned(),
                now_unix,
            });
            card.selected = selected
                .and_then(|choice| {
                    connection_choices(connection)
                        .iter()
                        .position(|offered| *offered == choice)
                })
                .unwrap_or(0);
            card.expire_connection_sent(now);
            self.cards.insert(0, card);
        }
        if draft_empty
            && (!self.draft_empty || old_first != self.cards.first().map(|card| card.id.clone()))
        {
            self.focused_at = Some(now);
        }
        self.draft_empty = draft_empty;
        self.foreground = Some(session_id.to_owned());
    }

    pub fn connection_remaining_seconds(&self) -> Option<i64> {
        self.cards.iter().find_map(|card| match &card.content {
            Content::Connection(connection) => Some(
                (connection.card.deadline_at - connection.now_unix)
                    .floor()
                    .max(0.0) as i64,
            ),
            _ => None,
        })
    }

    pub fn connection_lines(&self, width: u16) -> Vec<Line<'static>> {
        let Some(card) = self
            .cards
            .iter()
            .find(|card| matches!(card.content, Content::Connection(_)))
        else {
            return Vec::new();
        };
        let Content::Connection(connection) = &card.content else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        add(&mut rows, "Connection request", width);
        for target in &connection.card.targets {
            let suffix = if target
                .required_env
                .as_ref()
                .is_some_and(|env| !env.is_empty())
            {
                " — cannot be approved here"
            } else {
                ""
            };
            add(
                &mut rows,
                &format!("{}: {}{suffix}", target.name, target.state),
                width,
            );
            if let Some(url) = &target.connect_url {
                add(&mut rows, url, width);
            }
        }
        if let Some(seconds) = self.connection_remaining_seconds() {
            add(&mut rows, &format!("{seconds} s remaining"), width);
        }
        let choices = connection_choices(&connection.card);
        for (index, choice) in choices.iter().enumerate() {
            add_choice(
                &mut rows,
                card.selected,
                index,
                connection_word(*choice),
                width,
            );
        }
        let hint = if self.stopping.contains(&Some(connection.session_id.clone())) {
            "stopping…"
        } else if !self.draft_empty {
            "Send or clear your draft to answer"
        } else if card.sent {
            "sending…"
        } else {
            "Digits choose · Arrows select · Enter confirm · Esc Skip · Ctrl+C interrupt"
        };
        add(&mut rows, hint, width);
        rows
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
        let old_top = self.cards.first().map(|card| card.content.clone());
        self.hidden
            .retain(|key| prompts.iter().any(|cell| cell.key == *key));
        let mut old = std::mem::take(&mut self.cards);
        if let Some(index) = old
            .iter()
            .position(|card| matches!(card.id, Identity::Connection(_)))
        {
            self.cards.push(old.remove(index));
        }
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
                if !same_draft(&card.content, &content) {
                    card.secret = MaskedValue::new();
                    card.secret_chars = 0;
                    card.text.clear();
                    card.ticks.clear();
                    card.sent = false;
                    card.too_long = None;
                } else if matches!((&card.content, &content), (Content::Prompt(before), Content::Prompt(after))
                    if matches!(before.state, PromptState::Answering(_)) && after.state == PromptState::Open)
                {
                    // A failed answer reopens the same card without discarding typed state.
                    card.sent = false;
                }
                card.content = content;
                card
            } else {
                Card::new(id, content)
            };
            self.cards.push(card);
        }
        let new_first = self.cards.first().map(|card| card.id.clone());
        let new_top = self.cards.first().map(|card| card.content.clone());
        let content_changed = old_first.is_some()
            && old_first == new_first
            && match (&old_top, &new_top) {
                (Some(before), Some(after)) => !same_focus(before, after),
                _ => false,
            };
        if draft_empty && (!self.draft_empty || old_first != new_first || content_changed) {
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
        self.cards[0].expire_connection_sent(now);
        let control_c = key.mods.ctrl && matches!(key.code, KeyCode::Char('c' | 'C'));
        let session = self.cards[0].session().map(str::to_owned);
        if self.stopping.contains(&session) {
            return result;
        }
        if control_c {
            let card = &mut self.cards[0];
            let intent = match &card.content {
                _ if card.masked() && !card.sent => card.masked_answer(true),
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
            Content::Connection(connection) => {
                let choices = connection_choices(&connection.card);
                match key.code {
                    KeyCode::Up => {
                        card.selected = card.selected.saturating_sub(1);
                        None
                    }
                    KeyCode::Down => {
                        card.selected = (card.selected + 1).min(choices.len() - 1);
                        None
                    }
                    KeyCode::Enter => card.connection_answer(choices[card.selected], now),
                    KeyCode::Char(ch @ '1'..='9') => choices
                        .get(ch as usize - '1' as usize)
                        .copied()
                        .and_then(|choice| card.connection_answer(choice, now)),
                    KeyCode::Esc => card.connection_answer(ConnectionChoice::Skip, now),
                    _ => None,
                }
            }
            _ if card.masked() => match key.code {
                KeyCode::Enter => card.masked_answer(false),
                KeyCode::Esc => card.masked_answer(true),
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
                let choices: Vec<String> = shown_approval_choices(&cell.choices)
                    .into_iter()
                    .map(str::to_owned)
                    .collect();
                if card.field {
                    match key.code {
                        // Shift+Tab is the approvals toggle's key; the card keeps its state.
                        KeyCode::Tab | KeyCode::Esc
                            if !(key.mods.shift && key.code == KeyCode::Tab) =>
                        {
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
                        KeyCode::Tab if !key.mods.shift => {
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
            Content::Prompt(cell) if cell.kind == PromptKind::Clarify => {
                clarify_key(card, key.code)
            }
            _ => None,
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

fn connection_choices(card: &ConnectionCard) -> &'static [ConnectionChoice] {
    if card.targets.iter().any(|target| {
        target
            .required_env
            .as_ref()
            .is_some_and(|env| !env.is_empty())
    }) {
        &[ConnectionChoice::Skip, ConnectionChoice::Continue]
    } else {
        &[
            ConnectionChoice::Approve,
            ConnectionChoice::Skip,
            ConnectionChoice::Continue,
        ]
    }
}

fn connection_word(choice: ConnectionChoice) -> &'static str {
    match choice {
        ConnectionChoice::Approve => "Approve",
        ConnectionChoice::Skip => "Skip",
        ConnectionChoice::Continue => "Continue",
    }
}

fn offered_choice_count(card: &Card) -> Option<usize> {
    match &card.content {
        Content::Prompt(cell) if cell.kind == PromptKind::Clarify => {
            Some(cell.choices.len().min(MAX_RENDERED_CHOICES))
        }
        _ => None,
    }
}

fn clarify_answer(card: &Card, count: usize, multi: bool) -> String {
    if !multi {
        return card.text.clone();
    }
    let Content::Prompt(cell) = &card.content else {
        return String::new();
    };
    let choices: Vec<String> = (0..count)
        .filter(|index| card.ticks.contains(index))
        .filter_map(|index| cell.choices.get(index).cloned())
        .collect();
    multi_select_answer(
        &choices,
        &(0..choices.len()).collect::<Vec<_>>(),
        Some(&card.text),
    )
}

fn clarify_key(card: &mut Card, key: KeyCode) -> Option<PromptIntent> {
    let count = offered_choice_count(card)?;
    let multi = matches!(&card.content, Content::Prompt(cell) if cell.multi_select && count > 0);
    let free = count == 0;
    if card.field {
        return match key {
            KeyCode::Esc if free => card.answer(String::new()),
            KeyCode::Esc => {
                card.field = false;
                None
            }
            KeyCode::Enter if multi || !card.text.trim().is_empty() => {
                card.answer(clarify_answer(card, count, multi))
            }
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
            card.selected = (card.selected + 1).min(count);
            None
        }
        KeyCode::Esc => card.answer(String::new()),
        KeyCode::Enter if multi => card.answer(clarify_answer(card, count, multi)),
        KeyCode::Enter => select_clarify(card, count, multi, card.selected),
        KeyCode::Char(' ') if multi => select_clarify(card, count, multi, card.selected),
        KeyCode::Char(ch @ '1'..='9') => {
            select_clarify(card, count, multi, ch as usize - '1' as usize)
        }
        _ => None,
    }
}

fn select_clarify(
    card: &mut Card,
    count: usize,
    multi: bool,
    index: usize,
) -> Option<PromptIntent> {
    if index > count {
        return None;
    }
    if index == count {
        card.field = true;
        return None;
    }
    if !multi {
        let answer = match &card.content {
            Content::Prompt(cell) => cell.choices.get(index).cloned().unwrap_or_default(),
            _ => return None,
        };
        return card.answer(answer);
    }
    if let Some(position) = card.ticks.iter().position(|&selected| selected == index) {
        card.ticks.remove(position);
    } else {
        card.ticks.push(index);
    }
    None
}

fn shown_approval_choices(choices: &[String]) -> Vec<&str> {
    let end = choices.len().min(MAX_RENDERED_CHOICES);
    display_choices(&choices[..end])
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
            Content::Connection(_) => return stack.connection_lines(width),
            Content::Prompt(cell) => (
                if cell.kind == PromptKind::ServerRequest {
                    cell.command.as_deref().unwrap_or_default()
                } else if cell.kind == PromptKind::Approval {
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
                        (!field("command").is_empty()).then(|| ("$ ", field("command"))),
                    ),
                    "secret" => (
                        format!("Secret for {}", field("env_var")),
                        Some(("", field("prompt"))),
                    ),
                    "vault.code" => (
                        if field("site").is_empty() {
                            "One-time code".into()
                        } else {
                            format!("One-time code for {}", field("site"))
                        },
                        Some(("", field("hint"))),
                    ),
                    _ => (
                        format!("Unlock {}", field("display_name")),
                        Some(("", field("backend"))),
                    ),
                };
                (title, detail)
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
        add_capped(&mut rows, &header, width);
        let hint = match &card.content {
            Content::Connection(_) => return stack.connection_lines(width),
            Content::Prompt(cell) if cell.kind == PromptKind::ServerRequest => {
                let method = cell.command.as_deref().unwrap_or_default();
                add(&mut rows, "asked while you were away", width);
                if let Some(seconds) = recovered_limit_seconds(method) {
                    add(&mut rows, &format!("limit {seconds} s"), width);
                }
                if is_answerable(method) {
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
                } else {
                    add(&mut rows, "This request cannot be answered here.", width);
                    String::new()
                }
            }
            Content::Masked(view) => {
                if let Some((prefix, detail)) = masked_detail {
                    if prefix.is_empty() {
                        add_capped(&mut rows, detail, width);
                    } else {
                        add_capped_prefixed(&mut rows, prefix, detail, width);
                    }
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
                    append_approval_command(&mut rows, command, width);
                }
                if let Some(description) = &cell.description {
                    add_capped(&mut rows, description, width);
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
                let choices = shown_approval_choices(&cell.choices);
                for (index, label) in choices.iter().enumerate() {
                    add_choice(&mut rows, card.selected, index, label, width);
                }
                if cell.choices.len() > choices.len() {
                    add(
                        &mut rows,
                        &format!("… ({} more choices)", cell.choices.len() - choices.len()),
                        width,
                    );
                }
                if card.field || !card.text.is_empty() {
                    add_field(
                        &mut rows,
                        "note (sent after the answer) ",
                        &card.text,
                        width,
                    );
                }
                format!(
                    "1-{} choose · Enter confirm · Esc deny · Tab note · Ctrl+C deny and stop",
                    choices.len()
                )
            }
            Content::Prompt(cell) => {
                if let Some(question) = &cell.question {
                    add_capped(&mut rows, question, width);
                }
                if let Some(seconds) = cell.timeout_s {
                    add(
                        &mut rows,
                        &format!("Hermes stops waiting after {} s.", seconds.floor()),
                        width,
                    );
                }
                let count = cell.choices.len().min(MAX_RENDERED_CHOICES);
                for (index, label) in cell.choices.iter().take(count).enumerate() {
                    if cell.multi_select {
                        let (label, _) = bounded_source(label, width);
                        let mark = if card.ticks.contains(&index) {
                            "x"
                        } else {
                            " "
                        };
                        let label = format!("[{mark}] {label}");
                        add_choice(&mut rows, card.selected, index, &label, width);
                    } else {
                        add_choice(&mut rows, card.selected, index, label, width);
                    }
                }
                if cell.choices.len() > count {
                    add(
                        &mut rows,
                        &format!("… ({} more choices)", cell.choices.len() - count),
                        width,
                    );
                }
                if count > 0 {
                    add_choice(
                        &mut rows,
                        card.selected,
                        count,
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
                        cell.choices.len().min(MAX_RENDERED_CHOICES) + 1
                    )
                } else {
                    format!(
                        "1-{} choose · Enter confirm · Esc skip · Ctrl+C skip and stop",
                        cell.choices.len().min(MAX_RENDERED_CHOICES) + 1
                    )
                }
            }
        };
        let hint = if stack.stopping.contains(&card.session().map(str::to_owned)) {
            "stopping…".into()
        } else if !stack.draft_empty {
            if card.masked() {
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

/// One approval-command row. CR and tab stay visible, unlike a line feed or four spaces.
fn approval_command_row(source: &str) -> String {
    let mut out = String::new();
    let mut buf = [0; 4];
    for c in source.chars() {
        match c {
            '\t' => out.push('\u{2409}'),
            '\r' => out.push('\u{240d}'),
            other => out.push_str(&sanitize(other.encode_utf8(&mut buf))),
        }
    }
    out
}

fn add(rows: &mut Vec<Line<'static>>, text: &str, width: u16) {
    if !text.is_empty() {
        rows.extend(wrap_lines(
            vec![Line::from(sanitize(text).into_owned())],
            width,
        ));
    }
}

fn add_capped(rows: &mut Vec<Line<'static>>, text: &str, width: u16) {
    if text.is_empty() {
        return;
    }
    let (text, cut) = bounded_source(text, width);
    let (wrapped, mut hidden) = if is_plain_ascii(text) {
        capped_ascii(text, width.max(1), DRAWN_LINE_CAP)
    } else {
        capped_wrap(text, width, DRAWN_LINE_CAP)
    };
    if cut {
        hidden = hidden.max(1);
    }
    rows.extend(wrapped);
    if hidden > 0 {
        add(rows, &format!("… ({hidden} more lines)"), width);
    }
}

fn add_capped_prefixed(rows: &mut Vec<Line<'static>>, prefix: &str, body: &str, width: u16) {
    if body.is_empty() {
        add_capped(rows, prefix, width);
        return;
    }
    let (body, cut) = bounded_source(body, width);
    if !prefix.contains('\n') && is_plain_ascii_lines(body) {
        let width = width.max(1);
        let mut shown = Vec::new();
        let mut hidden = 0usize;
        let mut first = true;
        for line in logical_lines(body) {
            let room = DRAWN_LINE_CAP.saturating_sub(shown.len());
            let (wrapped, extra) = if first {
                first = false;
                capped_ascii_prefixed(prefix, line, width, room)
            } else {
                capped_wrap(line, width, room)
            };
            shown.extend(wrapped);
            hidden += extra;
        }
        if cut {
            hidden = hidden.max(1);
        }
        rows.extend(shown);
        if hidden > 0 {
            add(rows, &format!("… ({hidden} more lines)"), width);
        }
        return;
    }
    let mut combined = String::with_capacity(prefix.len() + body.len());
    combined.push_str(prefix);
    combined.push_str(body);
    add_capped(rows, &combined, width);
}

fn append_approval_command(rows: &mut Vec<Line<'static>>, command: &str, width: u16) {
    let (command, cut) = bounded_source(command, width);
    let inner = width.saturating_sub(2).max(1);
    let mut shown = Vec::new();
    let mut hidden = 0usize;
    for source in command.split('\n') {
        let room = DRAWN_LINE_CAP.saturating_sub(shown.len());
        let (wrapped, extra) = if is_plain_ascii(source) {
            capped_ascii(source, inner, room)
        } else {
            capped_wrap(&approval_command_row(source), inner, room)
        };
        for line in wrapped {
            let prefix = if shown.is_empty() { "$ " } else { "  " };
            shown.push(Line::from(format!("{prefix}{line}")));
        }
        hidden += extra;
    }
    if cut {
        hidden = hidden.max(1);
    }
    rows.extend(shown);
    if hidden > 0 {
        add(rows, &format!("… {hidden} more lines"), width);
    }
}

fn logical_lines(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = Some(text);
    std::iter::from_fn(move || {
        let current = rest?;
        if let Some(pos) = current.find('\n') {
            let mut line = &current[..pos];
            rest = Some(&current[pos + 1..]);
            if let Some(stripped) = line.strip_suffix('\r') {
                line = stripped;
            }
            Some(line)
        } else {
            rest = None;
            Some(current)
        }
    })
}

fn is_plain_ascii(text: &str) -> bool {
    text.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
}

fn is_plain_ascii_lines(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => index += 1,
            // A carriage return is a line break only as part of CRLF. A bare CR stays visible.
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => index += 1,
            byte if (0x20..=0x7e).contains(&byte) => index += 1,
            _ => return false,
        }
    }
    true
}

fn bounded_source(text: &str, width: u16) -> (&str, bool) {
    let budget = usize::from(width.max(1))
        .saturating_mul(DRAWN_LINE_CAP + 1)
        .saturating_mul(8)
        .max(64);
    if text.len() <= budget {
        return (text, false);
    }
    let mut end = budget;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

fn capped_wrap(text: &str, width: u16, limit: usize) -> (Vec<Line<'static>>, usize) {
    let width = width.max(1);
    if text.is_empty() {
        let shown = if limit == 0 {
            Vec::new()
        } else {
            vec![Line::from("")]
        };
        return (shown, usize::from(limit == 0));
    }
    if is_plain_ascii(text) {
        return capped_ascii(text, width, limit);
    }
    let clean = sanitize(text);
    let (total, end) = grapheme_rows(&clean, width, limit);
    let mut shown = if end == 0 {
        Vec::new()
    } else {
        wrap_lines(vec![Line::from(clean[..end].to_owned())], width)
    };
    let cut = shown.len() > limit || end < clean.len();
    shown.truncate(limit);
    let hidden = total.saturating_sub(shown.len()).max(usize::from(cut));
    (shown, hidden)
}

fn capped_ascii(text: &str, width: u16, limit: usize) -> (Vec<Line<'static>>, usize) {
    let width_us = usize::from(width.max(1));
    if text.is_empty() {
        let shown = if limit == 0 {
            Vec::new()
        } else {
            vec![Line::from("")]
        };
        return (shown, usize::from(limit == 0));
    }
    let total = text.len().div_ceil(width_us);
    let take = total.min(limit);
    let mut lines = Vec::with_capacity(take);
    for row in 0..take {
        let start = row * width_us;
        let end = (start + width_us).min(text.len());
        lines.push(Line::from(text[start..end].to_owned()));
    }
    (lines, total - take)
}

fn capped_ascii_prefixed(
    prefix: &str,
    line: &str,
    width: u16,
    limit: usize,
) -> (Vec<Line<'static>>, usize) {
    let width = width.max(1);
    let prefix_cols = display_columns(prefix);
    if prefix_cols >= width || !is_plain_ascii(line) {
        let mut combined = String::with_capacity(prefix.len() + line.len());
        combined.push_str(prefix);
        combined.push_str(line);
        return capped_wrap(&combined, width, limit);
    }
    let first_room = usize::from(width - prefix_cols);
    let width_us = usize::from(width);
    let extra_rows = line.len().saturating_sub(first_room).div_ceil(width_us);
    let total = 1 + extra_rows;
    let take = total.min(limit);
    let mut lines = Vec::with_capacity(take);
    if take > 0 {
        let first_end = first_room.min(line.len());
        lines.push(Line::from(format!("{prefix}{}", &line[..first_end])));
    }
    let mut offset = first_room.min(line.len());
    for _ in 1..take {
        let end = (offset + width_us).min(line.len());
        lines.push(Line::from(line[offset..end].to_owned()));
        offset = end;
    }
    (lines, total - take)
}

fn display_columns(text: &str) -> u16 {
    text.graphemes(true)
        .map(crate::history::grapheme_width)
        .fold(0, |used, columns| used.saturating_add(columns))
}

fn grapheme_rows(text: &str, width: u16, limit: usize) -> (usize, usize) {
    let width = width.max(1);
    if text.is_empty() {
        return (0, 0);
    }
    if limit == 0 {
        return (1, 0);
    }
    let mut rows = 1usize;
    let mut used = 0u16;
    let mut seen = 0usize;
    let max_graphemes = limit
        .saturating_mul(usize::from(width))
        .saturating_add(limit)
        .max(1);
    for (offset, grapheme) in text.grapheme_indices(true) {
        seen += 1;
        let columns = crate::history::grapheme_width(grapheme);
        if used > 0 && used.saturating_add(columns) > width {
            rows += 1;
            used = 0;
            if rows > limit {
                return (rows, offset);
            }
        }
        used = used.saturating_add(columns);
        if seen >= max_graphemes {
            let cut = offset.saturating_add(grapheme.len()).min(text.len());
            return (limit + 1, cut);
        }
    }
    (rows, text.len())
}

fn add_choice(
    rows: &mut Vec<Line<'static>>,
    selected: usize,
    index: usize,
    label: &str,
    width: u16,
) {
    let marker = if selected == index { "› " } else { "  " };
    add_capped_prefixed(rows, &format!("{marker}{}. ", index + 1), label, width);
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
