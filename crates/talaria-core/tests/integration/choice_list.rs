//! The inline selector behind `/theme` and `/skin`: a list with descriptions, the current entry
//! marked, typing to filter, Up and Down to preview, Enter to apply and Esc to cancel.

use talaria_core::commands::{ChoiceEffect, ChoiceItem, ChoiceKey, ChoiceList, RowKind};

fn item(id: &str, name: &str, description: &str, group: &str, current: bool) -> ChoiceItem {
    ChoiceItem {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        group: Some(group.into()),
        current,
    }
}

fn skins() -> ChoiceList {
    ChoiceList::new(vec![
        item("harbor", "harbor", "Cool blue and grey", "Skins", false),
        item("meadow", "meadow", "Soft greens", "Skins", true),
        item("cinder", "cinder", "Warm reds on charcoal", "Skins", false),
    ])
}

fn shown(list: &ChoiceList) -> Vec<(RowKind, String, String)> {
    list.rows()
        .into_iter()
        .map(|row| (row.kind, row.name, row.description))
        .collect()
}

#[test]
fn current_entry_is_marked_and_selected_at_open() {
    let list = skins();
    assert_eq!(list.selected().map(|item| item.id.as_str()), Some("meadow"));
    assert_eq!(
        shown(&list),
        [
            (RowKind::Header, "Skins".to_owned(), String::new()),
            (
                RowKind::Command,
                "harbor".to_owned(),
                "Cool blue and grey".to_owned()
            ),
            (
                RowKind::Command,
                "meadow (current)".to_owned(),
                "Soft greens".to_owned()
            ),
            (
                RowKind::Command,
                "cinder".to_owned(),
                "Warm reds on charcoal".to_owned()
            ),
        ]
    );
    assert_eq!(list.selected_row(), 2);
}

#[test]
fn moving_selection_emits_preview_for_each_entry() {
    let mut list = skins();
    assert_eq!(
        list.update(ChoiceKey::Next),
        ChoiceEffect::Preview("cinder".into())
    );
    // The list wraps at either end, as the slash palette does.
    assert_eq!(
        list.update(ChoiceKey::Next),
        ChoiceEffect::Preview("harbor".into())
    );
    assert_eq!(
        list.update(ChoiceKey::Prev),
        ChoiceEffect::Preview("cinder".into())
    );
    assert_eq!(list.selected_row(), 3);
}

#[test]
fn escape_cancels_and_enter_applies_the_selected_entry() {
    let mut list = skins();
    list.update(ChoiceKey::Prev);
    assert_eq!(
        list.update(ChoiceKey::Apply),
        ChoiceEffect::Apply("harbor".into())
    );
    let mut list = skins();
    list.update(ChoiceKey::Next);
    assert_eq!(list.update(ChoiceKey::Cancel), ChoiceEffect::Cancel);
}

#[test]
fn typing_filters_by_name_and_description() {
    let mut list = skins();
    // "w" still matches meadow, the selection, so nothing is previewed; "wa" matches only
    // cinder's description, so the selection moves there and previews it once.
    let effects: Vec<ChoiceEffect> = "WARM"
        .chars()
        .map(|ch| list.update(ChoiceKey::Type(ch)))
        .collect();
    assert_eq!(
        effects,
        [
            ChoiceEffect::Redraw,
            ChoiceEffect::Preview("cinder".into()),
            ChoiceEffect::Redraw,
            ChoiceEffect::Redraw,
        ]
    );
    assert_eq!(list.filter(), "WARM");
    assert_eq!(
        shown(&list),
        [
            (RowKind::Header, "Skins".to_owned(), String::new()),
            (
                RowKind::Command,
                "cinder".to_owned(),
                "Warm reds on charcoal".to_owned()
            ),
        ]
    );
    // Erasing the filter keeps the selection and shows every row again.
    for _ in 0..4 {
        assert_eq!(list.update(ChoiceKey::Erase), ChoiceEffect::Redraw);
    }
    assert_eq!(list.update(ChoiceKey::Erase), ChoiceEffect::Redraw);
    assert_eq!(list.rows().len(), 4);
    assert_eq!(list.selected().map(|item| item.id.as_str()), Some("cinder"));
}

#[test]
fn no_match_shows_status_row_and_enter_does_nothing() {
    let mut list = skins();
    for ch in "zzz".chars() {
        list.update(ChoiceKey::Type(ch));
    }
    assert_eq!(
        shown(&list),
        [(
            RowKind::Status,
            "No match for \"zzz\"".to_owned(),
            String::new()
        )]
    );
    assert_eq!(list.selected(), None);
    assert_eq!(list.update(ChoiceKey::Apply), ChoiceEffect::Redraw);
    assert_eq!(list.update(ChoiceKey::Next), ChoiceEffect::Redraw);
}

#[test]
fn group_headers_are_skipped_by_the_selection() {
    let mut list = ChoiceList::new(vec![
        item("file:", "Configured", "Follow config.toml", "Theme", true),
        item("file:ember", "ember", "Warm dark", "Theme", false),
        item(
            "background:light",
            "light",
            "Light background",
            "Background",
            false,
        ),
        item("background:auto", "auto", "Detect", "Background", true),
    ]);
    assert_eq!(list.selected_row(), 1);
    list.update(ChoiceKey::Next);
    assert_eq!(
        list.update(ChoiceKey::Next),
        ChoiceEffect::Preview("background:light".into())
    );
    // Row 3 is the Background header; the selection lands on the row after it.
    assert_eq!(list.selected_row(), 4);
    let rows = list.rows();
    assert_eq!(rows[3].kind, RowKind::Header);
    assert_eq!(rows[3].name, "Background");
    assert_eq!(rows[5].name, "auto (current)");
}

#[test]
fn control_characters_never_enter_the_filter() {
    let mut list = skins();
    assert_eq!(list.update(ChoiceKey::Type('\u{1b}')), ChoiceEffect::Redraw);
    assert_eq!(list.filter(), "");
}
