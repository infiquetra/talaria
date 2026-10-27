//! Local commands win over a Hermes command of the same name. Issue #223.

use std::collections::BTreeSet;

use serde_json::json;
use talaria_core::commands::{
    CatalogResult, CatalogStatus, CommandCatalog, CommandSource, LocalAction, LocalCommandTable,
    PaletteContext, PaletteEffect, PaletteInput, PaletteState, palette_update as update,
};

fn catalog_with_quit() -> CommandCatalog {
    let result: CatalogResult = serde_json::from_value(json!({
        "pairs": [["/quit", "Leave (usage: /quit)"]],
        "categories": [{"name": "Exit", "pairs": [["/quit", "Leave (usage: /quit)"]]}],
        "commands": {"/quit": {}}
    }))
    .expect("catalogue");
    CommandCatalog::from_result(&result)
}

fn drive(text: &str, then: &[PaletteInput]) -> (PaletteState, Vec<PaletteEffect>) {
    let catalog = catalog_with_quit();
    let local = LocalCommandTable::builtin();
    let status = CatalogStatus::Ready;
    let refused = BTreeSet::new();
    let ctx = PaletteContext {
        catalog: Some(&catalog),
        catalog_status: &status,
        local: &local,
        refused: &refused,
    };
    let mut state = PaletteState::closed();
    let mut effects = update(
        &mut state,
        PaletteInput::DraftChanged {
            text: text.into(),
            caret: text.len(),
        },
        &ctx,
    );
    for input in then {
        effects.extend(update(&mut state, input.clone(), &ctx));
    }
    (state, effects)
}

#[test]
fn local_command_shadows_backend() {
    let table = LocalCommandTable::builtin();
    let quit = table.find("quit").expect("/quit");
    assert_eq!(quit.action, LocalAction::Quit);
    assert_eq!(
        quit.summary,
        "Leave Talaria; the session keeps running in Hermes"
    );
    assert!(!quit.takes_argument);
    assert!(quit.aliases.contains(&"exit"));

    let (state, _) = drive("/", &[]);
    let quit_rows: Vec<_> = state
        .rows
        .iter()
        .filter(|row| row.name == "/quit" || row.name == "quit")
        .collect();
    assert_eq!(quit_rows.len(), 1);
    assert_eq!(quit_rows[0].source, Some(CommandSource::Talaria));

    let (_, enter) = drive("/quit", &[PaletteInput::Enter]);
    assert!(enter.iter().any(|effect| matches!(
        effect,
        PaletteEffect::Run { line } if line == "/quit"
    )));
    assert!(
        enter
            .iter()
            .all(|effect| !matches!(effect, PaletteEffect::RequestCompletion { .. }))
    );

    let (_, now) = drive("/quit now", &[]);
    assert!(
        now.iter()
            .all(|effect| !matches!(effect, PaletteEffect::Run { .. })),
        "/quit now shows `/quit does not take an argument.` and runs nothing"
    );
}

#[test]
fn local_table_names_are_unique() {
    let table = LocalCommandTable::builtin();
    let mut seen = BTreeSet::new();
    for command in table.iter() {
        assert!(
            command
                .name
                .chars()
                .all(|ch| ch.is_lowercase() || ch == '-')
        );
        assert!(!command.name.contains('/'));
        assert!(seen.insert(command.name), "repeated name {}", command.name);
        for alias in command.aliases {
            assert!(alias.chars().all(|ch| ch.is_lowercase() || ch == '-'));
            assert!(!alias.contains('/'));
            assert!(seen.insert(alias), "repeated alias {alias}");
        }
    }
}
