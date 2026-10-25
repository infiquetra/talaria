//! Routing of comfort keys to their owner. Pure. Behavior contract: issue 228.

use crate::keys::Action;

/// Who holds the focus when a key arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Focus {
    /// The draft box.
    Composer,
    /// The history search box.
    Search,
    /// A prompt card.
    Card,
    /// The queued prompt picker.
    QueueSelection,
    /// An overlay screen.
    Overlay,
}

/// Who receives the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Route {
    /// Suspend the process.
    Suspend,
    /// The search box takes the raw key.
    Search,
    /// The card takes the raw key.
    Card,
    /// The queue picker takes the raw key.
    QueueSelection,
    /// The overlay takes the raw key.
    Overlay,
    /// Open the draft in the external editor.
    OpenEditor,
    /// Open history search.
    HistorySearch,
    /// The composer takes the raw key.
    Composer,
}

/// Pick the owner for a resolved action under a focus.
pub fn route(action: Option<Action>, focus: Focus) -> Route {
    if action == Some(Action::Suspend) {
        return Route::Suspend;
    }
    match focus {
        Focus::Overlay => Route::Overlay,
        Focus::Search => Route::Search,
        Focus::Card => Route::Card,
        Focus::QueueSelection => Route::QueueSelection,
        Focus::Composer => match action {
            Some(Action::OpenEditor) => Route::OpenEditor,
            Some(Action::HistorySearch) => Route::HistorySearch,
            _ => Route::Composer,
        },
    }
}
