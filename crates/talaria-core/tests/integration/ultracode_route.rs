use proptest::prelude::*;
use talaria_core::{
    session::ids::SessionId,
    ultracode::{
        SubmitRoute, UltracodeCommand, UltracodeState, contains_keyword, route_submit,
        route_submit_with_model,
    },
};

#[test]
fn route_table() {
    let plain = SubmitRoute::Plain;
    let workflow = |text: &str, one_shot| SubmitRoute::Workflow {
        command_line: format!("/talaria-workflows {text}"),
        one_shot,
    };
    let cases = [
        ("", true, plain.clone()),
        ("   \n", true, plain.clone()),
        ("/help", true, plain.clone()),
        ("  /plan x", true, plain.clone()),
        ("!ls", true, plain.clone()),
        ("fix the parser", true, workflow("fix the parser", false)),
        (
            "fix the parser  \n",
            true,
            workflow("fix the parser", false),
        ),
        ("fix the parser", false, plain.clone()),
        (
            "ultracode: fix it",
            false,
            workflow("ultracode: fix it", true),
        ),
        (
            "please ULTRACODE.",
            false,
            workflow("please ULTRACODE.", true),
        ),
        ("ultracoder", false, plain.clone()),
        ("pre-ultracode", false, plain.clone()),
        ("ultracode_x", false, plain.clone()),
        ("`ultracode`", false, workflow("`ultracode`", true)),
        (
            "line one\nultracode\nline three",
            false,
            workflow("line one\nultracode\nline three", true),
        ),
        ("ultracode", true, workflow("ultracode", false)),
    ];
    for (text, enabled, expected) in cases {
        assert_eq!(
            route_submit(text, enabled),
            expected,
            "{text:?}, enabled={enabled}"
        );
    }
    assert!(contains_keyword("éultracode"));
}
#[test]
fn route_submit_includes_session_model() {
    let route = route_submit_with_model("fix the parser", true, Some(("model-a", "provider-a")));
    assert_eq!(
        route,
        SubmitRoute::Workflow {
            command_line: "/talaria-workflows fix the parser\nSession model: model-a on provider-a"
                .into(),
            one_shot: false,
        }
    );
    // Nothing is added when the session's pair is unknown or not plain text.
    for pair in [
        None,
        Some(("", "provider-a")),
        Some(("model-a", "")),
        Some(("a\nb", "p")),
    ] {
        assert_eq!(
            route_submit_with_model("fix the parser", true, pair),
            route_submit("fix the parser", true),
            "{pair:?}"
        );
    }
    // A prompt that does not take the workflow route stays plain.
    assert_eq!(
        route_submit_with_model("/help", true, Some(("model-a", "provider-a"))),
        SubmitRoute::Plain
    );
}
proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, .. ProptestConfig::default() })]
    #[test]
    fn keyword_property(text in "[A-Za-z0-9_.,: \\n-]{0,120}", position in 0usize..120, uppercase in any::<bool>()) {
        let split = position.min(text.len());
        let keyword = if uppercase { "ULTRACODE" } else { "ultracode" };
        let candidate = format!("{}{}{}", &text[..split], keyword, &text[split..]);
        let oracle = candidate.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')).any(|word| word.eq_ignore_ascii_case("ultracode"));
        prop_assert_eq!(contains_keyword(&candidate), oracle);
    }
}
#[test]
fn state_and_command_parsing() {
    let a = SessionId::new("fixture-a");
    let b = SessionId::new("fixture-b");
    let mut state = UltracodeState::new(false);
    assert!(!state.is_on(&a));
    assert!(!state.is_on(&b));
    state.set(&a, true);
    assert!(state.is_on(&a));
    assert!(!state.is_on(&b));
    assert!(!state.toggle(&a));
    assert!(state.toggle(&b));
    for (input, expected) in [
        ("", UltracodeCommand::Toggle),
        ("on", UltracodeCommand::On),
        (" On ", UltracodeCommand::On),
        ("off", UltracodeCommand::Off),
        ("status", UltracodeCommand::Status),
    ] {
        assert_eq!(UltracodeCommand::parse(input).unwrap(), expected);
    }
    assert!(UltracodeCommand::parse("maybe").is_err());
    assert!(UltracodeCommand::parse("on now").is_err());
    let default_on = UltracodeState::new(true);
    assert!(default_on.is_on(&a));
}

