//! Palette keystrokes. Issue #223.

use std::collections::BTreeSet;
use std::time::Duration;

use serde_json::{Value, json};
use talaria_core::commands::{
    CatalogResult, CatalogStatus, CommandCatalog, CompleteResult, LocalCommandTable,
    PaletteContext, PaletteEffect, PaletteInput, PaletteStage, PaletteState, RowKind,
    palette_update as update,
};

fn decode_catalog(value: Value) -> CommandCatalog {
    let result: CatalogResult = serde_json::from_value(value).expect("catalogue");
    CommandCatalog::from_result(&result)
}

fn complete(value: Value) -> CompleteResult {
    serde_json::from_value(value).expect("completion")
}

struct World {
    catalog: Option<CommandCatalog>,
    status: CatalogStatus,
    local: LocalCommandTable,
    refused: BTreeSet<String>,
}

impl World {
    fn bare() -> Self {
        Self {
            catalog: None,
            status: CatalogStatus::NotLoaded,
            local: LocalCommandTable::builtin(),
            refused: BTreeSet::new(),
        }
    }

    fn ready(catalog: CommandCatalog) -> Self {
        Self {
            catalog: Some(catalog),
            status: CatalogStatus::Ready,
            local: LocalCommandTable::builtin(),
            refused: BTreeSet::new(),
        }
    }

    fn apply(&self, state: &mut PaletteState, input: PaletteInput) -> Vec<PaletteEffect> {
        let ctx = PaletteContext {
            catalog: self.catalog.as_ref(),
            catalog_status: &self.status,
            local: &self.local,
            refused: &self.refused,
        };
        update(state, input, &ctx)
    }
}

fn draft(text: &str, caret: usize) -> PaletteInput {
    PaletteInput::DraftChanged {
        text: text.into(),
        caret,
    }
}

fn search_catalog() -> CommandCatalog {
    decode_catalog(json!({
        "pairs": [
            ["/plan-fast", "A faster plan (usage: /plan-fast <task>)"],
            ["/plan", "Plan a task (usage: /plan <task>)"],
            ["/version", "Show the Hermes version"],
            ["/personality", "Pick a personality (usage: /personality <name>)"],
            ["/fixture-user-1", "Fixture user command 1"],
            ["/fixture-skill-1", "Fixture skill 1"]
        ],
        "categories": [
            {"name": "Info", "pairs": [
                ["/plan-fast", "A faster plan (usage: /plan-fast <task>)"],
                ["/plan", "Plan a task (usage: /plan <task>)"],
                ["/version", "Show the Hermes version"],
                ["/personality", "Pick a personality (usage: /personality <name>)"]
            ]},
            {"name": "User commands", "pairs": [["/fixture-user-1", "Fixture user command 1"]]},
            {"name": "Exit", "pairs": [["/quit", "Leave (usage: /quit)"]]}
        ],
        "commands": {
            "/plan-fast": {"argument_mode": "text"},
            "/plan": {"argument_mode": "text"},
            "/version": {},
            "/personality": {"argument_mode": "options"}
        },
        "skills": {
            "/fixture-skill-1": {"usage": 2, "origin": "local"},
            "/fixture-skill-2": {"usage": 0, "origin": "bundled"},
            "/fixture-skill-3": {"usage": 4, "origin": "hub"}
        }
    }))
}

fn names(state: &PaletteState) -> Vec<String> {
    state
        .rows
        .iter()
        .filter(|row| row.kind == RowKind::Command)
        .map(|row| row.name.clone())
        .collect()
}

#[test]
fn opens_only_for_slash_shape() {
    let world = World::bare();
    let mut state = PaletteState::closed();
    let open_cases = [("/", 1, false), ("/he", 3, false), ("/he x", 5, true)];
    for (text, caret, argument) in open_cases {
        let _ = world.apply(&mut state, draft(text, caret));
        assert!(state.open, "{text}");
        assert_eq!(
            matches!(state.stage, PaletteStage::Argument { .. }),
            argument,
            "{text}"
        );
    }
    for (text, caret) in [
        ("hello /x", 9),
        ("/usr/x", 6),
        ("/goal a\nb", 8),
        ("/ ", 2),
        ("/ words", 7),
    ] {
        let _ = world.apply(&mut state, draft(text, caret));
        assert!(!state.open, "{text}");
    }
    let at_mention = "/plan fix @src";
    let _ = world.apply(&mut state, draft(at_mention, at_mention.len()));
    assert!(!state.open);
    let after_fix = "/plan fix".len();
    let _ = world.apply(&mut state, draft(at_mention, after_fix));
    assert!(state.open);
    assert!(matches!(state.stage, PaletteStage::Argument { .. }));
}

