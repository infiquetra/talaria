//! One-row menu: headings, dropdown, and the keys that move through them.
//!
//! Drawing and key handling live here. The layout rules stay in the core crate.

use crate::theme::surfaces::{ViewStyles, tokens as t};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
};

const HEADINGS: [&str; 6] = ["Session", "Model", "View", "Voice", "Settings", "Help"];
const FLEET_OFF: &str =
    "The fleet view is off: [fleet] enabled = false in Talaria's configuration.";
const REMOTE_VOICE: &str = "Voice needs a local Hermes gateway for now, because a remote gateway would record and play on its own machine.";

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
    /// The persistent menu-bar preference is on.
    pub bar_on: bool,
    /// In a narrow terminal, the single menu contains all actions.
    pub compact: bool,
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
            bar_on: false,
            compact: false,
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
            bar_on: true,
            ..Self::default()
        }
    }

    /// Items for this state. Preferences decide which rows are disabled or checked.
    #[must_use]
    pub fn items(&self) -> Vec<MenuItem> {
        use MenuBinding::{KeyAction as Key, LocalCommand as Command, VoiceRead};
        let entries = [
            ("Session", "Sessions", Key("session_picker_open")),
            ("Session", "Fleet", Key("fleet_view_open")),
            ("Session", "Next waiting request", Key("attention_next")),
            ("Model", "Choose model", Command("model")),
            ("Model", "Choose reasoning effort", Command("effort")),
            ("View", "Right work panel", Key("layout_panel")),
            ("View", "Top bar", Key("toggle_menu_bar")),
            ("View", "Tasks", Key("toggle_tasks")),
            ("View", "Agents", Key("agents_panel")),
            ("View", "Transcript", Key("transcript_open")),
            ("Voice", "Record or stop", Key("voice_record")),
            ("Voice", "Read-aloud", VoiceRead),
            ("Settings", "Talaria settings", Command("config")),
            ("Settings", "Providers", Command("providers")),
            ("Settings", "MCP servers", Command("mcp")),
            ("Settings", "Profiles", Command("profiles")),
            ("Help", "Gateway health check", Command("doctor")),
        ];
        entries
            .into_iter()
            .map(|(menu, label, binding)| {
                let reason = if !self.fleet_enabled
                    && matches!(binding, Key("fleet_view_open") | Key("attention_next"))
                {
                    FLEET_OFF
                } else if self.remote_gateway && matches!(binding, Key("voice_record") | VoiceRead)
                {
                    REMOTE_VOICE
                } else {
                    ""
                };
                MenuItem {
                    menu,
                    label,
                    binding,
                    enabled: reason.is_empty(),
                    reason: reason.into(),
                    checked: (label == "Right work panel" && self.panel_on)
                        || (label == "Top bar" && self.bar_on),
                }
            })
            .collect()
    }

    /// Apply one menu key. Composer text is not part of the menu.
    pub fn on_key(&mut self, key: MenuKey) -> MenuOutcome {
        let mut outcome = MenuOutcome {
            consumed: self.visible && self.focused,
            activated: None,
            notice: None,
        };
        if !outcome.consumed {
            return outcome;
        }
        match key {
            MenuKey::Left | MenuKey::BackTab => {
                self.menu_index = (self.menu_index + HEADINGS.len() - 1) % HEADINGS.len();
                self.item_index = 0;
            }
            MenuKey::Right | MenuKey::Tab => {
                self.menu_index = (self.menu_index + 1) % HEADINGS.len();
                self.item_index = 0;
            }
            MenuKey::Down if !self.dropdown_open => {
                self.dropdown_open = true;
                self.item_index = 0;
            }
            MenuKey::Down | MenuKey::Up if self.dropdown_open => {
                let items = self.items();
                let count = if self.compact {
                    items.len()
                } else {
                    items
                        .iter()
                        .filter(|i| i.menu == HEADINGS[self.menu_index])
                        .count()
                };
                if count > 0 {
                    self.item_index = if key == MenuKey::Down {
                        (self.item_index + 1) % count
                    } else {
                        (self.item_index + count - 1) % count
                    };
                }
            }
            MenuKey::Enter if !self.dropdown_open => {
                self.dropdown_open = true;
                self.item_index = 0;
            }
            MenuKey::Enter => {
                let items = self.items();
                let item = if self.compact {
                    items.into_iter().nth(self.item_index)
                } else {
                    items
                        .into_iter()
                        .filter(|i| i.menu == HEADINGS[self.menu_index])
                        .nth(self.item_index)
                };
                if let Some(item) = item {
                    if item.enabled {
                        outcome.activated = Some(item.binding);
                        self.dropdown_open = false;
                        self.focused = false;
                        if self.transient {
                            self.visible = false;
                        }
                    } else {
                        outcome.notice = Some(item.reason);
                    }
                }
            }
            MenuKey::Esc if self.dropdown_open => self.dropdown_open = false,
            MenuKey::Esc => {
                self.focused = false;
                if self.transient {
                    self.visible = false;
                }
            }
            _ => {}
        }
        outcome
    }

    /// Paint the menu into `area`. The first row is the headings.
    pub fn draw(&self, area: Rect, buffer: &mut Buffer) {
        self.draw_with_theme(area, buffer, &ViewStyles::default());
    }

    /// Draw the menu's row and open dropdown using an explicit palette.
    pub fn draw_with_theme(&self, area: Rect, buffer: &mut Buffer, views: &ViewStyles) {
        if !self.visible || area.is_empty() {
            return;
        }
        let row = Rect::new(area.x, area.y, area.width, 1);
        if views.active() {
            buffer.set_style(row, views.style(t::MENU_BACKGROUND, Style::default()));
        }
        let heading = if area.width < 60 {
            "Talaria   Menu"
        } else {
            "Talaria   Session   Model   View   Voice   Settings   Help"
        };
        buffer.set_stringn(
            area.x,
            area.y,
            heading,
            usize::from(area.width),
            views.style(t::MENU_ITEM, Style::default()),
        );
        if views.active() {
            buffer.set_stringn(
                area.x,
                area.y,
                "Talaria",
                usize::from(area.width),
                views.style(t::MENU_BRAND, Style::default()),
            );
        }
        if self.focused {
            let name = if area.width < 60 {
                "Menu"
            } else {
                HEADINGS[self.menu_index.min(5)]
            };
            if let Some(start) = heading.find(name) {
                let mut focused = views.style(
                    t::MENU_ITEM_FOCUSED,
                    Style::default().add_modifier(Modifier::REVERSED),
                );
                let style = if views.active() {
                    // A heading keeps the bar's item colours. Focus contributes
                    // attributes while the dropdown owns selected-row colours.
                    focused.fg = None;
                    focused.bg = None;
                    focused.underline_color = None;
                    views
                        .style(t::MENU_ITEM, Style::default())
                        .patch(focused)
                        .add_modifier(Modifier::REVERSED)
                } else {
                    focused
                };
                buffer.set_stringn(
                    area.x + start as u16,
                    area.y,
                    name,
                    usize::from(area.width.saturating_sub(start as u16)),
                    style,
                );
            }
        }
        if !self.dropdown_open || area.height < 2 {
            return;
        }
        let heading_index = self.menu_index.min(5);
        let x = if area.width < 60 {
            area.x
        } else {
            area.x + heading.find(HEADINGS[heading_index]).unwrap_or(0) as u16
        };
        let width = area.right().saturating_sub(x).min(30);
        let items = self.items();
        let items: Vec<_> = items
            .iter()
            .filter(|item| area.width < 60 || item.menu == HEADINGS[heading_index])
            .collect();
        let capacity = usize::from(area.height.saturating_sub(1));
        let offset = self.item_index.saturating_add(1).saturating_sub(capacity);
        for (index, item) in items.into_iter().enumerate().skip(offset) {
            let y = area.y.saturating_add(1 + (index - offset) as u16);
            if y >= area.bottom() {
                break;
            }
            let mark = if item.checked { "[x]" } else { "   " };
            let text = if area.width < 60 {
                format!(
                    "{mark} {}: {}{}",
                    item.menu,
                    item.label,
                    if item.enabled { "" } else { " (disabled)" }
                )
            } else {
                format!(
                    "{mark} {}{}",
                    item.label,
                    if item.enabled { "" } else { " (disabled)" }
                )
            };
            let style = if !item.enabled {
                views.style(t::MENU_DROPDOWN_DISABLED, Style::default())
            } else if self.focused && index == self.item_index {
                views.style(
                    t::MENU_DROPDOWN_SELECTED,
                    Style::default().add_modifier(Modifier::REVERSED),
                )
            } else {
                views.style(t::MENU_DROPDOWN_ITEM, Style::default())
            };
            let style = if views.active() {
                views
                    .style(t::MENU_DROPDOWN_BACKGROUND, Style::default())
                    .patch(style)
            } else if self.focused && index == self.item_index {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                style
            };
            buffer.set_stringn(
                x,
                y,
                " ".repeat(usize::from(width)),
                usize::from(width),
                style,
            );
            buffer.set_stringn(x, y, text, usize::from(width), style);
        }
    }
}