#[test]
fn bypass_flags_prompt_commands_only_when_on() {
    use talaria_core::ultracode::bypass;
    for (text, expected) in [
        ("/goal build a fixture tool", Some("goal")),
        ("  /goal   build x", Some("goal")),
        ("/GOAL build x", Some("GOAL")),
        ("/q fix the parser", Some("q")),
        ("/queue fix the parser", Some("queue")),
        ("/steer look at the tests", Some("steer")),
        ("/s look at the tests", Some("s")),
        ("/bg write the docs", Some("bg")),
        ("/background write the docs", Some("background")),
        ("/btw what is a fixture", Some("btw")),
        ("/plan the refactor", Some("plan")),
        ("/goal", None),
        ("/goal status", None),
        ("/goal pause", None),
        ("/goal clear", None),
        ("/goal show", None),
        ("/queue", None),
        ("/help", None),
        ("/ultracode on", None),
        ("!ls", None),
        ("fix the parser", None),
        ("", None),
    ] {
        assert_eq!(bypass(text, true).as_deref(), expected, "{text:?}");
        assert_eq!(bypass(text, false), None, "{text:?} with Ultracode off");
    }
}

#[test]
fn carry_copies_explicit_state_to_child() {
    let parent = SessionId::new("fixture-parent");
    let child = SessionId::new("fixture-child");
    let mut state = UltracodeState::new(false);
    assert!(!state.carry(&parent, &child), "nothing to carry");
    assert!(!state.is_on(&child));
    state.set(&parent, true);
    assert!(state.carry(&parent, &child));
    assert!(state.is_on(&child));
    state.set(&parent, false);
    let mut default_on = UltracodeState::new(true);
    default_on.set(&parent, false);
    assert!(!default_on.carry(&parent, &child));
    assert!(!default_on.is_on(&child), "an explicit off is carried too");
}

#[test]
fn the_full_skill_goes_once_per_conversation_until_it_changes() {
    use talaria_core::ultracode::SkillEpoch;
    let a = SessionId::new("fixture-a");
    let reconnected = SessionId::new("fixture-a2");
    let epoch = |stored: &str, compressions| SkillEpoch {
        stored: stored.into(),
        compressions,
    };
    let mut state = UltracodeState::new(true);
    assert!(
        !state.skill_loaded(&a),
        "the first routed prompt carries it"
    );
    state.skill_sent(&a, epoch("stored-a", 0));
    state.note_epoch(&a, &epoch("stored-a", 0));
    assert!(state.skill_loaded(&a), "later prompts get the reminder");
    state.carry(&a, &reconnected);
    state.note_epoch(&reconnected, &epoch("stored-a", 0));
    assert!(
        state.skill_loaded(&reconnected),
        "a reconnect keeps the conversation"
    );
    state.note_epoch(&a, &epoch("stored-a", 1));
    assert!(!state.skill_loaded(&a), "a compaction may have dropped it");
    state.skill_sent(&a, epoch("stored-a", 1));
    state.note_epoch(&a, &epoch("stored-other", 1));
    assert!(
        !state.skill_loaded(&a),
        "a new stored id is a new conversation"
    );
    state.skill_sent(&a, epoch("stored-other", 1));
    state.rearm(&a);
    assert!(
        !state.skill_loaded(&a),
        "typing the command again re-arms it"
    );
}

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// The saved file's text after `changes` are merged into `file`.
fn merged(
    mut file: talaria_core::ultracode::SavedModes,
    changes: &talaria_core::ultracode::SavedChanges,
) -> String {
    file.merge(changes);
    file.to_json()
}

