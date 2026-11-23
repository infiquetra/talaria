//! A short inline selector: rows with a name and a description, the current entry marked,
//! typing to filter, Up and Down to move, Enter to apply and Esc to cancel.
//!
//! `/theme` and `/skin` open it under the composer. It only says what the person chose; the
//! application decides what a preview, an apply or a cancel does.

use super::palette::{PaletteRow, RowKind};

/// One entry of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoiceItem {
    /// What the application acts on. Unique in the list.
    pub id: String,
    /// The name shown in the first column.
    pub name: String,
    /// One line describing the entry.
    pub description: String,
    /// The group title drawn above the entry's run of rows.
    pub group: Option<String>,
    /// This entry is in force now.
    pub current: bool,
}

/// One key for the open list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChoiceKey {
    /// Select the next visible entry, wrapping at the end.
    Next,
    /// Select the previous visible entry, wrapping at the start.
    Prev,
    /// Enter.
    Apply,
    /// Esc.
    Cancel,
    /// A printable character for the filter.
    Type(char),
    /// Backspace in the filter.
    Erase,
}

/// What the application should do after one key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChoiceEffect {
    /// The selection moved to this entry.
    Preview(String),
    /// Enter on this entry.
    Apply(String),
    /// Esc: put back what was in force when the list opened.
    Cancel,
    /// Only the list changed.
    Redraw,
}

/// The open list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoiceList {
    items: Vec<ChoiceItem>,
    filter: String,
    /// Id of the selected entry. It survives a filter that hides it.
    selected: Option<String>,
    note: Option<String>,
}

impl ChoiceList {
    /// A list with the first current entry selected, else the first entry.
    #[must_use]
    pub fn new(items: Vec<ChoiceItem>) -> Self {
        let selected = items
            .iter()
            .find(|item| item.current)
            .or_else(|| items.first())
            .map(|item| item.id.clone());
        Self {
            items,
            filter: String::new(),
            selected,
            note: None,
        }
    }

    /// The filter typed so far.
    #[must_use]
    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// A line the footer shows before the key hints.
    #[must_use]
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// Replace the footer note.
    pub fn set_note(&mut self, note: Option<String>) {
        self.note = note;
    }

    /// Every entry, filtered or not.
    #[must_use]
    pub fn items(&self) -> &[ChoiceItem] {
        &self.items
    }

    /// The selected entry, when the filter shows it.
    #[must_use]
    pub fn selected(&self) -> Option<&ChoiceItem> {
        let id = self.selected.as_deref()?;
        self.visible().into_iter().find(|item| item.id == id)
    }

    /// Entries the filter keeps: a case-insensitive match on the name or the description.
    fn visible(&self) -> Vec<&ChoiceItem> {
        let filter = self.filter.to_lowercase();
        self.items
            .iter()
            .filter(|item| {
                filter.is_empty()
                    || item.name.to_lowercase().contains(&filter)
                    || item.description.to_lowercase().contains(&filter)
            })
            .collect()
    }

    /// The rows to draw: a header for each group, then its entries; the current entry ends in
    /// `(current)`. With nothing matching, one status row says so.
    #[must_use]
    pub fn rows(&self) -> Vec<PaletteRow> {
        let visible = self.visible();
        if visible.is_empty() {
            return vec![PaletteRow {
                kind: RowKind::Status,
                name: format!("No match for \"{}\"", self.filter),
                description: String::new(),
                source: None,
                refused: false,
                takes_argument: false,
                group: None,
            }];
        }
        let mut rows = Vec::new();
        let mut group: Option<&str> = None;
        for item in visible {
            if let Some(title) = item.group.as_deref()
                && group != Some(title)
            {
                rows.push(PaletteRow {
                    kind: RowKind::Header,
                    name: title.to_owned(),
                    description: String::new(),
                    source: None,
                    refused: false,
                    takes_argument: false,
                    group: None,
                });
            }
            group = item.group.as_deref();
            let name = if item.current {
                format!("{} (current)", item.name)
            } else {
                item.name.clone()
            };
            rows.push(PaletteRow {
                kind: RowKind::Command,
                name,
                description: item.description.clone(),
                source: None,
                refused: false,
                takes_argument: false,
                group: item.group.clone(),
            });
        }
        rows
    }

    /// Index in [`Self::rows`] of the selected entry, or 0 when none is shown.
    #[must_use]
    pub fn selected_row(&self) -> usize {
        let Some(id) = self.selected().map(|item| item.id.clone()) else {
            return 0;
        };
        let visible = self.visible();
        let mut row = 0;
        let mut group: Option<&str> = None;
        for item in visible {
            if item.group.is_some() && item.group.as_deref() != group {
                row += 1;
            }
            group = item.group.as_deref();
            if item.id == id {
                return row;
            }
            row += 1;
        }
        0
    }

    /// Apply one key.
    pub fn update(&mut self, key: ChoiceKey) -> ChoiceEffect {
        match key {
            ChoiceKey::Next | ChoiceKey::Prev => {
                let visible = self.visible();
                if visible.is_empty() {
                    return ChoiceEffect::Redraw;
                }
                let count = visible.len();
                let at = self
                    .selected
                    .as_deref()
                    .and_then(|id| visible.iter().position(|item| item.id == id));
                let next = match (at, key) {
                    (None, _) => 0,
                    (Some(at), ChoiceKey::Next) => (at + 1) % count,
                    (Some(at), _) => (at + count - 1) % count,
                };
                let id = visible[next].id.clone();
                self.select(id)
            }
            ChoiceKey::Apply => match self.selected() {
                Some(item) => ChoiceEffect::Apply(item.id.clone()),
                None => ChoiceEffect::Redraw,
            },
            ChoiceKey::Cancel => ChoiceEffect::Cancel,
            ChoiceKey::Type(ch) => {
                if ch.is_control() {
                    return ChoiceEffect::Redraw;
                }
                self.filter.push(ch);
                self.refilter()
            }
            ChoiceKey::Erase => {
                self.filter.pop();
                self.refilter()
            }
        }
    }

    /// Keep the selection when the filter still shows it, else move to the first match.
    fn refilter(&mut self) -> ChoiceEffect {
        if self.selected().is_some() {
            return ChoiceEffect::Redraw;
        }
        match self.visible().first().map(|item| item.id.clone()) {
            Some(id) => self.select(id),
            None => ChoiceEffect::Redraw,
        }
    }

    fn select(&mut self, id: String) -> ChoiceEffect {
        if self.selected.as_deref() == Some(id.as_str()) {
            return ChoiceEffect::Redraw;
        }
        self.selected = Some(id.clone());
        ChoiceEffect::Preview(id)
    }
}
