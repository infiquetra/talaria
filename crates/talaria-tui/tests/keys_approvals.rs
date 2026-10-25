//! Test 22 (#237): Shift+Tab toggles approvals and stays rebindable.

use std::collections::BTreeMap;

use crossterm::event::{KeyCode as TermCode, KeyEvent, KeyModifiers};
use talaria_tui::keys::{Action, Key, KeyCode, KeyContext, KeyMap};

fn backtab(modifiers: KeyModifiers) -> Key {
    Key::from(KeyEvent::new(TermCode::BackTab, modifiers))
}

fn key(code: KeyCode, shift: bool, ctrl: bool) -> Key {
    Key::from(KeyEvent::new(
        match code {
            KeyCode::Char(c) => TermCode::Char(c),
            KeyCode::Tab => TermCode::Tab,
            _ => TermCode::Null,
        },
        (if shift {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        }) | (if ctrl {
            KeyModifiers::CONTROL
        } else {
            KeyModifiers::NONE
        }),
    ))
}

#[test]
fn backtab_toggles_and_rebinds() {
    // The default label names the key.
    let map = KeyMap::defaults();
    assert_eq!(
        map.label(Action::ApprovalsToggle),
        Some("Shift+Tab".to_owned())
    );

    // Shift+Tab arrives as BackTab and resolves to the toggle, whatever modifiers
    // the terminal reports with it.
    for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
        let pressed = backtab(modifiers);
        assert_eq!(pressed.code, KeyCode::Tab, "modifiers {modifiers:?}");
        assert!(pressed.mods.shift, "modifiers {modifiers:?}");
        assert_eq!(
            map.resolve(&pressed, KeyContext::Composer),
            Some(Action::ApprovalsToggle)
        );
    }
    // With control held it is a different chord and resolves to nothing.
    let pressed = backtab(KeyModifiers::SHIFT | KeyModifiers::CONTROL);
    assert_eq!(pressed.code, KeyCode::Tab);
    assert_eq!(map.resolve(&pressed, KeyContext::Composer), None);

    // The toggle rebinds like any other action.
    let entries = BTreeMap::from([
        ("approvals_toggle".to_owned(), vec!["ctrl+t".to_owned()]),
        ("toggle_tasks".to_owned(), Vec::new()),
    ]);
    let (map, notices) = KeyMap::from_entries(&entries);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(
        map.label(Action::ApprovalsToggle),
        Some("Ctrl+T".to_owned())
    );
    assert_eq!(
        map.resolve(&key(KeyCode::Char('t'), false, true), KeyContext::Composer),
        Some(Action::ApprovalsToggle)
    );
    assert_eq!(
        map.resolve(&backtab(KeyModifiers::SHIFT), KeyContext::Composer),
        None
    );

    // An empty list unbinds it.
    let entries = BTreeMap::from([("approvals_toggle".to_owned(), Vec::new())]);
    let (map, notices) = KeyMap::from_entries(&entries);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(map.label(Action::ApprovalsToggle), None);
    assert_eq!(
        map.resolve(&backtab(KeyModifiers::SHIFT), KeyContext::Composer),
        None
    );

    // A completion list binds nothing to Shift+Tab.
    let map = KeyMap::defaults();
    let pressed = backtab(KeyModifiers::SHIFT);
    let held = Action::ALL.iter().find(|action| {
        action.context() == KeyContext::Completion
            && map
                .chords(**action)
                .iter()
                .any(|chord| chord.matches(&pressed))
    });
    assert_eq!(held, None);
}