#[test]
fn the_switch_follows_the_stored_session_across_runtime_ids() {
    use talaria_core::ultracode::SavedModes;
    let (a1, w1, b2) = (
        SessionId::new("runtime-a1"),
        SessionId::new("runtime-w1"),
        SessionId::new("runtime-b2"),
    );
    let mut state = UltracodeState::new(false);
    assert!(!state.adopt(&a1, "stored-s"));
    state.set(&a1, true);
    // Another session in between keeps its own setting.
    assert!(!state.adopt(&w1, "stored-w"));
    // The same stored session under a new runtime id is still on.
    assert!(state.adopt(&b2, "stored-s"));
    assert!(state.is_on(&b2));

    // A restart reads the saved file and restores it under yet another runtime id.
    let saved = state.take_saved(10 * DAY_MS).expect("a change is saved");
    let text = merged(SavedModes::default(), &saved);
    let mut restarted = UltracodeState::new(false);
    restarted.restore(SavedModes::from_json(&text).expect("the saved file reads back"));
    let c3 = SessionId::new("runtime-c3");
    assert!(restarted.adopt(&c3, "stored-s"));
    assert!(!restarted.adopt(&SessionId::new("runtime-d4"), "stored-w"));
    assert!(
        restarted.take_saved(10 * DAY_MS).is_some(),
        "an open refreshes when it was seen"
    );
    assert!(
        restarted.take_saved(10 * DAY_MS).is_none(),
        "nothing changed since"
    );

    // Off is remembered too, even when the launch default is on.
    let mut defaulted = UltracodeState::new(true);
    assert!(defaulted.adopt(&a1, "stored-s"));
    defaulted.set(&a1, false);
    let text = merged(
        SavedModes::default(),
        &defaulted.take_saved(DAY_MS).unwrap(),
    );
    let mut again = UltracodeState::new(true);
    again.restore(SavedModes::from_json(&text).unwrap());
    assert!(!again.adopt(&b2, "stored-s"));
}

#[test]
fn a_setting_made_before_the_stored_id_is_known_is_kept() {
    let (a1, child) = (
        SessionId::new("runtime-a1"),
        SessionId::new("runtime-child"),
    );
    let mut state = UltracodeState::new(false);
    state.set(&a1, true);
    // A branch copies the parent's setting before the child is opened.
    assert!(state.carry(&a1, &child));
    assert!(state.adopt(&child, "stored-child"));
    let saved = merged(Default::default(), &state.take_saved(DAY_MS).unwrap());
    assert!(saved.contains("stored-child"), "{saved}");
}

#[test]
fn compaction_moves_the_switch_to_the_new_stored_id() {
    use talaria_core::ultracode::SavedModes;
    let a1 = SessionId::new("runtime-a1");
    let mut state = UltracodeState::new(false);
    state.adopt(&a1, "stored-before");
    state.set(&a1, true);
    let _ = state.take_saved(DAY_MS);
    state.rotate(&a1, "stored-after");
    assert!(state.is_on(&a1));
    let text = merged(
        SavedModes::default(),
        &state.take_saved(DAY_MS).expect("the move is saved"),
    );
    let mut restarted = UltracodeState::new(false);
    restarted.restore(SavedModes::from_json(&text).unwrap());
    assert!(restarted.adopt(&SessionId::new("runtime-b2"), "stored-after"));
    // Rotating to the id it already has changes nothing.
    state.rotate(&a1, "stored-after");
    assert!(state.take_saved(DAY_MS).is_none());
}

