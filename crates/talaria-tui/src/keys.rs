//! Plain key values, chord text, and the composer key map.

use std::collections::BTreeMap;

use crossterm::event::KeyEvent;

/// Where an action is looked up. Later screens add their own variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyContext {
    /// The draft box at the bottom of the screen.
    Composer,
}

/// One composer command a key chord can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    Submit,
    Newline,
    Interrupt,
    Cancel,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    LineStart,
    LineEnd,
    BufferStart,
    BufferEnd,
    WordLeft,
    WordRight,
    DeleteBackward,
    DeleteForward,
    DeleteWordBackward,
    KillToLineEnd,
    KillToLineStart,
    Yank,
}

impl Action {
    /// Every action, in the order used for conflicts and notices.
    pub const ALL: [Self; 20] = [
        Self::Submit,
        Self::Newline,
        Self::Interrupt,
        Self::Cancel,
        Self::MoveLeft,
        Self::MoveRight,
        Self::MoveUp,
        Self::MoveDown,
        Self::LineStart,
        Self::LineEnd,
        Self::BufferStart,
        Self::BufferEnd,
        Self::WordLeft,
        Self::WordRight,
        Self::DeleteBackward,
        Self::DeleteForward,
        Self::DeleteWordBackward,
        Self::KillToLineEnd,
        Self::KillToLineStart,
        Self::Yank,
    ];

    /// Name used in the keys table of the configuration file.
    #[must_use]
    pub const fn config_name(self) -> &'static str {
        match self {
            Self::Submit => "submit",
            Self::Newline => "newline",
            Self::Interrupt => "interrupt",
            Self::Cancel => "cancel",
            Self::MoveLeft => "move_left",
            Self::MoveRight => "move_right",
            Self::MoveUp => "move_up",
            Self::MoveDown => "move_down",
            Self::LineStart => "line_start",
            Self::LineEnd => "line_end",
            Self::BufferStart => "buffer_start",
            Self::BufferEnd => "buffer_end",
            Self::WordLeft => "word_left",
            Self::WordRight => "word_right",
            Self::DeleteBackward => "delete_backward",
            Self::DeleteForward => "delete_forward",
            Self::DeleteWordBackward => "delete_word_backward",
            Self::KillToLineEnd => "kill_to_line_end",
            Self::KillToLineStart => "kill_to_line_start",
            Self::Yank => "yank",
        }
    }

    /// The action named in the configuration file, if the name is known.
    #[must_use]
    pub fn from_config_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|action| action.config_name() == name)
    }

    /// The screen this action belongs to.
    #[must_use]
    pub const fn context(self) -> KeyContext {
        KeyContext::Composer
    }
}

/// A key after it has been turned into Talaria's own value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    /// The key that was pressed.
    pub code: KeyCode,
    /// Which modifier keys were held.
    pub mods: Mods,
    /// Press, repeat, or release.
    pub kind: KeyKind,
}

impl From<KeyEvent> for Key {
    fn from(_event: KeyEvent) -> Self {
        todo!()
    }
}

/// The key identity, without modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    /// A typed character.
    Char(char),
    /// Enter or return.
    Enter,
    /// Escape.
    Esc,
    /// Tab.
    Tab,
    /// Backspace.
    Backspace,
    /// Forward delete.
    Delete,
    /// Insert.
    Insert,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// A function key. `F(1)` is F1.
    F(u8),
    /// A key this list does not name. Shift+Tab arrives here.
    Other,
}

/// Modifier keys. `other` is set for hyper or meta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Mods {
    /// Control.
    pub ctrl: bool,
    /// Alt or option.
    pub alt: bool,
    /// Shift.
    pub shift: bool,
    /// Super or command.
    pub super_key: bool,
    /// Hyper or meta, which match no chord.
    pub other: bool,
}

/// Whether the key went down, repeated, or came up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyKind {
    /// The key was pressed.
    Press,
    /// The key is repeating.
    Repeat,
    /// The key was released. Releases never match a chord.
    Release,
}

/// One binding: modifiers plus a single key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChord;

impl KeyChord {
    /// Parse chord text such as `ctrl+j` or `shift+enter`.
    pub fn parse(_text: &str, _context: KeyContext) -> Result<Self, String> {
        todo!()
    }

    /// Whether this chord matches a key. Releases and hyper or meta never match.
    #[must_use]
    pub fn matches(&self, _key: &Key) -> bool {
        todo!()
    }

    /// Display text such as `Shift+Enter` or `Ctrl+C`.
    #[must_use]
    pub fn label(&self) -> String {
        todo!()
    }

    /// True when a terminal can send this chord only with keyboard enhancement.
    #[must_use]
    pub fn needs_enhancement(&self) -> bool {
        todo!()
    }
}

/// The chords bound to each composer action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyMap;

impl KeyMap {
    /// The built-in bindings.
    #[must_use]
    pub fn defaults() -> Self {
        todo!()
    }

    /// Apply the keys table from configuration on top of the defaults.
    #[must_use]
    pub fn from_entries(_entries: &BTreeMap<String, Vec<String>>) -> (Self, Vec<String>) {
        todo!()
    }