#[test]
fn browse_order_groups_and_hides_unused_bundled() {
    let world = World::ready(search_catalog());
    let mut state = PaletteState::closed();
    let _ = world.apply(&mut state, draft("/", 1));
    let headers: Vec<_> = state
        .rows
        .iter()
        .filter(|row| row.kind == RowKind::Header)
        .map(|row| row.name.as_str())
        .collect();
    assert_eq!(headers.first().copied(), Some("Talaria"));
    assert!(!headers.contains(&"Exit"));
    assert!(!names(&state).iter().any(|name| name == "/fixture-skill-2"));
    assert!(names(&state).iter().any(|name| name == "/fixture-skill-3"));
    let quit = state
        .rows
        .iter()
        .find(|row| row.name == "/quit")
        .expect("/quit");
    assert_eq!(
        quit.description,
        "Leave Talaria; the session keeps running in Hermes"
    );

    let loading = World {
        status: CatalogStatus::Loading,
        catalog: None,
        ..World::bare()
    };
    let mut state = PaletteState::closed();
    let _ = loading.apply(&mut state, draft("/", 1));
    assert!(
        state
            .rows
            .iter()
            .any(|row| row.name == "Loading Hermes commands…")
    );
    assert!(names(&state).iter().any(|name| name == "/quit"));
}

#[test]
fn query_filters_locally_then_requests_completion() {
    let world = World::ready(search_catalog());
    let mut state = PaletteState::closed();
    let effects = world.apply(&mut state, draft("/Plan", 5));
    assert_eq!(names(&state).first().map(String::as_str), Some("/plan"));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        PaletteEffect::RequestCompletion { text, after, .. }
            if text == "/Plan" && *after == Duration::from_millis(60)
    )));
    let request_id = state.request_id;
    let _ = world.apply(
        &mut state,
        PaletteInput::CompletionArrived {
            request_id,
            result: complete(json!({
                "items": [
                    {"text": "plan-fast", "display": "/plan-fast", "meta": "A faster plan"},
                    {"text": "plan", "display": "/plan", "meta": "Plan a task"}
                ],
                "replace_from": 1
            })),
        },
    );
    assert_eq!(names(&state).first().map(String::as_str), Some("/plan"));
    let selected = state.rows[state.selected].name.clone();
    let request_id = state.request_id;
    let _ = world.apply(
        &mut state,
        PaletteInput::CompletionArrived {
            request_id,
            result: complete(json!({
                "items": [
                    {"text": "fixture-skill-1", "display": "/fixture-skill-1", "meta": "Fixture skill 1"},
                    {"text": "plan", "display": "/plan", "meta": "Plan a task"}
                ],
                "replace_from": 1
            })),
        },
    );
    assert_eq!(state.rows[state.selected].name, selected);
}

#[test]
fn stale_completion_reply_is_dropped() {
    let world = World::ready(search_catalog());
    let mut state = PaletteState::closed();
    let _ = world.apply(&mut state, draft("/p", 2));
    let first = state.request_id;
    let _ = world.apply(&mut state, draft("/pl", 3));
    let second = state.request_id;
    let before = names(&state);
    let _ = world.apply(
        &mut state,
        PaletteInput::CompletionArrived {
            request_id: first,
            result: complete(json!({
                "items": [{"text": "plan-fast", "display": "/plan-fast", "meta": "later"}],
                "replace_from": 1
            })),
        },
    );
    assert_eq!(names(&state), before);
    assert_ne!(first, second);
}

#[test]
fn completion_failure_keeps_local_rows_and_sets_footer() {
    let world = World::ready(search_catalog());
    let mut state = PaletteState::closed();
    let _ = world.apply(&mut state, draft("/plan", 5));
    let kept = names(&state);
    let request_id = state.request_id;
    let reason = format!("{} and more", "x".repeat(70));
    let _ = world.apply(
        &mut state,
        PaletteInput::CompletionFailed {
            request_id,
            reason: reason.clone(),
        },
    );
    assert_eq!(names(&state), kept);
    let footer = state.footer.expect("footer");
    assert!(footer.starts_with("completion unavailable: "));
    assert_eq!(
        footer,
        format!(
            "completion unavailable: {}",
            reason.chars().take(60).collect::<String>()
        )
    );
}

