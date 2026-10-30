//! Board state, navigation, and write intentions without input or output.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveTable {
    pub columns: Vec<String>,
    pub manual: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardComment {
    pub author: String,
    pub body: String,
    pub age: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardCard {
    pub id: String,
    pub title: String,
    pub assignee: String,
    pub status: String,
    pub created_at_ms: u64,
    pub started_at_ms: Option<u64>,
    pub parent_titles: Vec<(String, String)>,
    pub child_titles: Vec<(String, String)>,
    pub body: String,
    pub latest_summary: Option<String>,
    pub comments: Vec<CardComment>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameColumn {
    pub label: String,
    pub cards: Vec<FrameCard>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameCard {
    pub id: String,
    pub title: String,
    pub assignee: String,
    pub status: String,
    pub age: String,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KanbanFrame {
    pub board: String,
    pub shown: usize,
    pub total: usize,
    pub live: String,
    pub columns: Vec<FrameColumn>,
    pub status_line: String,
    pub detail: Option<BoardCard>,
}

#[derive(Clone, Debug)]
pub struct KanbanBoardState {
    workflow: Option<MoveTable>,
    cards: Vec<BoardCard>,
    pub board: String,
    pub column: usize,
    pub row: usize,
    pub cursor: i64,
    pub live: bool,
    pub parked: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    pub filter: String,
    pub mode: BoardMode,
    pub detail_id: Option<String>,
    pub detail_fields: Vec<String>,
    labels: BTreeMap<String, String>,
    now_ms: u64,
    fetching: bool,
    connecting: bool,
    dirty: bool,
    last_fetch_ms: Option<u64>,
}

impl KanbanBoardState {
    pub fn blank() -> Self {
        Self {
            workflow: None,
            cards: Vec::new(),
            board: "default".into(),
            column: 0,
            row: 0,
            cursor: 0,
            live: false,
            parked: false,
            loading: false,
            error: None,
            notice: None,
            filter: String::new(),
            mode: BoardMode::Browse,
            detail_id: None,
            detail_fields: vec![],
            labels: BTreeMap::new(),
            now_ms: 0,
            fetching: false,
            connecting: false,
            dirty: false,
            last_fetch_ms: None,
        }
    }

    pub fn load_workflow(&mut self, table: Option<MoveTable>) {
        self.workflow = table.filter(|table| {
            !table.columns.is_empty() && !table.columns.iter().any(|column| column == "archived")
        });
    }

    pub fn load_cards(&mut self, cards: Vec<BoardCard>) {
        self.cards = cards
            .into_iter()
            .filter(|c| c.status != "archived")
            .collect();
    }

    pub fn offered(&self, status: &str) -> Vec<String> {
        let Some(table) = &self.workflow else {
            return vec![];
        };
        let mut offered = if table.columns.iter().any(|s| s == status) {
            table
                .columns
                .iter()
                .filter(|s| {
                    table
                        .manual
                        .get(status)
                        .is_some_and(|allowed| allowed.contains(s))
                })
                .cloned()
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        offered.push("archived".into());
        offered
    }

    pub fn writes_allowed(&self) -> bool {
        self.workflow.is_some() && !self.loading && self.error.is_none()
    }

    pub fn card(&self, id: &str) -> Option<&BoardCard> {
        self.cards.iter().find(|card| card.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoardKey {
    Char(char),
    Enter,
    Esc,
    Backspace,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoardMode {
    Browse,
    Moves {
        destinations: Vec<String>,
        selected: usize,
    },
    Boards {
        slugs: Vec<String>,
        selected: usize,
    },
    Confirm {
        destination: String,
        prompt: String,
    },
    Evidence {
        text: String,
    },
    Comment {
        text: String,
    },
    Filter,
}
#[derive(Clone, Debug)]
pub enum KanbanInput {
    Key(BoardKey),
    Snapshot {
        cards: Vec<BoardCard>,
        cursor: i64,
    },
    Workflow {
        table: Option<MoveTable>,
        labels: BTreeMap<String, String>,
    },
    Detail {
        card: BoardCard,
        fields: Vec<String>,
    },
    Boards(Vec<String>),
    Tail {
        cursor: i64,
        events: bool,
        now_ms: u64,
    },
    TailParked,
    FetchFinished {
        now_ms: u64,
    },
    Tick {
        now_ms: u64,
    },
    Failed(String),
    Notice(String),
    Mutated,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KanbanEffect {
    Fetch {
        workflow: bool,
        detail: Option<String>,
    },
    Detail(String),
    Boards,
    Dispatch,
    Patch {
        id: String,
        status: String,
        summary: Option<String>,
    },
    Comment {
        id: String,
        body: String,
    },
    StartTail {
        cursor: i64,
    },
    StopTail,
    Close,
}
impl KanbanBoardState {
    pub fn open(board: &str, now_ms: u64) -> (Self, Vec<KanbanEffect>) {
        let mut state = Self::blank();
        state.board = board.into();
        state.now_ms = now_ms;
        state.loading = true;
        state.fetching = true;
        (
            state,
            vec![KanbanEffect::Fetch {
                workflow: true,
                detail: None,
            }],
        )
    }
    pub fn selected(&self) -> Option<&BoardCard> {
        if let Some(id) = &self.detail_id {
            return self.card(id);
        }
        let columns = self.column_keys();
        let key = columns.get(self.column)?;
        self.filtered()
            .into_iter()
            .filter(|c| self.column_for(c) == *key)
            .nth(self.row)
    }
    fn column_keys(&self) -> Vec<String> {
        let mut keys = self
            .workflow
            .as_ref()
            .map(|w| w.columns.clone())
            .unwrap_or_else(|| {
                vec![
                    "triage",
                    "todo",
                    "scheduled",
                    "ready",
                    "running",
                    "blocked",
                    "review",
                    "done",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect()
            });
        keys.retain(|k| k != "archived");
        if self
            .cards
            .iter()
            .any(|c| c.status != "archived" && !keys.contains(&c.status))
        {
            keys.push("Other".into());
        }
        keys
    }
    fn column_for<'a>(&'a self, card: &'a BoardCard) -> &'a str {
        if self.column_keys().contains(&card.status) {
            &card.status
        } else {
            "Other"
        }
    }
    fn filtered(&self) -> Vec<&BoardCard> {
        let filter = self.filter.to_lowercase();
        self.cards
            .iter()
            .filter(|c| {
                c.status != "archived"
                    && (c.title.to_lowercase().contains(&filter)
                        || c.assignee.to_lowercase().contains(&filter))
            })
            .collect()
    }
    pub fn frame(&self) -> KanbanFrame {
        let selected = self.selected().map(|c| c.id.as_str());
        let columns = self
            .column_keys()
            .iter()
            .map(|key| FrameColumn {
                label: self.labels.get(key).cloned().unwrap_or_else(|| {
                    let mut c = key.chars();
                    c.next()
                        .map(|v| v.to_uppercase().to_string() + c.as_str())
                        .unwrap_or_default()
                }),
                cards: self
                    .filtered()
                    .into_iter()
                    .filter(|c| self.column_for(c) == *key)
                    .map(|c| FrameCard {
                        id: c.id.clone(),
                        title: c.title.clone(),
                        assignee: c.assignee.clone(),
                        status: c.status.clone(),
                        age: age(self.now_ms.saturating_sub(if c.status == "running" {
                            c.started_at_ms.unwrap_or(c.created_at_ms)
                        } else {
                            c.created_at_ms
                        })),
                        selected: selected == Some(c.id.as_str()),
                    })
                    .collect(),
            })
            .collect();
        let tail = if self.parked {
            "tail parked".into()
        } else if self.live {
            format!("live:{}", self.cursor)
        } else {
            "connecting...".into()
        };
        let mut status = vec![tail];
        if self.workflow.is_none() && !self.loading {
            status.push("columns unavailable; board is read-only".into());
        }
        if self.parked {
            status.push("live updates unavailable; press r to refresh".into());
        }
        if let Some(notice) = &self.notice {
            status.push(notice.clone());
        }
        KanbanFrame {
            board: self.board.clone(),
            shown: self.filtered().len(),
            total: self.cards.len(),
            live: if self.parked {
                "parked".into()
            } else if self.live {
                self.cursor.to_string()
            } else {
                "off".into()
            },
            columns,
            status_line: status.join(" | "),
            detail: self
                .detail_id
                .as_ref()
                .and_then(|id| self.card(id))
                .cloned(),
        }
    }
    pub fn menu_lines(&self) -> Vec<String> {
        match &self.mode {
            BoardMode::Moves {
                destinations,
                selected,
            } => destinations
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    format!(
                        "{} {}  {}{}",
                        if i == *selected { ">" } else { " " },
                        i + 1,
                        self.labels.get(s).cloned().unwrap_or_else(|| s.clone()),
                        if matches!(s.as_str(), "done" | "blocked" | "archived") {
                            " (asks first)"
                        } else {
                            ""
                        }
                    )
                })
                .collect(),
            BoardMode::Boards { slugs, selected } => slugs
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{} {}  {s}", if i == *selected { ">" } else { " " }, i + 1))
                .collect(),
            BoardMode::Confirm { prompt, .. } => {
                vec![prompt.clone(), "Enter confirms, Esc cancels".into()]
            }
            BoardMode::Evidence { text } => vec![format!("Completion evidence: {text}")],
            BoardMode::Comment { text } => vec![format!("Comment: {text}")],
            BoardMode::Filter => vec![format!("Filter: {}", self.filter)],
            BoardMode::Browse => vec![],
        }
    }
    fn refresh(&mut self, workflow: bool) -> Vec<KanbanEffect> {
        if self.fetching {
            self.dirty = true;
            return vec![];
        }
        self.fetching = true;
        self.dirty = false;
        self.last_fetch_ms = Some(self.now_ms);
        vec![KanbanEffect::Fetch {
            workflow: workflow || self.workflow.is_none(),
            detail: self.detail_id.clone(),
        }]
    }
    fn clamp(&mut self) {
        self.column = self.column.min(self.column_keys().len().saturating_sub(1));
        let key = &self.column_keys()[self.column];
        self.row = self.row.min(
            self.filtered()
                .iter()
                .filter(|c| self.column_for(c) == *key)
                .count()
                .saturating_sub(1),
        );
    }
    fn choose_move(&mut self, destination: String) -> Vec<KanbanEffect> {
        let Some(card) = self.selected().cloned() else {
            return vec![];
        };
        if matches!(destination.as_str(), "done" | "blocked" | "archived") {
            let label = match destination.as_str() {
                "done" => "Done",
                "blocked" => "Blocked",
                _ => "Archived",
            };
            let hard = if destination == "blocked" {
                ""
            } else {
                " This is hard to undo."
            };
            let children = if destination == "archived"
                && card.child_titles.iter().any(|(_, s)| s != "done")
            {
                " Archiving releases its child cards without this card's result."
            } else {
                ""
            };
            self.mode = BoardMode::Confirm {
                destination,
                prompt: format!("Move '{}' to {label}?{hard}{children}", card.title),
            };
            vec![]
        } else {
            self.mode = BoardMode::Browse;
            vec![KanbanEffect::Patch {
                id: card.id,
                status: destination,
                summary: None,
            }]
        }
    }
}
pub fn age(ms: u64) -> String {
    let seconds = ms / 1000;
    let (n, suffix) = if seconds >= 86400 {
        (seconds / 86400, "d")
    } else if seconds >= 3600 {
        (seconds / 3600, "h")
    } else if seconds >= 60 {
        (seconds / 60, "m")
    } else {
        (seconds, "s")
    };
    format!("{n}{suffix}")
}
pub fn truncate(text: &str) -> String {
    let count = text.chars().count();
    if count <= 2000 {
        text.into()
    } else {
        format!(
            "{} (+{} chars, full text on card)",
            text.chars().take(2000).collect::<String>(),
            count - 2000
        )
    }
}
pub fn update(state: &mut KanbanBoardState, input: KanbanInput) -> Vec<KanbanEffect> {
    match input {
        KanbanInput::Snapshot { cards, cursor } => {
            state.load_cards(cards);
            state.cursor = state.cursor.max(cursor);
            state.loading = false;
            state.error = None;
            state.clamp();
            if state
                .detail_id
                .as_ref()
                .is_some_and(|id| state.card(id).is_none())
            {
                state.detail_id = None;
                state.detail_fields.clear();
            }
            if !state.live && !state.parked && !state.connecting {
                state.connecting = true;
                return vec![KanbanEffect::StartTail { cursor }];
            }
        }
        KanbanInput::Workflow { table, labels } => {
            state.load_workflow(table);
            state.labels = labels;
        }
        KanbanInput::Detail { card, fields } => {
            if let Some(old) = state.cards.iter_mut().find(|c| c.id == card.id) {
                *old = card;
            }
            state.detail_fields = fields;
        }
        KanbanInput::Boards(slugs) => state.mode = BoardMode::Boards { slugs, selected: 0 },
        KanbanInput::Tail {
            cursor,
            events,
            now_ms,
        } => {
            state.now_ms = now_ms;
            state.cursor = state.cursor.max(cursor);
            state.connecting = false;
            state.live = true;
            state.parked = false;
            if events {
                state.dirty = true;
                if !state.fetching
                    && state
                        .last_fetch_ms
                        .is_none_or(|last| now_ms.saturating_sub(last) >= 250)
                {
                    return state.refresh(false);
                }
            }
        }
        KanbanInput::TailParked => {
            state.connecting = false;
            state.parked = true;
            state.live = false;
        }
        KanbanInput::FetchFinished { now_ms } => {
            state.fetching = false;
            state.now_ms = now_ms;
        }
        KanbanInput::Tick { now_ms } => {
            state.now_ms = now_ms;
            if state.dirty
                && !state.fetching
                && state
                    .last_fetch_ms
                    .is_none_or(|last| now_ms.saturating_sub(last) >= 250)
            {
                return state.refresh(false);
            }
        }
        KanbanInput::Failed(error) => {
            state.fetching = false;
            state.loading = false;
            state.notice = Some(error.clone());
            if state.cards.is_empty() {
                state.error = Some(error);
            }
        }
        KanbanInput::Notice(notice) => state.notice = Some(notice),
        KanbanInput::Mutated => return state.refresh(false),
        KanbanInput::Key(key) => {
            state.notice = None;
            let mode = state.mode.clone();
            match mode {
                BoardMode::Moves {
                    destinations,
                    mut selected,
                } => {
                    match key {
                        BoardKey::Esc => state.mode = BoardMode::Browse,
                        BoardKey::Char('j') => {
                            selected = (selected + 1).min(destinations.len().saturating_sub(1))
                        }
                        BoardKey::Char('k') => selected = selected.saturating_sub(1),
                        BoardKey::Enter => {
                            if let Some(dst) = destinations.get(selected) {
                                return state.choose_move(dst.clone());
                            }
                        }
                        BoardKey::Char(c) if ('1'..='9').contains(&c) => {
                            if let Some(dst) = destinations.get((c as u8 - b'1') as usize) {
                                return state.choose_move(dst.clone());
                            }
                        }
                        _ => {}
                    }
                    if !matches!(state.mode, BoardMode::Browse) {
                        state.mode = BoardMode::Moves {
                            destinations,
                            selected,
                        };
                    }
                }
                BoardMode::Boards {
                    slugs,
                    mut selected,
                } => {
                    let choose = match key {
                        BoardKey::Esc => {
                            state.mode = BoardMode::Browse;
                            false
                        }
                        BoardKey::Char('j') => {
                            selected = (selected + 1).min(slugs.len().saturating_sub(1));
                            false
                        }
                        BoardKey::Char('k') => {
                            selected = selected.saturating_sub(1);
                            false
                        }
                        BoardKey::Char(c) if ('1'..='9').contains(&c) => {
                            selected = (c as u8 - b'1') as usize;
                            true
                        }
                        BoardKey::Enter => true,
                        _ => false,
                    };
                    if choose && let Some(slug) = slugs.get(selected) {
                        let (next, mut effects) = KanbanBoardState::open(slug, state.now_ms);
                        *state = next;
                        effects.insert(0, KanbanEffect::StopTail);
                        return effects;
                    }
                    if !matches!(state.mode, BoardMode::Browse) {
                        state.mode = BoardMode::Boards { slugs, selected };
                    }
                }
                BoardMode::Confirm { destination, .. } => match key {
                    BoardKey::Esc => state.mode = BoardMode::Browse,
                    BoardKey::Enter => {
                        state.mode = BoardMode::Browse;
                        if destination == "done" {
                            state.mode = BoardMode::Evidence {
                                text: String::new(),
                            };
                        } else if let Some(card) = state.selected() {
                            return vec![KanbanEffect::Patch {
                                id: card.id.clone(),
                                status: destination,
                                summary: None,
                            }];
                        }
                    }
                    _ => {}
                },
                BoardMode::Evidence { mut text } | BoardMode::Comment { mut text } => {
                    let evidence = matches!(state.mode, BoardMode::Evidence { .. });
                    match key {
                        BoardKey::Esc => {
                            state.mode = BoardMode::Browse;
                            return vec![];
                        }
                        BoardKey::Backspace => {
                            text.pop();
                        }
                        BoardKey::Char(c) => text.push(c),
                        BoardKey::Enter => {
                            state.mode = BoardMode::Browse;
                            if text.trim().is_empty() {
                                state.notice = Some(
                                    if evidence {
                                        "completion evidence is empty; move cancelled"
                                    } else {
                                        "comment is empty; nothing sent"
                                    }
                                    .into(),
                                );
                                return vec![];
                            }
                            if let Some(card) = state.selected() {
                                return vec![if evidence {
                                    KanbanEffect::Patch {
                                        id: card.id.clone(),
                                        status: "done".into(),
                                        summary: Some(text),
                                    }
                                } else {
                                    KanbanEffect::Comment {
                                        id: card.id.clone(),
                                        body: text,
                                    }
                                }];
                            }
                            return vec![];
                        }
                    }
                    state.mode = if evidence {
                        BoardMode::Evidence { text }
                    } else {
                        BoardMode::Comment { text }
                    };
                }
                BoardMode::Filter => match key {
                    BoardKey::Esc => {
                        state.filter.clear();
                        state.mode = BoardMode::Browse;
                    }
                    BoardKey::Enter => state.mode = BoardMode::Browse,
                    BoardKey::Backspace => {
                        state.filter.pop();
                    }
                    BoardKey::Char(c) => state.filter.push(c),
                },
                BoardMode::Browse => match key {
                    BoardKey::Char('h') => {
                        state.column = state.column.saturating_sub(1);
                        state.row = 0;
                    }
                    BoardKey::Char('l') => {
                        state.column =
                            (state.column + 1).min(state.column_keys().len().saturating_sub(1));
                        state.row = 0;
                    }
                    BoardKey::Char('j') => state.row += 1,
                    BoardKey::Char('k') => state.row = state.row.saturating_sub(1),
                    BoardKey::Enter => {
                        if let Some(card) = state.selected() {
                            let id = card.id.clone();
                            state.detail_id = Some(id.clone());
                            return vec![KanbanEffect::Detail(id)];
                        }
                    }
                    BoardKey::Char('m') if state.writes_allowed() => {
                        if let Some(card) = state.selected() {
                            let id = card.id.clone();
                            let destinations = state.offered(&card.status);
                            state.mode = BoardMode::Moves {
                                destinations,
                                selected: 0,
                            };
                            return vec![KanbanEffect::Detail(id)];
                        }
                    }
                    BoardKey::Char('c') if state.writes_allowed() => {
                        state.mode = BoardMode::Comment {
                            text: String::new(),
                        }
                    }
                    BoardKey::Char('d') if state.writes_allowed() => {
                        return vec![KanbanEffect::Dispatch];
                    }
                    BoardKey::Char('b') => return vec![KanbanEffect::Boards],
                    BoardKey::Char('r') => {
                        if state.parked {
                            state.parked = false;
                            state.live = false;
                        }
                        return state.refresh(false);
                    }
                    BoardKey::Char('/') => state.mode = BoardMode::Filter,
                    BoardKey::Esc => {
                        if state.detail_id.take().is_some() {
                            state.detail_fields.clear();
                        } else if !state.filter.is_empty() {
                            state.filter.clear();
                        } else {
                            return vec![KanbanEffect::StopTail, KanbanEffect::Close];
                        }
                    }
                    _ => {}
                },
            }
            state.clamp();
        }
    }
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ready() -> KanbanBoardState {
        let mut state = KanbanBoardState::blank();
        state.load_workflow(Some(MoveTable {
            columns: vec!["ready".into(), "done".into()],
            manual: BTreeMap::from([("ready".into(), vec!["done".into()])]),
        }));
        state.load_cards(vec![BoardCard {
            id: "card".into(),
            title: "Work".into(),
            assignee: "fixture".into(),
            status: "ready".into(),
            created_at_ms: 0,
            started_at_ms: None,
            parent_titles: vec![],
            child_titles: vec![],
            body: String::new(),
            latest_summary: None,
            comments: vec![],
        }]);
        state
    }
    #[test]
    fn destructive_move_waits_for_confirmation_and_nonempty_evidence() {
        let mut s = ready();
        assert!(
            update(&mut s, KanbanInput::Key(BoardKey::Char('m')))
                .iter()
                .all(|e| matches!(e, KanbanEffect::Detail(_)))
        );
        assert!(update(&mut s, KanbanInput::Key(BoardKey::Enter)).is_empty());
        assert!(matches!(s.mode, BoardMode::Confirm { .. }));
        assert!(update(&mut s, KanbanInput::Key(BoardKey::Enter)).is_empty());
        update(&mut s, KanbanInput::Key(BoardKey::Char(' ')));
        assert!(update(&mut s, KanbanInput::Key(BoardKey::Enter)).is_empty());
        assert_eq!(
            s.notice.as_deref(),
            Some("completion evidence is empty; move cancelled")
        );
        s.mode = BoardMode::Evidence {
            text: "Verified".into(),
        };
        assert_eq!(
            update(&mut s, KanbanInput::Key(BoardKey::Enter)),
            vec![KanbanEffect::Patch {
                id: "card".into(),
                status: "done".into(),
                summary: Some("Verified".into())
            }]
        );
        assert_eq!(s.card("card").unwrap().status, "ready");
    }
    #[test]
    fn burst_refetches_have_one_flight_and_a_quarter_second_window() {
        let mut s = ready();
        let event = |now_ms| KanbanInput::Tail {
            cursor: 5,
            events: true,
            now_ms,
        };
        assert_eq!(update(&mut s, event(1000)).len(), 1);
        assert!(update(&mut s, event(1050)).is_empty());
        update(&mut s, KanbanInput::FetchFinished { now_ms: 1100 });
        assert!(update(&mut s, KanbanInput::Tick { now_ms: 1249 }).is_empty());
        assert_eq!(update(&mut s, KanbanInput::Tick { now_ms: 1250 }).len(), 1);
    }
    #[test]
    fn missing_workflow_prevents_every_write_and_other_only_archives() {
        let mut s = ready();
        assert_eq!(s.offered("holding"), ["archived"]);
        s.load_workflow(None);
        for key in ['m', 'c', 'd'] {
            assert!(update(&mut s, KanbanInput::Key(BoardKey::Char(key))).is_empty());
        }
        assert!(s.frame().status_line.contains("board is read-only"));
        s.load_workflow(Some(MoveTable {
            columns: vec![],
            manual: BTreeMap::new(),
        }));
        assert!(!s.writes_allowed());
        assert!(s.offered("ready").is_empty());
    }
}
