//! The card stack and its transient input state.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use ratatui::{buffer::Buffer, layout::Rect, text::Line};
use talaria_core::session::{
    ConnectionCard, ConnectionChoice, PromptCell, PromptKey, PromptKind, PromptState,
    is_answerable, recovered_limit_seconds,
};

use super::{
    KeyOutcome, MaskedValue, MaskedView, PromptIntent, display_choices, multi_select_answer,
};
use crate::history::{sanitize, wrap_lines};
use crate::keys::{Key, KeyCode, KeyKind};
use crate::live::LiveSection;

const LIMIT: usize = 16_384;

#[derive(Clone, PartialEq, Eq)]
enum Identity {
    Connection(String),
    Prompt(PromptKey),
    Masked(String),
}

enum Content {
    Connection(ConnectionContent),
    Prompt(PromptCell),
    Masked(MaskedView),
}

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
        let request_id = match &self.content {
            Content::Connection(_) => return None,
            Content::Masked(view) => {
                self.sent = true;
                view.request_id.clone()
            }
            Content::Prompt(cell) => cell.key.prompt_id.clone(),
        };
        Some(PromptIntent::Masked {
            request_id,
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
                let choices: Vec<String> = display_choices(&cell.choices)
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
