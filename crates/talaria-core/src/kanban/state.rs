//! Board reducer. A blank board offers no moves and allows no writes.

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
}

impl KanbanBoardState {
    pub fn blank() -> Self {
        Self {
            workflow: None,
            cards: Vec::new(),
        }
    }

    pub fn load_workflow(&mut self, table: Option<MoveTable>) {
        let _ = table;
        self.workflow = None;
    }

    pub fn load_cards(&mut self, cards: Vec<BoardCard>) {
        let _ = cards;
    }

    pub fn offered(&self, status: &str) -> Vec<String> {
        let _ = status;
        Vec::new()
    }

    pub fn writes_allowed(&self) -> bool {
        false
    }

    pub fn card(&self, id: &str) -> Option<&BoardCard> {
        self.cards.iter().find(|card| card.id == id)
    }
}