#[test]
fn next_prev_wrap_and_tab_accepts() {
    let world = World::ready(search_catalog());
    let mut state = PaletteState::closed();
    let _ = world.apply(&mut state, draft("/", 1));
    let first = names(&state)
        .first()
        .map(String::as_str)
        .unwrap_or("")
        .to_owned();
    for _ in 0..names(&state).len() {
        let _ = world.apply(&mut state, PaletteInput::Next);
        assert_ne!(state.rows[state.selected].kind, RowKind::Header);
    }
    assert_eq!(state.rows[state.selected].name, first);
    let _ = world.apply(&mut state, PaletteInput::Prev);
    assert_eq!(
        state.rows[state.selected].name,
        names(&state).last().map(String::as_str).unwrap_or("")
    );

    let _ = world.apply(&mut state, draft("/version", 8));
    let accepted = world.apply(&mut state, PaletteInput::Accept);
    assert!(accepted.iter().any(|effect| matches!(
        effect,
        PaletteEffect::ReplaceDraft { text } if text == "/version"
    )));
    assert!(!state.explicit_selection);

    let _ = world.apply(&mut state, draft("/plan", 5));
    let accepted = world.apply(&mut state, PaletteInput::Accept);
    assert!(accepted.iter().any(|effect| matches!(
        effect,
        PaletteEffect::ReplaceDraft { text } if text == "/plan "
    )));

    let _ = world.apply(&mut state, draft("/", 1));
    let accepted = world.apply(&mut state, PaletteInput::Accept);
    assert!(accepted.iter().any(|effect| matches!(
        effect,
        PaletteEffect::ReplaceDraft { text } if text == "/quit"
    )));

    state.rows.clear();
    let accepted = world.apply(&mut state, PaletteInput::Accept);
    assert!(
        accepted
            .iter()
            .all(|effect| !matches!(effect, PaletteEffect::ReplaceDraft { .. }))
    );

    let _ = world.apply(&mut state, draft("/plan", 5));
    let _ = world.apply(&mut state, PaletteInput::Next);
    let selected = state.rows[state.selected].name.clone();
    let _ = world.apply(&mut state, PaletteInput::CatalogChanged);
    assert_eq!(state.rows[state.selected].name, selected);
}

fn run_line(effects: &[PaletteEffect]) -> Option<&str> {
    effects.iter().find_map(|effect| match effect {
        PaletteEffect::Run { line } => Some(line.as_str()),
        _ => None,
    })
}

fn replaced(effects: &[PaletteEffect]) -> Option<&str> {
    effects.iter().find_map(|effect| match effect {
        PaletteEffect::ReplaceDraft { text } => Some(text.as_str()),
        _ => None,
    })
}

#[test]
fn enter_decision_table() {
    let world = World::ready(search_catalog());
    let mut state = PaletteState::closed();
    let _ = world.apply(&mut state, draft("/", 1));
    let bare = world.apply(&mut state, PaletteInput::Enter);
    assert!(run_line(&bare).is_none());
    assert!(state.open);

    let _ = world.apply(&mut state, PaletteInput::Next);
    let chosen = world.apply(&mut state, PaletteInput::Enter);
    assert!(run_line(&chosen).is_some() || replaced(&chosen).is_some());

    let _ = world.apply(&mut state, draft("/plan", 5));
    let exact = world.apply(&mut state, PaletteInput::Enter);
    assert_eq!(run_line(&exact), Some("/plan"));

    let _ = world.apply(&mut state, draft("/vers", 5));
    let prefix = world.apply(&mut state, PaletteInput::Enter);
    assert_eq!(replaced(&prefix), Some("/version"));
    assert_eq!(run_line(&prefix), Some("/version"));

    let _ = world.apply(&mut state, draft("/pla", 4));
    let argument = world.apply(&mut state, PaletteInput::Enter);
    assert_eq!(replaced(&argument), Some("/plan "));
    assert!(run_line(&argument).is_none());

    let _ = world.apply(&mut state, draft("/fixture-user", 13));
    let user = world.apply(&mut state, PaletteInput::Enter);
    assert_eq!(replaced(&user), Some("/fixture-user-1"));
    assert_eq!(run_line(&user), Some("/fixture-user-1"));

    let _ = world.apply(&mut state, draft("/ers", 4));
    let substring = world.apply(&mut state, PaletteInput::Enter);
    assert_eq!(run_line(&substring), Some("/ers"));
    let _ = world.apply(&mut state, draft("/ers", 4));
    let _ = world.apply(&mut state, PaletteInput::Next);
    let moved = world.apply(&mut state, PaletteInput::Enter);
    assert_eq!(replaced(&moved), Some("/version"));
    assert_eq!(run_line(&moved), Some("/version"));

    let _ = world.apply(&mut state, draft("/plan", 5));
    let pending = state.request_id;
    let _ = world.apply(&mut state, PaletteInput::Enter);
    let before = names(&state);
    let _ = world.apply(
        &mut state,
        PaletteInput::CompletionArrived {
            request_id: pending,
            result: complete(json!({"items": [{"text": "plan-fast", "display": "/plan-fast"}]})),
        },
    );
    assert_eq!(names(&state), before);

    let _ = world.apply(
        &mut state,
        draft("/personality al", "/personality al".len()),
    );
    let _ = world.apply(&mut state, PaletteInput::Next);
    let argued = world.apply(&mut state, PaletteInput::Enter);
    assert!(replaced(&argued).is_some());
    assert!(run_line(&argued).is_some());
    let _ = world.apply(&mut state, draft("/plan write", "/plan write".len()));
    state.explicit_selection = false;
    let typed = world.apply(&mut state, PaletteInput::Enter);
    assert_eq!(run_line(&typed), Some("/plan write"));
}

