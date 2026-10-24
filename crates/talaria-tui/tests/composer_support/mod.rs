//! Scripts and a fake clock for composer tests.

#![allow(dead_code)]

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use talaria_tui::composer::{Composer, ComposerContext, ComposerOutcome};
use talaria_tui::keys::{Key, KeyMap};
use talaria_tui::live::LiveSection;

/// Keys described by a script. A word of letters is typed; a chord name is one key.
#[must_use]
pub fn keys(script: &str) -> Vec<Key> {
    tokens(script)
        .into_iter()
        .flat_map(|token| token_keys(&token))
        .collect()
}

/// A composer on a clock the test moves itself.
pub struct Rig {
    /// The draft under test.
    pub composer: Composer,
    /// The instant passed to the next key or tick.
    pub now: Instant,
    turn_running: bool,
}

impl Rig {
    /// Default bindings, empty draft, idle turn.
    #[must_use]
    pub fn new() -> Self {
        Self::from_composer(Composer::new(KeyMap::defaults()))
    }

    /// Default bindings and a seeded history.
    #[must_use]
    pub fn with_history(entries: Vec<String>) -> Self {
        Self::from_composer(Composer::with_history(KeyMap::defaults(), entries))
    }

    /// A composer the caller already configured.
    #[must_use]
    pub fn from_composer(composer: Composer) -> Self {
        Self {
            composer,
            now: Instant::now(),
            turn_running: false,
        }
    }

    /// Set whether a turn is running for later key presses.
    pub fn set_running(&mut self, running: bool) {
        self.turn_running = running;
    }

    /// Type `script`, 100 ms between keys, and return each outcome.
    pub fn press(&mut self, script: &str) -> Vec<ComposerOutcome> {
        let mut outcomes = Vec::new();
        for key in keys(script) {
            self.now += Duration::from_millis(100);
            outcomes.push(self.composer.handle_key(
                key,
                ComposerContext {
                    turn_running: self.turn_running,
                },
                self.now,
            ));
        }
        outcomes
    }

    /// Type `text` one millisecond apart, as a burst would arrive.
    pub fn burst(&mut self, text: &str) -> Vec<ComposerOutcome> {
        let mut outcomes = Vec::new();
        for ch in text.chars() {
            self.now += Duration::from_millis(1);
            let key = Key::from(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
            outcomes.push(self.composer.handle_key(
                key,
                ComposerContext {
                    turn_running: self.turn_running,
                },
                self.now,
            ));
        }
        outcomes
    }

    /// Move the fake clock forward.
    pub fn advance(&mut self, ms: u64) {
        self.now += Duration::from_millis(ms);
    }

    /// Run a tick at the current fake time.
    pub fn tick(&mut self) -> ComposerOutcome {
        self.composer.tick(self.now)
    }
}

/// Cell symbols for each row, with trailing spaces removed.
#[must_use]
pub fn rows(section: &impl LiveSection, width: u16, height: u16) -> Vec<String> {
    let buffer = draw(section, width, height);
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

/// Modifiers on one cell of a drawn section.
#[must_use]
pub fn style_at(section: &impl LiveSection, width: u16, height: u16, x: u16, y: u16) -> Modifier {
    draw(section, width, height)[(x, y)].style().add_modifier
}

/// Symbol in one cell of a drawn section. An empty cell is a space.
#[must_use]
pub fn symbol_at(section: &impl LiveSection, width: u16, height: u16, x: u16, y: u16) -> String {
    draw(section, width, height)[(x, y)].symbol().to_string()
}

fn draw(section: &impl LiveSection, width: u16, height: u16) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    section.render(area, &mut buffer);
    buffer
}

fn tokens(script: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut rest = script.trim();
    while !rest.is_empty() {
        if let Some(body) = rest.strip_prefix('\'') {
            let end = body.find('\'').unwrap_or(body.len());
            tokens.push(body[..end].to_string());
            rest = body[end..].trim_start_matches('\'').trim_start();
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            tokens.push(rest[..end].to_string());
            rest = rest[end..].trim_start();
        }
    }
    tokens
}

fn token_keys(token: &str) -> Vec<Key> {
    if token.contains('+') || is_named_key(token) {
        vec![Key::from(chord_event(token))]
    } else {
        token
            .chars()
            .map(|ch| {
                let mods = if ch.is_ascii_uppercase() {
                    KeyModifiers::SHIFT
                } else {
                    KeyModifiers::NONE
                };
                let code = if ch.is_ascii_alphabetic() {
                    KeyCode::Char(if mods == KeyModifiers::SHIFT {
                        ch.to_ascii_uppercase()
                    } else {
                        ch
                    })
                } else {
                    KeyCode::Char(ch)
                };
                Key::from(KeyEvent::new(code, mods))
            })
            .collect()
    }
}

fn is_named_key(token: &str) -> bool {
    named_code(&token.to_ascii_lowercase()).is_some()
}

fn chord_event(token: &str) -> KeyEvent {
    let parts: Vec<&str> = token.split('+').map(str::trim).collect();
    let (key, modifiers) = parts.split_last().unwrap_or((&"", &[]));
    let mut mods = KeyModifiers::NONE;
    for modifier in modifiers {
        mods |= match modifier.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => KeyModifiers::CONTROL,
            "alt" | "option" => KeyModifiers::ALT,
            "shift" => KeyModifiers::SHIFT,
            "super" => KeyModifiers::SUPER,
            _ => panic!("script has an unknown modifier in {token}"),
        };
    }
    if let Some(code) = named_code(&key.to_ascii_lowercase()) {
        return KeyEvent::new(code, mods);
    }
    let mut chars = key.chars();
    let Some(ch) = chars.next() else {
        panic!("script chord {token} has no key");
    };
    if chars.next().is_some() {
        panic!("script chord {token} is not one key");
    }
    if ch.is_ascii_alphabetic() {
        let implicit_shift = mods.is_empty() && ch.is_ascii_uppercase();
        if mods.contains(KeyModifiers::SHIFT) || implicit_shift {
            mods |= KeyModifiers::SHIFT;
        }
        let code = if mods == KeyModifiers::SHIFT {
            KeyCode::Char(ch.to_ascii_uppercase())
        } else {
            KeyCode::Char(ch.to_ascii_lowercase())
        };
        return KeyEvent::new(code, mods);
    }
    KeyEvent::new(KeyCode::Char(ch), mods)
}

fn named_code(key: &str) -> Option<KeyCode> {
    Some(match key {
        "enter" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "space" => KeyCode::Char(' '),
        "plus" => KeyCode::Char('+'),
        other => {
            let number = other.strip_prefix('f')?.parse::<u8>().ok()?;
            if (1..=24).contains(&number) {
                KeyCode::F(number)
            } else {
                return None;
            }
        }
    })
}
