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
    let _ = (action, focus);
    todo!("#228")
}