#[test]
fn argument_stage_uses_replace_from_in_code_points() {
    let world = World::ready(search_catalog());
    let mut state = PaletteState::closed();
    let draft_text = "/personality al";
    let _ = world.apply(&mut state, draft(draft_text, draft_text.len()));
    assert!(state.rows.iter().any(|row| row.kind == RowKind::Hint));
    let request_id = state.request_id;
    let _ = world.apply(
        &mut state,
        PaletteInput::CompletionArrived {
            request_id,
            result: complete(json!({
                "items": [{"text": "alice", "display": "alice", "meta": "a friendly assistant"}],
                "replace_from": 13
            })),
        },
    );
    let accepted = world.apply(&mut state, PaletteInput::Accept);
    assert_eq!(replaced(&accepted), Some("/personality alice"));

    let naive = "/naïve al";
    let bytes: usize = naive.chars().take(7).map(char::len_utf8).sum();
    let _ = world.apply(&mut state, draft(naive, naive.len()));
    let request_id = state.request_id;
    let _ = world.apply(
        &mut state,
        PaletteInput::CompletionArrived {
            request_id,
            result: complete(json!({
                "items": [{"text": "alice", "display": "alice", "meta": ""}],
                "replace_from": 7
            })),
        },
    );
    let accepted = world.apply(&mut state, PaletteInput::Accept);
    let expected = format!("{}alice", &naive[..bytes]);
    assert_eq!(replaced(&accepted), Some(expected.as_str()));

    let _ = world.apply(&mut state, draft("/plan x", 7));
    let request_id = state.request_id;
    let _ = world.apply(
        &mut state,
        PaletteInput::CompletionArrived {
            request_id,
            result: complete(json!({
                "items": [{"text": "nope", "display": "nope"}],
                "replace_from": 80
            })),
        },
    );
    assert!(state.rows.iter().any(|row| row.kind == RowKind::Hint));

    let _ = world.apply(&mut state, draft("/plan write a haiku", 19));
    let request_id = state.request_id;
    let _ = world.apply(
        &mut state,
        PaletteInput::CompletionArrived {
            request_id,
            result: complete(json!({"items": []})),
        },
    );
    assert!(state.rows.iter().any(|row| row.kind == RowKind::Hint));
    assert!(names(&state).is_empty());

    let _ = world.apply(&mut state, draft("/zzz extra", 10));
    let request_id = state.request_id;
    let _ = world.apply(
        &mut state,
        PaletteInput::CompletionArrived {
            request_id,
            result: complete(json!({"items": []})),
        },
    );
    assert!(state.rows.is_empty());
    assert!(matches!(state.stage, PaletteStage::Argument { .. }));

    let request_id = state.request_id;
    let prose = world.apply(
        &mut state,
        PaletteInput::CompletionArrived {
            request_id,
            result: complete(json!({"items": [{"text": "hello", "display": "hello"}]})),
        },
    );
    assert!(names(&state).is_empty() || prose.is_empty());
}

#[test]
fn esc_closes_until_draft_changes() {
    let world = World::ready(search_catalog());
    let mut state = PaletteState::closed();
    let _ = world.apply(&mut state, draft("/he", 3));
    let _ = world.apply(&mut state, PaletteInput::Close);
    assert!(!state.open);
    assert_eq!(state.closed_for.as_deref(), Some("/he"));
    let _ = world.apply(&mut state, draft("/he", 3));
    assert!(!state.open);
    let _ = world.apply(&mut state, draft("/hex", 4));
    assert!(state.open);
}