#[test]
fn the_saved_file_drops_sessions_not_seen_for_thirty_days() {
    use talaria_core::ultracode::SavedModes;
    let mut state = UltracodeState::new(false);
    let (old, recent) = (SessionId::new("runtime-old"), SessionId::new("runtime-new"));
    state.adopt(&old, "stored-old");
    state.set(&old, true);
    let mut file = SavedModes::default();
    file.merge(&state.take_saved(DAY_MS).unwrap());
    state.adopt(&recent, "stored-new");
    state.set(&recent, true);
    let text = merged(file, &state.take_saved(35 * DAY_MS).unwrap());
    assert!(text.contains("stored-new"), "{text}");
    assert!(!text.contains("stored-old"), "{text}");
    // A file read at start-up is pruned the same way, against the time it is read.
    let mut restarted = UltracodeState::new(false);
    let mut saved = SavedModes::from_json(&text).unwrap();
    saved.prune(80 * DAY_MS);
    restarted.restore(saved);
    assert!(!restarted.adopt(&SessionId::new("runtime-x"), "stored-new"));
    assert!(SavedModes::from_json("not json").is_err());
    assert!(SavedModes::from_json("{\"version\":2,\"sessions\":{}}").is_err());
}

#[test]
fn two_windows_saving_in_turn_keep_each_others_settings() {
    use talaria_core::ultracode::SavedModes;
    // Both windows read the same empty file at start-up.
    let (mut first, mut second) = (UltracodeState::new(false), UltracodeState::new(false));
    let mut file = SavedModes::default();
    let (s, t) = (SessionId::new("runtime-s"), SessionId::new("runtime-t"));
    second.adopt(&s, "stored-s");
    second.set(&s, true);
    file.merge(&second.take_saved(DAY_MS).unwrap());
    first.adopt(&t, "stored-t");
    first.set(&t, true);
    file.merge(&first.take_saved(2 * DAY_MS).unwrap());
    let mut restarted = UltracodeState::new(false);
    restarted.restore(SavedModes::from_json(&file.to_json()).unwrap());
    assert!(restarted.adopt(&SessionId::new("runtime-1"), "stored-s"));
    assert!(restarted.adopt(&SessionId::new("runtime-2"), "stored-t"));

    // A window that only opens a session another window has since changed keeps that change.
    let mut stale = UltracodeState::new(false);
    stale.restore(SavedModes::from_json(&file.to_json()).unwrap());
    second.set(&s, false);
    file.merge(&second.take_saved(3 * DAY_MS).unwrap());
    assert!(stale.adopt(&SessionId::new("runtime-3"), "stored-s"));
    file.merge(&stale.take_saved(4 * DAY_MS).unwrap());
    let mut again = UltracodeState::new(false);
    again.restore(file);
    assert!(!again.adopt(&SessionId::new("runtime-4"), "stored-s"));
}

#[test]
fn a_session_compacted_while_away_keeps_its_setting() {
    let (a1, w1) = (SessionId::new("runtime-a1"), SessionId::new("runtime-w1"));
    let mut state = UltracodeState::new(false);
    state.adopt(&a1, "stored-s");
    state.set(&a1, true);
    state.adopt(&w1, "stored-w");
    // The same runtime session comes back with the compaction's tip as its stored id.
    assert!(state.adopt(&a1, "stored-tip"));
    // A stored id that already has its own setting keeps it.
    let mut other = UltracodeState::new(false);
    other.adopt(&a1, "stored-s");
    other.set(&a1, true);
    other.adopt(&w1, "stored-w");
    other.set(&w1, false);
    assert!(!other.adopt(&a1, "stored-w"));
    // A resume by id answers with the id it was asked for: a stored id saved earlier.
    let mut restarted = UltracodeState::new(false);
    restarted.restore(
        talaria_core::ultracode::SavedModes::from_json(
            r#"{"version":1,"sessions":{"stored-s":{"on":true,"seen_ms":0}}}"#,
        )
        .unwrap(),
    );
    assert!(restarted.adopt(&SessionId::new("stored-s"), "stored-tip"));
    assert!(restarted.take_saved(DAY_MS).is_some(), "the move is saved");
}
