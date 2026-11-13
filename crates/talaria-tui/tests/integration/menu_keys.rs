use talaria_tui::workspace::menu::{MenuKey, MenuState};

#[test]
fn navigation_keys() {
    let mut menu = MenuState::shown();
    menu.draft = "keep".into();
    menu.selection = 1..4;
    assert_eq!(menu.menu_index, 0);
    assert!(!menu.dropdown_open);

    let right = menu.on_key(MenuKey::Right);
    assert!(right.consumed, "the focused menu takes Right");
    assert_eq!(menu.menu_index, 1, "Right moves to the next menu");
    assert!(!menu.dropdown_open);

    menu.on_key(MenuKey::Left);
    assert_eq!(menu.menu_index, 0, "Left moves back");

    menu.on_key(MenuKey::Tab);
    assert_eq!(menu.menu_index, 1, "Tab moves to the next menu");

    menu.on_key(MenuKey::BackTab);
    assert_eq!(menu.menu_index, 0, "Shift+Tab moves to the previous menu");

    menu.on_key(MenuKey::Down);
    assert!(menu.dropdown_open, "Down opens the dropdown");
    assert_eq!(menu.item_index, 0);

    menu.on_key(MenuKey::Down);
    assert_eq!(menu.item_index, 1, "Down selects the next item");

    menu.on_key(MenuKey::Up);
    assert_eq!(menu.item_index, 0, "Up selects the previous item");

    menu.dropdown_open = false;
    menu.on_key(MenuKey::Enter);
    assert!(menu.dropdown_open, "Enter opens the dropdown");
    assert_eq!(menu.item_index, 0);

    let activated = menu.on_key(MenuKey::Enter);
    assert!(
        activated.activated.is_some(),
        "Enter activates the selected item"
    );
    assert!(!menu.dropdown_open, "activation closes the dropdown");
    assert_eq!(menu.draft, "keep");
    assert_eq!(menu.selection, 1..4);
}

#[test]
fn esc_closes_dropdown_then_returns_focus() {
    let mut menu = MenuState::shown();
    menu.dropdown_open = true;
    menu.draft = "keep".into();
    menu.selection = 1..3;
    menu.on_key(MenuKey::Esc);
    assert!(!menu.dropdown_open, "Esc closes the dropdown first");
    assert!(
        menu.focused,
        "the menu keeps focus after the dropdown closes"
    );
    assert!(menu.visible);
    assert_eq!(menu.draft, "keep");
    assert_eq!(menu.selection, 1..3);
    menu.on_key(MenuKey::Esc);
    assert!(!menu.focused, "a second Esc returns focus to the composer");
    assert!(
        menu.visible,
        "the menu row stays while its preference is on"
    );
    assert_eq!(menu.draft, "keep");

    let mut transient = MenuState::shown();
    transient.transient = true;
    transient.dropdown_open = true;
    transient.on_key(MenuKey::Esc);
    assert!(!transient.dropdown_open);
    assert!(transient.focused);
    transient.on_key(MenuKey::Esc);
    assert!(!transient.visible, "a transient menu closes on Esc");
    assert!(!transient.focused);
}

#[test]
fn plain_letters_reach_the_menu_only_while_focused() {
    let mut focused = MenuState::shown();
    focused.draft = "keep".into();
    focused.selection = 2..5;
    let taken = focused.on_key(MenuKey::Letter('s'));
    assert!(taken.consumed, "plain letters reach a focused menu");
    assert_eq!(focused.draft, "keep");
    assert_eq!(focused.selection, 2..5);

    let mut idle = MenuState::hidden();
    idle.draft = "keep".into();
    idle.selection = 2..5;
    let ignored = idle.on_key(MenuKey::Letter('s'));
    assert!(
        !ignored.consumed,
        "plain letters miss the menu until it has focus"
    );
    assert_eq!(idle.draft, "keep");
    assert_eq!(idle.selection, 2..5);
}
