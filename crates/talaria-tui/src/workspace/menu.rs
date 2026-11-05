//! One-row menu: headings, dropdown, and the keys that move through them.
//!
//! Drawing and key handling live here. The layout rules stay in the core crate.

use ratatui::{buffer::Buffer, layout::Rect};

/// What one menu item runs. Every binding names an action that already exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuBinding {
    /// A configurable key action, by its configuration name.
    KeyAction(&'static str),
    /// A built-in local command, without a leading slash.
    LocalCommand(&'static str),
    /// `/voice read` with no argument. The label carries no phase.
    VoiceRead,
}

/// One row inside a menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    /// Heading the item belongs to.
    pub menu: &'static str,
    /// Visible label.
    pub label: &'static str,
    /// Existing action this item runs.
    pub binding: MenuBinding,
    /// False when the item can be shown but not run.
    pub enabled: bool,
    /// Why a disabled item refuses. Empty when the item can run.
    pub reason: String,
    /// True when the item reflects a preference that is currently on.
    pub checked: bool,
}

/// A key the menu understands while it is on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKey {
    Left,
    Right,
    Up,
    Down,
    Enter,
    Tab,
    /// Shift+Tab.
    BackTab,
    Esc,
    Letter(char),
}

/// Result of one menu key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuOutcome {
    /// The menu took the key, so the composer must not see it.
    pub consumed: bool,
    /// The item Enter ran. None when nothing ran.
    pub activated: Option<MenuBinding>,
    /// A sentence shown for a disabled item. None when there is nothing to say.
    pub notice: Option<String>,
}

/// Runtime menu. Hidden and unfocused is the default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuState {
    /// The row or the transient menu is on screen.
    pub visible: bool,
    /// Keyboard focus is in the menu.
    pub focused: bool,
    /// A dropdown is open.
    pub dropdown_open: bool,
    /// Shown for one command while the preference stays off.
    pub transient: bool,
    /// Heading index. Session is 0, then Model, View, Voice, Settings, and Help.
    pub menu_index: usize,
    /// Item index inside the open dropdown.
    pub item_index: usize,
    /// When false, focus still has a marker that is not a colour.
    pub color: bool,
    /// Fleet commands can run.
    pub fleet_enabled: bool,
    /// Voice is refused because the gateway is remote.
    pub remote_gateway: bool,
    /// The work panel preference is on, so its menu item is checked.
    pub panel_on: bool,
    /// Composer draft the menu must leave unchanged.
    pub draft: String,
    /// Composer selection the menu must leave unchanged.
    pub selection: std::ops::Range<usize>,
}

impl Default for MenuState {
    fn default() -> Self {
        Self {
            visible: false,
            focused: false,
            dropdown_open: false,
            transient: false,
            menu_index: 0,
            item_index: 0,
            color: true,
            fleet_enabled: true,
            remote_gateway: false,
            panel_on: false,
            draft: String::new(),
            selection: 0..0,
        }
    }
}

impl MenuState {
    /// No row and no focus.
    #[must_use]
    pub fn hidden() -> Self {
        Self::default()
    }

    /// The row is on screen and focused. The dropdown stays closed.
    #[must_use]
    pub fn shown() -> Self {
        Self {
            visible: true,
            focused: true,
            ..Self::default()
        }
    }

    /// Items for this state. Preferences decide which rows are disabled or checked.
    #[must_use]
    pub fn items(&self) -> Vec<MenuItem> {
        let _ = self;
        Vec::new()
    }

    /// Apply one menu key. Composer text is not part of the menu.
    pub fn on_key(&mut self, key: MenuKey) -> MenuOutcome {
        let _ = key;
        MenuOutcome {
            consumed: false,
            activated: None,
            notice: None,
        }
    }

    /// Paint the menu into `area`. The first row is the headings.
    pub fn draw(&self, area: Rect, buffer: &mut Buffer) {
        let _ = (self, area, buffer);
    }
}
