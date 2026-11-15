//! Local commands win over a Hermes command of the same name. Issue #223.

use std::collections::BTreeSet;

use serde_json::json;
use talaria_core::commands::{
    CatalogResult, CatalogStatus, CommandCatalog, CommandSource, LocalAction, LocalCommandTable,
    PaletteContext, PaletteEffect, PaletteInput, PaletteState, palette_update as update,
    parse_slash_line,
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

#[test]
fn copy_is_local_and_shadows_a_remote_copy() {
    let table = LocalCommandTable::builtin();
    let copy = table.find("copy").expect("/copy");
    assert_eq!(copy.action, LocalAction::Copy);
    assert_eq!(copy.tag, "talaria");
    assert!(copy.takes_argument);

    let parsed = parse_slash_line("/copy 2").expect("slash line");
    assert_eq!(parsed.name, "copy");
    assert_eq!(parsed.arg, "2");
}

#[test]
fn clear_is_local_and_takes_no_argument() {
    let table = LocalCommandTable::builtin();
    let matches: Vec<_> = table.iter().filter(|cmd| cmd.name == "clear").collect();
    assert_eq!(matches.len(), 1);
    let clear = table.find("clear").expect("/clear");
    assert!(clear.aliases.is_empty());
    assert!(!clear.takes_argument);
    assert!(clear.args_hint.is_empty());
    assert_eq!(
        clear.summary,
        "Start a new session in this folder; the previous session keeps running"
    );
    assert_eq!(clear.action, LocalAction::Clear);
}