    /// The action bound to this key, if any.
    #[must_use]
    pub fn resolve(&self, _key: &Key) -> Option<Action> {
        todo!()
    }

    /// Chords bound to one action, in binding order.
    #[must_use]
    pub fn chords(&self, _action: Action) -> &[KeyChord] {
        todo!()
    }

    /// Label of the first chord bound to an action.
    #[must_use]
    pub fn label(&self, _action: Action) -> Option<String> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    use super::{Action, Key, KeyChord, KeyContext, KeyMap};

    fn parse(text: &str) -> Result<KeyChord, String> {
        KeyChord::parse(text, KeyContext::Composer)
    }

    fn labels(map: &KeyMap, action: Action) -> Vec<String> {
        map.chords(action).iter().map(KeyChord::label).collect()
    }

    fn entries(pairs: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
        pairs
            .iter()
            .map(|(name, chords)| {
                (
                    (*name).to_string(),
                    chords.iter().map(|chord| (*chord).to_string()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn chord_parse_accepts_and_refuses() {
        let ctrl_j = parse("ctrl+j").expect("ctrl+j");
        assert_eq!(ctrl_j.label(), "Ctrl+J");
        assert!(!ctrl_j.needs_enhancement());
        let shift_enter = parse("Shift+Enter").expect("Shift+Enter");
        assert_eq!(shift_enter.label(), "Shift+Enter");
        assert!(shift_enter.needs_enhancement());
        let f24 = parse("f24").expect("f24");
        assert_eq!(f24.label(), "F24");
        assert!(!f24.needs_enhancement());

        assert_eq!(parse("control+j").unwrap().label(), "Ctrl+J");
        assert_eq!(parse("option+enter").unwrap().label(), "Alt+Enter");
        assert_eq!(parse("escape").unwrap().label(), "Esc");

        let refused = [
            ("", "empty"),
            (" ", "empty"),
            ("   ", "empty"),
            ("+", "missing key"),
            ("ctrl+", "missing key"),
            ("ctrl++", "missing key"),
            ("hyper+a", "unknown modifier \"hyper\""),
            ("banana", "unknown key \"banana\""),
            ("a", "a plain letter would stop that key from typing"),
            ("A", "a plain letter would stop that key from typing"),
            ("shift+a", "a plain letter would stop that key from typing"),
            ("shift+x", "a plain letter would stop that key from typing"),
            ("f25", "unknown key \"f25\""),
            ("ctrl+ctrl+a", "repeated modifier \"ctrl\""),
        ];
        for (text, reason) in refused {
            assert_eq!(parse(text).unwrap_err(), reason, "{text:?}");
        }
    }

    #[test]
    fn chord_matches_crossterm_events() {
        let shift_enter = parse("shift+enter").unwrap();
        let plain_enter = parse("enter").unwrap();
        let shifted = Key::from(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
        assert!(shift_enter.matches(&shifted));
        assert!(!plain_enter.matches(&shifted));

        let map = KeyMap::defaults();
        let backspace = Key::from(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        let ctrl_h = Key::from(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL));
        assert_eq!(map.resolve(&backspace), Some(Action::DeleteBackward));
        assert_eq!(map.resolve(&ctrl_h), Some(Action::DeleteBackward));

        let hyper = Key::from(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::HYPER));
        assert_eq!(map.resolve(&hyper), None);
        assert!(!shift_enter.matches(&hyper));

        let back_tab = Key::from(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(map.resolve(&back_tab), None);

        let release = Key::from(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ));
        assert_eq!(map.resolve(&release), None);
        assert!(!plain_enter.matches(&release));

        let repeat = Key::from(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        ));
        assert_eq!(map.resolve(&repeat), Some(Action::Submit));
        assert!(plain_enter.matches(&repeat));
    }

    #[test]
    fn defaults_are_safe_and_unambiguous() {
        assert_eq!(Action::ALL.len(), 20);
        for action in Action::ALL {
            assert_eq!(action.context(), KeyContext::Composer);
            let name = action.config_name();
            assert_eq!(Action::from_config_name(name), Some(action));
        }
        assert_eq!(Action::from_config_name("not_an_action"), None);

        let map = KeyMap::defaults();
        let expected = [
            (Action::Submit, vec!["Enter"]),
            (Action::Newline, vec!["Shift+Enter", "Alt+Enter", "Ctrl+J"]),
            (Action::Interrupt, vec!["Esc"]),
            (Action::Cancel, vec!["Ctrl+C"]),
            (Action::MoveLeft, vec!["Left"]),
            (Action::MoveRight, vec!["Right"]),
            (Action::MoveUp, vec!["Up"]),
            (Action::MoveDown, vec!["Down"]),
            (Action::LineStart, vec!["Home", "Ctrl+A"]),
            (Action::LineEnd, vec!["End", "Ctrl+E"]),
            (Action::BufferStart, vec!["Ctrl+Home"]),
            (Action::BufferEnd, vec!["Ctrl+End"]),
            (Action::WordLeft, vec!["Alt+Left", "Ctrl+Left", "Alt+B"]),
            (Action::WordRight, vec!["Alt+Right", "Ctrl+Right", "Alt+F"]),
            (Action::DeleteBackward, vec!["Backspace", "Ctrl+H"]),
            (Action::DeleteForward, vec!["Delete"]),
            (Action::DeleteWordBackward, vec!["Ctrl+W", "Alt+Backspace"]),
            (Action::KillToLineEnd, vec!["Ctrl+K"]),
            (Action::KillToLineStart, vec!["Ctrl+U"]),
            (Action::Yank, vec!["Ctrl+Y"]),
        ];
        for (action, want) in expected {
            assert_eq!(labels(&map, action), want, "{}", action.config_name());
        }

        let risky = ["F1", "F2", "Ctrl+B", "Ctrl+S"];
        let mut seen: Vec<(KeyChord, Action)> = Vec::new();
        for action in Action::ALL {
            for chord in map.chords(action) {
                let label = chord.label();
                assert!(
                    !risky.contains(&label.as_str()),
                    "{label} is bound to {}",
                    action.config_name()
                );
                if let Some((_, other)) = seen.iter().find(|(held, _)| held == chord) {
                    panic!(
                        "{} shares {label} with {}",
                        action.config_name(),
                        other.config_name()
                    );
                }
                seen.push((chord.clone(), action));
            }
        }

        for (code, mods) in [
            (KeyCode::F(1), KeyModifiers::NONE),
            (KeyCode::F(2), KeyModifiers::NONE),
            (KeyCode::Char('b'), KeyModifiers::CONTROL),
            (KeyCode::Char('s'), KeyModifiers::CONTROL),
        ] {
            let key = Key::from(KeyEvent::new(code, mods));
            assert_eq!(map.resolve(&key), None);
        }
    }

    #[test]
    fn from_entries_reports_each_problem_once() {
        let (map, notices) = KeyMap::from_entries(&entries(&[
            ("bogus", &["x"]),
            ("delete_forward", &["ctrl+s"]),
            ("line_start", &["hyper+a"]),
            ("move_left", &["banana", "left"]),
            ("submit", &["alt+enter"]),
            ("word_left", &["ctrl+o"]),
            ("word_right", &["ctrl+o"]),
            ("yank", &[]),
        ]));
        let known = Action::ALL
            .iter()
            .map(|action| action.config_name())
            .collect::<Vec<_>>()
            .join(", ");
        let expected = [
            format!(
                "Ignoring [keys] bogus in ~/.talaria/config.toml: it is not a known action. \
                 Known actions: {known}."
            ),
            "[keys] delete_forward in ~/.talaria/config.toml: Ctrl+S may not reach Talaria in \
             some terminals (the system, Herdr or terminal flow control can take it); the \
             binding is kept."
                .to_string(),
            "[keys] line_start in ~/.talaria/config.toml: \"hyper+a\" is not a key Talaria \
             understands (unknown modifier \"hyper\"); ignoring it."
                .to_string(),
            "[keys] line_start in ~/.talaria/config.toml has no usable key; using the default \
             (Home, Ctrl+A)."
                .to_string(),
            "[keys] move_left in ~/.talaria/config.toml: \"banana\" is not a key Talaria \
             understands (unknown key \"banana\"); ignoring it."
                .to_string(),
            "[keys] word_right in ~/.talaria/config.toml: Ctrl+O is already bound to word_left; \
             keeping it on word_left only."
                .to_string(),
        ];
        assert_eq!(notices, expected);
        assert_eq!(labels(&map, Action::Submit), vec!["Alt+Enter".to_string()]);
        assert_eq!(
            labels(&map, Action::Newline),
            vec!["Shift+Enter".to_string(), "Ctrl+J".to_string()]
        );
        assert_eq!(labels(&map, Action::Yank), Vec::<String>::new());
        assert_eq!(labels(&map, Action::WordRight), Vec::<String>::new());
        assert_eq!(labels(&map, Action::MoveLeft), vec!["Left".to_string()]);
        assert_eq!(
            labels(&map, Action::DeleteForward),
            vec!["Ctrl+S".to_string()]
        );
        assert_eq!(
            labels(&map, Action::LineStart),
            vec!["Home".to_string(), "Ctrl+A".to_string()]
        );
        assert!(
            notices
                .iter()
                .all(|notice| !notice.to_lowercase().contains("ctrl+j")),
            "{notices:?}"
        );

        let (map, notices) = KeyMap::from_entries(&entries(&[("submit", &["esc"])]));
        assert_eq!(
            notices,
            vec![
                "[keys] interrupt in ~/.talaria/config.toml lost its default key Esc because \
                 submit now uses it; interrupt has no key until you bind one."
                    .to_string()
            ]
        );
        assert_eq!(labels(&map, Action::Submit), vec!["Esc".to_string()]);
        assert_eq!(labels(&map, Action::Interrupt), Vec::<String>::new());
    }
}
