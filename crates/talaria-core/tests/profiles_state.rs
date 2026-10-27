//! The profiles state machine. Issue #253.

use talaria_core::profiles::{
    Applied, CallFailure, CreateRequest, CreatedProfile, Description, DiscoveryFailure, FormField,
    FormView, GatewayView, HostTopology, KeyAction, Panel, ProfilesEffect, ProfilesInput,
    ProfilesState, RawEdit, RescanFailure, RescanSummary, RosterEntry, RowTone, Section,
};

fn state() -> ProfilesState {
    ProfilesState::new("default".to_owned())
}

fn roster() -> Vec<RosterEntry> {
    [
        ("default", "minimax-m3.1-flash", "ollama-cloud"),
        ("fixture", "", ""),
        ("fixture-a", "", ""),
        ("fixture-b", "", ""),
        ("fixture-c", "", ""),
        ("fixture-d", "", ""),
    ]
    .iter()
    .map(|(name, model, provider)| RosterEntry {
        name: name.to_string(),
        display_name: name.to_string(),
        is_default: *name == "default",
        model: model.to_string(),
        provider: provider.to_string(),
        description: String::new(),
        skill_count: 0,
    })
    .collect()
}

fn gateway() -> GatewayView {
    GatewayView {
        served: ["default", "fixture", "fixture-a"]
            .iter()
            .map(ToString::to_string)
            .collect(),
        parked: [(
            "fixture-b".to_owned(),
            "session store unusable: <home>/x".to_owned(),
        )]
        .into_iter()
        .collect(),
    }
}

fn topo() -> HostTopology {
    HostTopology {
        stopped: ["fixture-c".to_owned()].into_iter().collect(),
        own_gateway: Some(vec![]),
        gateway_mode: Some("multiplex".to_owned()),
        single_reason: None,
    }
}

fn description(name: &str) -> Description {
    Description {
        name: name.to_owned(),
        description: "Throwaway profile".to_owned(),
        soul: "You are a careful reviewer.\n".to_owned(),
        model_provider: "ollama-cloud".to_owned(),
        model_default: "minimax-m3.1-flash".to_owned(),
        skills: vec![("plan".to_owned(), true)],
        toolsets: vec![("Terminal".to_owned(), true)],
        toolsets_pinned: true,
        mcp_servers: vec![("files".to_owned(), false, "stdio".to_owned())],
    }
}

fn discovery_failure() -> DiscoveryFailure {
    DiscoveryFailure {
        message: "no Hermes gateway is running for this home".to_owned(),
        next_step:
            "Run `hermes gateway ensure --json` to start the Hermes gateway, then try again."
                .to_owned(),
    }
}

/// Drive one full load at generation `gen`, returning every effect.
fn full_load(machine: &mut ProfilesState, generation: u64, now_ms: u64) -> Vec<ProfilesEffect> {
    let mut effects = Vec::new();
    effects.extend(machine.update(ProfilesInput::Discovered(generation, Ok(gateway())), now_ms));
    effects.extend(machine.update(ProfilesInput::Listed(generation, Ok(roster())), now_ms));
    effects.extend(machine.update(ProfilesInput::Topology(generation, Ok(topo())), now_ms));
    effects
}

fn is_fetch(effect: &ProfilesEffect) -> bool {
    matches!(
        effect,
        ProfilesEffect::Discover { .. }
            | ProfilesEffect::List { .. }
            | ProfilesEffect::Topology { .. }
            | ProfilesEffect::Describe { .. }
    )
}

fn type_text(machine: &mut ProfilesState, text: &str, now_ms: u64) {
    for char in text.chars() {
        machine.update(ProfilesInput::Key(KeyAction::Char(char)), now_ms);
    }
}

fn form_of(machine: &ProfilesState) -> FormView {
    match machine.view().panel {
        Panel::Form(form) => form,
        other => panic!("no create form open: {other:?}"),
    }
}

fn review_of(machine: &ProfilesState) -> Vec<String> {
    match machine.view().panel {
        Panel::Review(lines) => lines,
        other => panic!("no review open: {other:?}"),
    }
}

/// Open the form and submit one name with an empty description, fresh start and
/// copied keys. Returns the effects of the Enter that sends.
fn submit_create(machine: &mut ProfilesState, name: &str, now_ms: u64) -> Vec<ProfilesEffect> {
    machine.update(ProfilesInput::Key(KeyAction::New), now_ms);
    type_text(machine, name, now_ms);
    machine.update(ProfilesInput::Key(KeyAction::Enter), now_ms);
    machine.update(ProfilesInput::Key(KeyAction::CtrlEnter), now_ms);
    machine.update(ProfilesInput::Key(KeyAction::Enter), now_ms)
}

#[test]
fn open_fetches_three_sources_and_draws_loading() {
    let mut machine = state();
    let effects = machine.update(ProfilesInput::Open, 0);
    assert_eq!(
        effects,
        vec![
            ProfilesEffect::Discover { generation: 0 },
            ProfilesEffect::List { generation: 0 },
            ProfilesEffect::Topology { generation: 0 },
        ],
        "one refresh, one shared generation"
    );
    let view = machine.view();
    assert!(view.loading, "the first load shows the loading line");
    assert!(view.rows.is_empty(), "no rows before the answers");
    assert_eq!(view.header, "Profiles (- on disk · 0 served)");
    assert_eq!(view.selected, None);
    assert_eq!(view.notice, None);

    // A refresh bumps the generation.
    full_load(&mut machine, 0, 0);
    let effects = machine.update(ProfilesInput::Refresh, 100);
    assert_eq!(
        effects,
        vec![
            ProfilesEffect::Discover { generation: 1 },
            ProfilesEffect::List { generation: 1 },
            ProfilesEffect::Topology { generation: 1 },
        ]
    );
}

#[test]
fn partial_failures_degrade_with_one_notice() {
    let mut machine = state();
    let _ = machine.update(ProfilesInput::Open, 0);
    let _ = machine.update(ProfilesInput::Discovered(0, Ok(gateway())), 0);
    let _ = machine.update(ProfilesInput::Listed(0, Err("no answer".to_owned())), 0);
    let _ = machine.update(
        ProfilesInput::Topology(0, Err("no answer within 8 s".to_owned())),
        0,
    );
    let view = machine.view();
    assert!(!view.loading);
    assert_eq!(
        view.notice.as_deref(),
        Some(
            "Hermes did not return the profile list (no answer). Showing only what the gateway serves."
        ),
        "the roster failure shows first"
    );
    assert_eq!(view.header, "Profiles (- on disk · 3 served)");
    assert_eq!(view.rows.len(), 4, "served and parked rows only");
    assert!(view.rows.iter().all(|row| !row.name_cell.is_empty()));

    // Each key press removes the shown notice, then the next shows.
    let _ = machine.update(ProfilesInput::Key(KeyAction::Down), 10);
    assert_eq!(
        machine.view().notice.as_deref(),
        Some(
            "Some reasons are unavailable: the gateway did not answer /api/status (no answer within 8 s)."
        )
    );
    let _ = machine.update(ProfilesInput::Key(KeyAction::Down), 20);
    assert_eq!(machine.view().notice, None, "each notice shows once");

    // A topology without the gateway list keeps rows 3 and 5 and shows the second form.
    let mut machine = state();
    let _ = machine.update(ProfilesInput::Open, 0);
    let _ = machine.update(ProfilesInput::Discovered(0, Ok(gateway())), 0);
    let _ = machine.update(ProfilesInput::Listed(0, Ok(roster())), 0);
    let gated = HostTopology {
        stopped: ["fixture-c".to_owned()].into_iter().collect(),
        own_gateway: None,
        gateway_mode: Some("multiplex".to_owned()),
        single_reason: None,
    };
    let _ = machine.update(ProfilesInput::Topology(0, Ok(gated)), 0);
    let view = machine.view();
    assert_eq!(
        view.notice.as_deref(),
        Some(
            "Some reasons are unavailable: the gateway did not say which profiles run their own gateway."
        )
    );
    let stopped = view.rows.iter().find(|row| row.name_cell == "fixture-c");
    assert_eq!(stopped.map(|row| row.label.as_str()), Some("stopped"));

    // A discovery failure shows the gateway's own words and unclassifies every row.
    let mut machine = state();
    let _ = machine.update(ProfilesInput::Open, 0);
    full_load(&mut machine, 0, 0);
    let _ = machine.update(ProfilesInput::Refresh, 100);
    let _ = machine.update(ProfilesInput::Discovered(1, Err(discovery_failure())), 100);
    let view = machine.view();
    assert_eq!(
        view.notice.as_deref(),
        Some("no Hermes gateway is running for this home")
    );
    assert_eq!(view.rows.len(), 6, "the last roster stays on screen");
    assert!(view.rows.iter().all(|row| row.label == "unknown"));
    assert!(view.rows.iter().all(|row| row.next_step
        == "Run `hermes gateway ensure --json` to start the Hermes gateway, then try again."));
    // Late roster and topology failures add no second or third notice.
    let _ = machine.update(ProfilesInput::Listed(1, Err("no answer".to_owned())), 100);
    let _ = machine.update(
        ProfilesInput::Topology(1, Err("no answer within 8 s".to_owned())),
        100,
    );
    let _ = machine.update(ProfilesInput::Key(KeyAction::Down), 110);
    assert_eq!(machine.view().notice, None);
}

#[test]
fn describe_after_selection_settles_and_is_cached() {
    let mut machine = state();
    let _ = machine.update(ProfilesInput::Open, 0);

    // Rows draw only when discovery has answered and the other two have settled.
    let _ = machine.update(ProfilesInput::Discovered(0, Ok(gateway())), 0);
    assert!(machine.view().loading);
    assert!(machine.view().rows.is_empty());
    let _ = machine.update(ProfilesInput::Listed(0, Ok(roster())), 0);
    assert!(machine.view().loading);
    let effects = machine.update(ProfilesInput::Topology(0, Ok(topo())), 0);
    let view = machine.view();
    assert!(!view.loading);
    assert_eq!(view.rows.len(), 6);
    assert_eq!(view.header, "Profiles (6 on disk · 3 served)");
    assert_eq!(view.selected, Some(0), "the connected profile is selected");
    assert!(
        effects.contains(&ProfilesEffect::Describe {
            generation: 0,
            name: "default".to_owned()
        }),
        "the selected row is described at once: {effects:?}"
    );

    // A selection move schedules the describe delay; a newer move replaces the token.
    let effects = machine.update(ProfilesInput::Key(KeyAction::Down), 10);
    let first = match effects.as_slice() {
        [
            ProfilesEffect::ScheduleTick {
                after_ms: 150,
                token,
            },
        ] => *token,
        other => panic!("one 150 ms tick, got {other:?}"),
    };
    let effects = machine.update(ProfilesInput::Key(KeyAction::Down), 20);
    let second = match effects.as_slice() {
        [
            ProfilesEffect::ScheduleTick {
                after_ms: 150,
                token,
            },
        ] => *token,
        other => panic!("one 150 ms tick, got {other:?}"),
    };
    assert_ne!(first, second);
    assert_eq!(machine.view().selected, Some(2));
    assert!(
        machine
            .update(ProfilesInput::Tick { token: first }, 170)
            .is_empty(),
        "the stale tick does nothing"
    );
    let effects = machine.update(ProfilesInput::Tick { token: second }, 180);
    assert_eq!(
        effects,
        vec![ProfilesEffect::Describe {
            generation: 0,
            name: "fixture-a".to_owned()
        }]
    );

    // The describe fills the row's model cell.
    let effects = machine.update(
        ProfilesInput::Described(0, "fixture-a".to_owned(), Ok(description("fixture-a"))),
        190,
    );
    assert!(
        !effects
            .iter()
            .any(|effect| effect == &ProfilesEffect::Redraw)
    );
    assert_eq!(
        machine.view().rows[2].model_cell,
        "minimax-m3.1-flash · ollama-cloud"
    );
    assert_eq!(
        machine.view().rows[1].model_cell,
        "-",
        "undescribed stays blank"
    );

    // The detail carries the title, the state text and the formatted lines.
    match machine.view().panel {
        Panel::Detail(detail) => {
            assert_eq!(detail.title, "fixture-a · served");
            assert_eq!(
                detail.state_text,
                "Served by this gateway. Start a session on it with: talaria --profile fixture-a"
            );
            assert_eq!(
                detail.lines,
                [
                    "Model: ollama-cloud · minimax-m3.1-flash",
                    "Skills: 1 of 1 on",
                    "Toolsets: Terminal",
                    "MCP servers: files (off, stdio)",
                    "Description: Throwaway profile",
                    "Soul:",
                    "You are a careful reviewer.",
                ]
            );
        }
        other => panic!("the list carries its detail: {other:?}"),
    }

    // While a describe runs the model reads `reading...`; a failure shows its text.
    let _ = machine.update(ProfilesInput::Key(KeyAction::Up), 200);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Open), 200);
    match machine.view().panel {
        Panel::Detail(detail) => {
            assert_eq!(detail.title, "fixture · served");
            assert_eq!(
                detail.lines.first().map(String::as_str),
                Some("Model: reading...")
            );
        }
        other => panic!("the list carries its detail: {other:?}"),
    }
    let _ = machine.update(
        ProfilesInput::Described(
            0,
            "fixture".to_owned(),
            Err(CallFailure::Refused {
                code: 4064,
                message: "profile 'fixture' not found".to_owned(),
                reason: None,
            }),
        ),
        210,
    );
    match machine.view().panel {
        Panel::Detail(detail) => {
            assert_eq!(
                detail.lines,
                ["Hermes refused: profile 'fixture' not found"]
            );
        }
        other => panic!("the list carries its detail: {other:?}"),
    }

    // Back to the described row before the refresh below.
    let _ = machine.update(ProfilesInput::Key(KeyAction::Down), 220);
    assert_eq!(machine.view().selected, Some(2));

    // The selection follows the name across a refresh that re-sorts the rows.
    let _ = machine.update(ProfilesInput::Refresh, 230);
    let mut parked_gateway = gateway();
    parked_gateway.served.retain(|name| name != "fixture-a");
    parked_gateway
        .parked
        .push(("fixture-a".to_owned(), "reconcile moved it".to_owned()));
    let _ = machine.update(ProfilesInput::Discovered(1, Ok(parked_gateway)), 230);
    let _ = machine.update(ProfilesInput::Listed(1, Ok(roster())), 230);
    let effects = machine.update(ProfilesInput::Topology(1, Ok(topo())), 230);
    let view = machine.view();
    let selected = view.selected.expect("a row stays selected");
    assert_eq!(view.rows[selected].name_cell, "fixture-a");
    assert_ne!(selected, 2, "the row moved but the name stayed selected");
    assert!(
        effects.contains(&ProfilesEffect::Describe {
            generation: 1,
            name: "fixture-a".to_owned()
        }),
        "the refresh dropped the cache and describes again: {effects:?}"
    );

    // A filter narrows the visible rows and selects the first visible row.
    let _ = machine.update(ProfilesInput::Key(KeyAction::Filter), 240);
    for char in "fixture-a".chars() {
        let _ = machine.update(ProfilesInput::Key(KeyAction::Char(char)), 240);
    }
    let view = machine.view();
    assert_eq!(view.filter.as_deref(), Some("fixture-a"));
    assert_eq!(view.rows.len(), 1);
    assert_eq!(view.rows[0].name_cell, "fixture-a");
    assert_eq!(view.selected, Some(0));
    let _ = machine.update(ProfilesInput::Key(KeyAction::Back), 250);
    assert_eq!(machine.view().filter, None, "leaving clears the filter");

    // Cells carry the name, the tone and the short next step.
    let view = machine.view();
    assert_eq!(view.rows[0].name_cell, "default");
    assert_eq!(view.rows[0].tone, RowTone::Muted);
    assert_eq!(view.rows[0].next_step, "talaria --profile default");
    assert_eq!(view.rows[3].tone, RowTone::Warning);
    let mut displayed = state();
    let _ = displayed.update(ProfilesInput::Open, 0);
    let _ = displayed.update(ProfilesInput::Discovered(0, Ok(gateway())), 0);
    let mut shown = roster();
    shown[1].display_name = "Fixture Box".to_owned();
    let _ = displayed.update(ProfilesInput::Listed(0, Ok(shown)), 0);
    let _ = displayed.update(ProfilesInput::Topology(0, Ok(topo())), 0);
    assert_eq!(displayed.view().rows[1].name_cell, "Fixture Box (fixture)");

    // Once everything settled, nothing is scheduled: a stale tick returns no effect.
    assert!(
        machine
            .update(ProfilesInput::Tick { token: 4242 }, 300)
            .is_empty()
    );
}

#[test]
fn stale_generation_replies_are_dropped() {
    let mut machine = state();
    let _ = machine.update(ProfilesInput::Open, 0);
    full_load(&mut machine, 0, 0);
    let _ = machine.update(
        ProfilesInput::Described(0, "default".to_owned(), Ok(description("default"))),
        0,
    );
    let _ = machine.update(ProfilesInput::Refresh, 100);
    let before = machine.view();
    for input in [
        ProfilesInput::Discovered(0, Ok(gateway())),
        ProfilesInput::Listed(0, Ok(roster())),
        ProfilesInput::Topology(0, Ok(topo())),
        ProfilesInput::Described(0, "default".to_owned(), Ok(description("default"))),
    ] {
        assert!(
            machine.update(input, 110).is_empty(),
            "an answer from before the refresh changes nothing"
        );
        assert_eq!(machine.view(), before);
    }
}

#[test]
fn create_form_review_and_exact_effect() {
    let mut machine = state();
    let _ = machine.update(ProfilesInput::Open, 0);
    full_load(&mut machine, 0, 0);

    let _ = machine.update(ProfilesInput::Key(KeyAction::New), 10);
    let form = form_of(&machine);
    assert_eq!(form.focus, FormField::Name);
    assert_eq!(form.name, "");
    assert_eq!(form.name_error, None);
    assert_eq!(
        form.start_from,
        [
            "fresh",
            "default",
            "fixture",
            "fixture-a",
            "fixture-b",
            "fixture-c",
            "fixture-d"
        ]
    );
    assert_eq!(form.start_index, 0);
    assert!(form.copy_keys);
    assert_eq!(form.copy_from, "default");

    // Enter in Name validates; on failure the error shows and focus stays.
    type_text(&mut machine, "Bad Name!", 10);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Enter), 10);
    let form = form_of(&machine);
    assert_eq!(form.focus, FormField::Name);
    assert_eq!(
        form.name_error.as_deref(),
        Some(
            "Use lower-case letters, digits, - or _, starting with a letter or digit, up to 64 characters."
        )
    );
    let _ = machine.update(ProfilesInput::Key(KeyAction::ClearLine), 10);
    type_text(&mut machine, "fixture", 10);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Enter), 10);
    assert_eq!(
        form_of(&machine).name_error.as_deref(),
        Some("A profile named fixture already exists.")
    );

    // Editing keys work in the field.
    let _ = machine.update(ProfilesInput::Key(KeyAction::ClearLine), 10);
    type_text(&mut machine, "ab", 10);
    assert_eq!(form_of(&machine).name_cursor, 2);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Left), 10);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Backspace), 10);
    assert_eq!(form_of(&machine).name, "b");
    let _ = machine.update(ProfilesInput::Key(KeyAction::ClearLine), 10);

    // On success focus moves to Description, which caps at 200 characters.
    type_text(&mut machine, "fixture-new", 10);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Enter), 10);
    assert_eq!(form_of(&machine).focus, FormField::Description);
    assert_eq!(form_of(&machine).name_error, None);
    type_text(&mut machine, &"x".repeat(250), 10);
    assert_eq!(form_of(&machine).description.chars().count(), 200);
    let _ = machine.update(ProfilesInput::Key(KeyAction::ClearLine), 10);
    type_text(&mut machine, "Scratch work", 10);

    // Tab walks the fields; Left and Right wrap around the choices.
    let _ = machine.update(ProfilesInput::Key(KeyAction::Tab), 10);
    assert_eq!(form_of(&machine).focus, FormField::StartFrom);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Left), 10);
    assert_eq!(form_of(&machine).start_index, 6, "Left wraps to the end");
    let _ = machine.update(ProfilesInput::Key(KeyAction::Right), 10);
    assert_eq!(form_of(&machine).start_index, 0, "Right wraps to the start");
    let _ = machine.update(ProfilesInput::Key(KeyAction::Right), 10);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Right), 10);
    assert_eq!(form_of(&machine).start_index, 2);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Tab), 10);
    assert_eq!(form_of(&machine).focus, FormField::CopyKeys);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Space), 10);
    assert!(!form_of(&machine).copy_keys);
    let _ = machine.update(ProfilesInput::Key(KeyAction::BackTab), 10);
    assert_eq!(form_of(&machine).focus, FormField::StartFrom);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Tab), 10);

    // CtrlEnter reviews from any field; the review lists every choice.
    let _ = machine.update(ProfilesInput::Key(KeyAction::CtrlEnter), 10);
    assert_eq!(
        review_of(&machine),
        [
            "name: fixture-new",
            "description: \"Scratch work\"",
            "start from: copy of fixture",
            "copy provider keys from default: off",
            "Hermes also adds a shell command named fixture-new when that name is free.",
        ]
    );
    // Leaving the review returns to the form with every field kept.
    let _ = machine.update(ProfilesInput::Key(KeyAction::Back), 10);
    let form = form_of(&machine);
    assert_eq!(form.name, "fixture-new");
    assert_eq!(form.description, "Scratch work");
    assert_eq!(form.start_index, 2);
    assert!(!form.copy_keys);

    // Enter sends the exact request and closes the form.
    let _ = machine.update(ProfilesInput::Key(KeyAction::CtrlEnter), 10);
    let effects = machine.update(ProfilesInput::Key(KeyAction::Enter), 10);
    assert_eq!(
        effects,
        vec![ProfilesEffect::Create(CreateRequest {
            name: "fixture-new".to_owned(),
            description: Some("Scratch work".to_owned()),
            clone_from: Some("fixture".to_owned()),
            mirror_credentials: false,
        })]
    );
    assert!(matches!(machine.view().panel, Panel::None));

    // One create at a time: a second Enter only warns.
    let _ = machine.update(ProfilesInput::Key(KeyAction::New), 20);
    type_text(&mut machine, "fixture-other", 20);
    let _ = machine.update(ProfilesInput::Key(KeyAction::CtrlEnter), 20);
    let effects = machine.update(ProfilesInput::Key(KeyAction::Enter), 20);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, ProfilesEffect::Create(_)))
    );
    assert_eq!(
        machine.view().notice.as_deref(),
        Some("A create is already in progress.")
    );
    assert!(matches!(machine.view().panel, Panel::Review(_)));

    // Enter in the last field is the dependable path; unset stays unset.
    let mut bare = state();
    let _ = bare.update(ProfilesInput::Open, 0);
    full_load(&mut bare, 0, 0);
    let _ = bare.update(ProfilesInput::Key(KeyAction::New), 10);
    type_text(&mut bare, "fixture-bare", 10);
    let _ = bare.update(ProfilesInput::Key(KeyAction::Enter), 10);
    let _ = bare.update(ProfilesInput::Key(KeyAction::Enter), 10);
    assert_eq!(form_of(&bare).focus, FormField::StartFrom);
    let _ = bare.update(ProfilesInput::Key(KeyAction::Enter), 10);
    assert_eq!(form_of(&bare).focus, FormField::CopyKeys);
    let _ = bare.update(ProfilesInput::Key(KeyAction::Enter), 10);
    assert_eq!(
        review_of(&bare),
        [
            "name: fixture-bare",
            "description: none",
            "start from: fresh",
            "copy provider keys from default: on",
            "Hermes also adds a shell command named fixture-bare when that name is free.",
        ]
    );
    let effects = bare.update(ProfilesInput::Key(KeyAction::Enter), 10);
    assert_eq!(
        effects,
        vec![ProfilesEffect::Create(CreateRequest {
            name: "fixture-bare".to_owned(),
            description: None,
            clone_from: None,
            mirror_credentials: true,
        })]
    );

    // Without a roster the choices fall back to the served names.
    let mut unlisted = state();
    let _ = unlisted.update(ProfilesInput::Open, 0);
    let _ = unlisted.update(ProfilesInput::Discovered(0, Ok(gateway())), 0);
    let _ = unlisted.update(ProfilesInput::Listed(0, Err("no answer".to_owned())), 0);
    let _ = unlisted.update(ProfilesInput::Topology(0, Ok(topo())), 0);
    let _ = unlisted.update(ProfilesInput::Key(KeyAction::New), 10);
    assert_eq!(
        form_of(&unlisted).start_from,
        ["fresh", "default", "fixture", "fixture-a"]
    );
}

#[test]
fn create_outcomes_and_follow_up_discovery() {
    // Served at once: the first follow-up discovery finds the new row served.
    let mut machine = state();
    let _ = machine.update(ProfilesInput::Open, 0);
    full_load(&mut machine, 0, 0);
    let effects = submit_create(&mut machine, "fixture-new", 500);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ProfilesEffect::Create(_)))
    );
    let effects = machine.update(
        ProfilesInput::CreateAnswered(Ok(CreatedProfile {
            name: "fixture-new".to_owned(),
            keys_copied: true,
        })),
        1_000,
    );
    assert_eq!(
        machine.view().notice.as_deref(),
        Some(
            "Created profile fixture-new. Provider keys copied: yes. Waiting for the gateway to serve it..."
        )
    );
    let mut ticks: Vec<(u64, u64)> = effects
        .iter()
        .filter_map(|effect| match effect {
            ProfilesEffect::ScheduleTick { after_ms, token } => Some((*after_ms, *token)),
            _ => None,
        })
        .collect();
    ticks.sort();
    assert_eq!(
        ticks
            .iter()
            .map(|(after_ms, _)| *after_ms)
            .collect::<Vec<_>>(),
        [0, 5_000, 15_000, 35_000, 40_000]
    );
    let tokens: std::collections::BTreeSet<u64> = ticks.iter().map(|(_, token)| *token).collect();
    assert_eq!(tokens.len(), 5, "each tick has its own token");
    assert!(!effects.iter().any(is_fetch));
    let at = |after_ms: u64| ticks.iter().find(|(ms, _)| *ms == after_ms).unwrap().1;

    let effects = machine.update(ProfilesInput::Tick { token: at(0) }, 1_000);
    assert_eq!(
        effects,
        vec![
            ProfilesEffect::Discover { generation: 1 },
            ProfilesEffect::List { generation: 1 },
            ProfilesEffect::Topology { generation: 1 },
        ]
    );
    let mut served_gateway = gateway();
    served_gateway.served.push("fixture-new".to_owned());
    let mut served_roster = roster();
    served_roster.push(RosterEntry {
        name: "fixture-new".to_owned(),
        display_name: "fixture-new".to_owned(),
        is_default: false,
        model: String::new(),
        provider: String::new(),
        description: String::new(),
        skill_count: 0,
    });
    let _ = machine.update(ProfilesInput::Discovered(1, Ok(served_gateway)), 1_100);
    let _ = machine.update(ProfilesInput::Listed(1, Ok(served_roster)), 1_100);
    let _ = machine.update(ProfilesInput::Topology(1, Ok(topo())), 1_100);
    let row = machine
        .view()
        .rows
        .iter()
        .find(|row| row.name_cell == "fixture-new")
        .expect("the new row")
        .clone();
    assert_eq!(row.label, "served");

    // The remaining ticks still run and change nothing once served.
    for (after_ms, generation) in [(5_000, 2), (15_000, 3), (35_000, 4)] {
        let effects = machine.update(
            ProfilesInput::Tick {
                token: at(after_ms),
            },
            1_000 + after_ms,
        );
        assert_eq!(
            effects,
            vec![
                ProfilesEffect::Discover { generation },
                ProfilesEffect::List { generation },
                ProfilesEffect::Topology { generation },
            ]
        );
    }
    assert!(
        machine
            .update(ProfilesInput::Tick { token: at(40_000) }, 41_000)
            .is_empty(),
        "the 40 s tick redraws only when the view changed"
    );

    // Never served: the 40 s tick ends `starting` without a key press.
    let mut slow = state();
    let _ = slow.update(ProfilesInput::Open, 0);
    full_load(&mut slow, 0, 0);
    let _ = submit_create(&mut slow, "fixture-slow", 500);
    let effects = slow.update(
        ProfilesInput::CreateAnswered(Ok(CreatedProfile {
            name: "fixture-slow".to_owned(),
            keys_copied: false,
        })),
        1_000,
    );
    assert_eq!(
        slow.view().notice.as_deref(),
        Some(
            "Created profile fixture-slow. Provider keys copied: no. Waiting for the gateway to serve it..."
        )
    );
    let mut ticks: Vec<(u64, u64)> = effects
        .iter()
        .filter_map(|effect| match effect {
            ProfilesEffect::ScheduleTick { after_ms, token } => Some((*after_ms, *token)),
            _ => None,
        })
        .collect();
    ticks.sort();
    let at = |after_ms: u64| ticks.iter().find(|(ms, _)| *ms == after_ms).unwrap().1;
    for (after_ms, generation) in [(0, 1), (5_000, 2), (15_000, 3), (35_000, 4)] {
        let effects = slow.update(
            ProfilesInput::Tick {
                token: at(after_ms),
            },
            1_000 + after_ms,
        );
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, ProfilesEffect::Discover { .. }))
        );
        let _ = generation;
    }
    let effects = slow.update(ProfilesInput::Tick { token: at(40_000) }, 41_000);
    assert_eq!(effects, vec![ProfilesEffect::Redraw]);
    assert!(
        slow.update(ProfilesInput::Tick { token: at(40_000) }, 41_100)
            .is_empty(),
        "the spent token does nothing"
    );

    // Every failure outcome, in one machine.
    let mut failed = state();
    let _ = failed.update(ProfilesInput::Open, 0);
    full_load(&mut failed, 0, 0);

    let _ = submit_create(&mut failed, "fixture-o", 500);
    let effects = failed.update(
        ProfilesInput::CreateAnswered(Err(CallFailure::Unknown)),
        600,
    );
    assert_eq!(
        failed.view().notice.as_deref(),
        Some(
            "Talaria lost the answer to the create request. Hermes may have created fixture-o; the list below is fresh."
        )
    );
    assert_eq!(
        effects,
        vec![
            ProfilesEffect::Discover { generation: 1 },
            ProfilesEffect::List { generation: 1 },
            ProfilesEffect::Topology { generation: 1 },
        ],
        "a lost answer refreshes once, never retries"
    );

    let _ = submit_create(&mut failed, "fixture-p", 700);
    let _ = failed.update(
        ProfilesInput::CreateAnswered(Err(CallFailure::NotSent)),
        800,
    );
    assert_eq!(
        failed.view().notice.as_deref(),
        Some("Not sent: Talaria is not connected to the gateway. Nothing changed.")
    );
    assert_eq!(
        form_of(&failed).name,
        "fixture-p",
        "the form reopens with its fields"
    );

    let _ = failed.update(ProfilesInput::Key(KeyAction::CtrlEnter), 800);
    let _ = failed.update(ProfilesInput::Key(KeyAction::Enter), 800);
    let _ = failed.update(
        ProfilesInput::CreateAnswered(Err(CallFailure::Refused {
            code: 4062,
            message: "A profile named 'fixture-p' already exists.".to_owned(),
            reason: None,
        })),
        900,
    );
    assert_eq!(
        failed.view().notice.as_deref(),
        Some("Hermes refused: A profile named 'fixture-p' already exists.")
    );
    assert_eq!(form_of(&failed).name, "fixture-p");

    let _ = failed.update(ProfilesInput::Key(KeyAction::CtrlEnter), 900);
    let _ = failed.update(ProfilesInput::Key(KeyAction::Enter), 900);
    let _ = failed.update(
        ProfilesInput::CreateAnswered(Err(CallFailure::Refused {
            code: 4001,
            message: "profile_mismatch".to_owned(),
            reason: Some("profile_mismatch".to_owned()),
        })),
        1_000,
    );
    assert_eq!(
        failed.view().notice.as_deref(),
        Some("Hermes refused: profile_mismatch")
    );

    let _ = failed.update(ProfilesInput::Key(KeyAction::CtrlEnter), 1_000);
    let _ = failed.update(ProfilesInput::Key(KeyAction::Enter), 1_000);
    let _ = failed.update(
        ProfilesInput::CreateAnswered(Err(CallFailure::Refused {
            code: -32601,
            message: "unknown method: profiles.create".to_owned(),
            reason: Some("unknown_method".to_owned()),
        })),
        1_100,
    );
    assert_eq!(
        failed.view().notice.as_deref(),
        Some(
            "This gateway does not let Talaria create profiles over its socket. Use: hermes profile create fixture-p"
        )
    );
    assert!(
        matches!(failed.view().panel, Panel::None),
        "the form closes"
    );

    // A late answer while closed still records the creation and goes to the notice cell.
    let mut late = state();
    let _ = late.update(ProfilesInput::Open, 0);
    full_load(&mut late, 0, 0);
    let _ = submit_create(&mut late, "fixture-late", 500);
    assert_eq!(
        late.update(ProfilesInput::Closed, 600),
        vec![ProfilesEffect::CancelTicks]
    );
    assert_eq!(
        late.update(
            ProfilesInput::CreateAnswered(Ok(CreatedProfile {
                name: "fixture-late".to_owned(),
                keys_copied: true,
            })),
            1_000
        ),
        vec![ProfilesEffect::Notice(
            "Created profile fixture-late. Provider keys copied: yes. Waiting for the gateway to serve it...".to_owned()
        )],
        "no ticks while closed"
    );
    assert_eq!(
        late.update(
            ProfilesInput::ConfigureAnswered(
                "fixture".to_owned(),
                Section::Description,
                Ok(Applied {
                    ok: true,
                    applied: true
                })
            ),
            1_100
        ),
        vec![ProfilesEffect::Notice(
            "Saved the description of fixture.".to_owned()
        )]
    );
    assert_eq!(
        late.update(
            ProfilesInput::RescanAnswered(Ok(RescanSummary::Finished {
                added: 1,
                parked: 0,
                removed: 0
            })),
            1_200
        ),
        vec![ProfilesEffect::Notice(
            "Rescan finished: 1 added, 0 parked, 0 removed.".to_owned()
        )]
    );

    // Reopening inside the window refreshes and re-classifies once, on time.
    let effects = late.update(ProfilesInput::Open, 5_000);
    assert!(
        effects.contains(&ProfilesEffect::ScheduleTick {
            after_ms: 36_000,
            token: effects
                .iter()
                .find_map(|effect| match effect {
                    ProfilesEffect::ScheduleTick { token, .. } => Some(*token),
                    _ => None,
                })
                .expect("one re-classify tick"),
        })
    );
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, ProfilesEffect::ScheduleTick { .. }))
            .count(),
        1
    );
    full_load(&mut late, 1, 5_000);
    let row = late
        .view()
        .rows
        .iter()
        .find(|row| row.name_cell == "fixture-late")
        .expect("the created row")
        .clone();
    assert_eq!(row.label, "starting");

    // Reopening outside the window schedules nothing.
    let _ = late.update(ProfilesInput::Closed, 50_000);
    let effects = late.update(ProfilesInput::Open, 50_000);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, ProfilesEffect::ScheduleTick { .. }))
    );
}

fn input_of(machine: &ProfilesState) -> (String, String, usize, Option<String>) {
    match machine.view().panel {
        Panel::DescriptionInput {
            name,
            text,
            cursor,
            error,
        } => (name, text, cursor, error),
        other => panic!("no description input open: {other:?}"),
    }
}

fn soul_is_described(machine: &mut ProfilesState, soul: &str, now_ms: u64) {
    let effects = machine.update(ProfilesInput::Key(KeyAction::EditSoul), now_ms);
    assert!(
        effects.iter().any(
            |effect| matches!(effect, ProfilesEffect::Describe { name, .. } if name == "default")
        ),
        "the soul is always re-read, never taken from the cache: {effects:?}"
    );
    let generation = describe_gen(&effects, "default");
    let mut described = description("default");
    described.soul = soul.to_owned();
    let effects = machine.update(
        ProfilesInput::Described(generation, "default".to_owned(), Ok(described)),
        now_ms,
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ProfilesEffect::OpenEditor(opened) if opened == soul)),
        "the fresh soul opens in the editor: {effects:?}"
    );
}

fn describe_gen(effects: &[ProfilesEffect], name: &str) -> u64 {
    effects
        .iter()
        .find_map(|effect| match effect {
            ProfilesEffect::Describe {
                generation,
                name: described,
            } if described == name => Some(*generation),
            _ => None,
        })
        .expect("a describe effect")
}

/// Enter on a soul review re-reads, then delivers the re-read soul.
fn reread_soul(machine: &mut ProfilesState, soul: &str, now_ms: u64) -> Vec<ProfilesEffect> {
    let effects = machine.update(ProfilesInput::Key(KeyAction::Enter), now_ms);
    let generation = describe_gen(&effects, "default");
    let mut reread = description("default");
    reread.soul = soul.to_owned();
    machine.update(
        ProfilesInput::Described(generation, "default".to_owned(), Ok(reread)),
        now_ms,
    )
}

#[test]
fn configure_description_and_soul() {
    const MINUS: char = '\u{2212}';
    let mut machine = state();
    let _ = machine.update(ProfilesInput::Open, 0);
    full_load(&mut machine, 0, 0);

    // `e` on an undescribed row describes first, then opens the cleaned prefill.
    let effects = machine.update(ProfilesInput::Key(KeyAction::EditDescription), 10);
    assert_eq!(
        effects,
        vec![ProfilesEffect::Describe {
            generation: 0,
            name: "default".to_owned()
        }]
    );
    assert!(matches!(machine.view().panel, Panel::None));
    let mut described = description("default");
    described.description = "line one\nline two\nline three".to_owned();
    let _ = machine.update(
        ProfilesInput::Described(0, "default".to_owned(), Ok(described)),
        20,
    );
    assert_eq!(
        input_of(&machine),
        (
            "default".to_owned(),
            "line one line two line three".to_owned(),
            28,
            None
        ),
        "each run of line breaks reads as one space, cursor at the end"
    );

    // An untouched multi-line description is never rewritten.
    let effects = machine.update(ProfilesInput::Key(KeyAction::Enter), 30);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, ProfilesEffect::Configure { .. }))
    );
    assert_eq!(machine.view().notice.as_deref(), Some("No change."));
    assert!(matches!(machine.view().panel, Panel::None));

    // `e` on a described row opens at once from the cache.
    let effects = machine.update(ProfilesInput::Key(KeyAction::EditDescription), 40);
    assert!(effects.is_empty(), "no second describe: {effects:?}");
    type_text(&mut machine, "!", 40);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Enter), 40);
    assert_eq!(
        review_of(&machine),
        [r#"description: "line one line two line three" -> "line one line two line three!""#]
    );
    let effects = machine.update(ProfilesInput::Key(KeyAction::Enter), 40);
    assert_eq!(
        effects,
        vec![ProfilesEffect::Configure {
            name: "default".to_owned(),
            section: Section::Description,
            value: "line one line two line three!".to_owned(),
        }]
    );
    let effects = machine.update(
        ProfilesInput::ConfigureAnswered(
            "default".to_owned(),
            Section::Description,
            Ok(Applied {
                ok: true,
                applied: true,
            }),
        ),
        50,
    );
    assert_eq!(
        machine.view().notice.as_deref(),
        Some("Saved the description of default.")
    );
    assert_eq!(effects.len(), 3, "a saved change refreshes: {effects:?}");
    assert!(effects.iter().all(is_fetch));

    // Empty clears.
    let _ = machine.update(ProfilesInput::Key(KeyAction::EditDescription), 60);
    let _ = machine.update(ProfilesInput::Key(KeyAction::ClearLine), 60);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Enter), 60);
    assert_eq!(
        review_of(&machine),
        [r#"description: "line one line two line three" -> """#]
    );
    let effects = machine.update(ProfilesInput::Key(KeyAction::Enter), 60);
    assert!(matches!(
        effects.as_slice(),
        [ProfilesEffect::Configure { value, .. }] if value.is_empty()
    ));

    // More than 200 characters is refused under the input.
    let _ = machine.update(
        ProfilesInput::ConfigureAnswered(
            "default".to_owned(),
            Section::Description,
            Ok(Applied {
                ok: true,
                applied: true,
            }),
        ),
        70,
    );
    let _ = machine.update(ProfilesInput::Key(KeyAction::EditDescription), 70);
    let _ = machine.update(ProfilesInput::Key(KeyAction::ClearLine), 70);
    type_text(&mut machine, &"y".repeat(201), 70);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Enter), 70);
    assert_eq!(
        input_of(&machine).3.as_deref(),
        Some("Use at most 200 characters.")
    );
    let _ = machine.update(ProfilesInput::Key(KeyAction::Back), 70);
    assert!(matches!(machine.view().panel, Panel::None));

    // An equal soul sends nothing.
    soul_is_described(&mut machine, "base\n", 80);
    let effects = machine.update(ProfilesInput::EditorReturned(RawEdit::Unchanged), 90);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, ProfilesEffect::Configure { .. }))
    );
    assert_eq!(machine.view().notice.as_deref(), Some("No change."));

    // A changed soul reviews with the diff count and the first changed lines.
    soul_is_described(&mut machine, "base\n", 100);
    let _ = machine.update(
        ProfilesInput::EditorReturned(RawEdit::Changed("base\nadded\n".to_owned())),
        110,
    );
    assert_eq!(
        review_of(&machine),
        [format!("+1 {MINUS}0 lines"), "+ added".to_owned()]
    );
    // Enter re-reads first; only an unchanged base sends.
    let effects = reread_soul(&mut machine, "base\n", 110);
    assert_eq!(
        effects,
        vec![ProfilesEffect::Configure {
            name: "default".to_owned(),
            section: Section::Soul,
            value: "base\nadded\n".to_owned(),
        }]
    );

    // A changed base keeps the edit and says so; the next `s` reopens it.
    soul_is_described(&mut machine, "base\n", 130);
    let _ = machine.update(
        ProfilesInput::EditorReturned(RawEdit::Changed("mine\n".to_owned())),
        140,
    );
    assert_eq!(
        review_of(&machine),
        [
            format!("+1 {MINUS}1 lines"),
            "- base".to_owned(),
            "+ mine".to_owned()
        ]
    );
    let effects = reread_soul(&mut machine, "other\n", 140);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, ProfilesEffect::Configure { .. }))
    );
    assert_eq!(
        machine.view().notice.as_deref(),
        Some(
            "The soul of default changed since you opened it, so nothing was sent. Your text is kept: press s to reopen it; the review then compares it with the new version."
        )
    );
    let effects = machine.update(ProfilesInput::Key(KeyAction::EditSoul), 160);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ProfilesEffect::Describe { .. }))
    );
    let mut fresh = description("default");
    fresh.soul = "other\n".to_owned();
    let generation = describe_gen(&effects, "default");
    let effects = machine.update(
        ProfilesInput::Described(generation, "default".to_owned(), Ok(fresh)),
        170,
    );
    assert!(
        effects.iter().any(
            |effect| matches!(effect, ProfilesEffect::OpenEditor(opened) if opened == "mine\n")
        ),
        "the kept edit reopens: {effects:?}"
    );
    let _ = machine.update(ProfilesInput::EditorReturned(RawEdit::Unchanged), 180);
    assert_eq!(
        review_of(&machine),
        [
            format!("+1 {MINUS}1 lines"),
            "- other".to_owned(),
            "+ mine".to_owned()
        ],
        "the review compares the kept text with the new base"
    );
    let effects = reread_soul(&mut machine, "other\n", 180);
    assert!(matches!(
        effects.as_slice(),
        [ProfilesEffect::Configure { value, .. }] if value == "mine\n"
    ));
    let _ = machine.update(
        ProfilesInput::ConfigureAnswered(
            "default".to_owned(),
            Section::Soul,
            Ok(Applied {
                ok: true,
                applied: true,
            }),
        ),
        200,
    );
    assert_eq!(
        machine.view().notice.as_deref(),
        Some(format!("Saved the soul of default (+1 {MINUS}1 lines).").as_str())
    );

    // Not saved, lost and not sent.
    soul_is_described(&mut machine, "v2\n", 210);
    let _ = machine.update(
        ProfilesInput::EditorReturned(RawEdit::Changed("v2 edited\n".to_owned())),
        220,
    );
    let effects = reread_soul(&mut machine, "v2\n", 220);
    assert_eq!(
        effects,
        vec![ProfilesEffect::Configure {
            name: "default".to_owned(),
            section: Section::Soul,
            value: "v2 edited\n".to_owned(),
        }]
    );
    let effects = machine.update(
        ProfilesInput::ConfigureAnswered(
            "default".to_owned(),
            Section::Soul,
            Ok(Applied {
                ok: false,
                applied: false,
            }),
        ),
        240,
    );
    assert_eq!(
        machine.view().notice.as_deref(),
        Some("Hermes did not save the soul of default.")
    );
    assert!(!effects.iter().any(is_fetch));
    assert!(
        matches!(machine.view().panel, Panel::Review(_)),
        "the review reopens"
    );

    let effects = reread_soul(&mut machine, "v2\n", 250);
    assert_eq!(
        effects,
        vec![ProfilesEffect::Configure {
            name: "default".to_owned(),
            section: Section::Soul,
            value: "v2 edited\n".to_owned(),
        }]
    );
    let effects = machine.update(
        ProfilesInput::ConfigureAnswered(
            "default".to_owned(),
            Section::Soul,
            Err(CallFailure::Unknown),
        ),
        270,
    );
    assert_eq!(
        machine.view().notice.as_deref(),
        Some(
            "Talaria lost the answer to the change. Hermes may have saved the soul of default; the list below is fresh."
        )
    );
    assert_eq!(effects.len(), 3, "a lost answer refreshes: {effects:?}");

    soul_is_described(&mut machine, "v3\n", 280);
    let _ = machine.update(
        ProfilesInput::EditorReturned(RawEdit::Changed("v3 edited\n".to_owned())),
        290,
    );
    let effects = reread_soul(&mut machine, "v3\n", 290);
    assert_eq!(
        effects,
        vec![ProfilesEffect::Configure {
            name: "default".to_owned(),
            section: Section::Soul,
            value: "v3 edited\n".to_owned(),
        }]
    );
    let _ = machine.update(
        ProfilesInput::ConfigureAnswered(
            "default".to_owned(),
            Section::Soul,
            Err(CallFailure::NotSent),
        ),
        310,
    );
    assert_eq!(
        machine.view().notice.as_deref(),
        Some("Not sent: Talaria is not connected to the gateway. Nothing changed.")
    );
    assert!(
        matches!(machine.view().panel, Panel::Review(_)),
        "the review reopens"
    );

    let effects = reread_soul(&mut machine, "v3\n", 320);
    assert_eq!(
        effects,
        vec![ProfilesEffect::Configure {
            name: "default".to_owned(),
            section: Section::Soul,
            value: "v3 edited\n".to_owned(),
        }]
    );
    let _ = machine.update(
        ProfilesInput::ConfigureAnswered(
            "default".to_owned(),
            Section::Soul,
            Err(CallFailure::Refused {
                code: -32601,
                message: "unknown method: profiles.configure".to_owned(),
                reason: Some("unknown_method".to_owned()),
            }),
        ),
        340,
    );
    assert_eq!(
        machine.view().notice.as_deref(),
        Some("This gateway does not let Talaria change profiles over its socket.")
    );
    assert!(matches!(machine.view().panel, Panel::None));

    // Exactly 262 144 bytes open; one more does not.
    let effects = machine.update(ProfilesInput::Key(KeyAction::EditSoul), 350);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ProfilesEffect::Describe { .. }))
    );
    let mut big = description("default");
    big.soul = "a".repeat(262_144);
    let generation = describe_gen(&effects, "default");
    let effects = machine.update(
        ProfilesInput::Described(generation, "default".to_owned(), Ok(big)),
        360,
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ProfilesEffect::OpenEditor(_)))
    );
    let _ = machine.update(ProfilesInput::EditorReturned(RawEdit::Unchanged), 370);
    let _ = machine.update(ProfilesInput::Key(KeyAction::EditSoul), 380);
    let mut too_big = description("default");
    too_big.soul = "a".repeat(262_145);
    let generation = describe_gen(&effects, "default");
    let effects = machine.update(
        ProfilesInput::Described(generation, "default".to_owned(), Ok(too_big)),
        390,
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, ProfilesEffect::OpenEditor(_)))
    );
    assert_eq!(
        machine.view().notice.as_deref(),
        Some("This soul file is too large to edit here (256 KiB).")
    );

    // A soul that held invalid bytes warns in the review.
    soul_is_described(&mut machine, "a\u{FFFD}b\n", 400);
    let _ = machine.update(
        ProfilesInput::EditorReturned(RawEdit::Changed("a\u{FFFD}b\n!\n".to_owned())),
        410,
    );
    let review = review_of(&machine);
    assert_eq!(
        review.last().map(String::as_str),
        Some(
            "Note: this soul held text that is not UTF-8; saving writes it as the replacement character."
        )
    );

    // Tab, carriage return and trailing spaces reach the outbound value unchanged.
    soul_is_described(&mut machine, "l1\nl2\n", 420);
    let _ = machine.update(
        ProfilesInput::EditorReturned(RawEdit::Changed("l1\tx\nl2\r \n".to_owned())),
        430,
    );
    let effects = reread_soul(&mut machine, "l1\nl2\n", 430);
    assert_eq!(
        effects,
        vec![ProfilesEffect::Configure {
            name: "default".to_owned(),
            section: Section::Soul,
            value: "l1\tx\nl2\r \n".to_owned(),
        }]
    );

    // A describe failure shows its text and opens nothing.
    let mut failed = state();
    let _ = failed.update(ProfilesInput::Open, 0);
    full_load(&mut failed, 0, 0);
    let _ = failed.update(ProfilesInput::Key(KeyAction::EditDescription), 10);
    let _ = failed.update(
        ProfilesInput::Described(
            0,
            "default".to_owned(),
            Err(CallFailure::Refused {
                code: 4064,
                message: "profile 'default' not found".to_owned(),
                reason: None,
            }),
        ),
        20,
    );
    assert_eq!(
        failed.view().notice.as_deref(),
        Some("Hermes refused: profile 'default' not found")
    );
    assert!(matches!(failed.view().panel, Panel::None));
    let _ = failed.update(ProfilesInput::Key(KeyAction::EditSoul), 30);
    let _ = failed.update(
        ProfilesInput::Described(0, "default".to_owned(), Err(CallFailure::Unknown)),
        40,
    );
    assert_eq!(
        failed.view().notice.as_deref(),
        Some("Could not read this profile: no answer.")
    );
    assert!(matches!(failed.view().panel, Panel::None));
    let _ = failed.update(ProfilesInput::Key(KeyAction::EditSoul), 50);
    let _ = failed.update(
        ProfilesInput::Described(0, "default".to_owned(), Err(CallFailure::NotSent)),
        60,
    );
    assert_eq!(
        failed.view().notice.as_deref(),
        Some("Could not read this profile: not connected.")
    );
}

#[test]
fn rescan_confirm_and_outcomes() {
    const CONFIRM: &str = "Ask the gateway to rescan profiles now? It starts serving new profiles and retries parked ones, including their messaging bots. Press y to confirm, any other key to cancel.";
    let mut machine = state();
    let _ = machine.update(ProfilesInput::Open, 0);
    full_load(&mut machine, 0, 0);

    // `R` asks first and sends nothing.
    let effects = machine.update(ProfilesInput::Key(KeyAction::Rescan), 10);
    assert!(effects.is_empty());
    assert_eq!(machine.view().confirm.as_deref(), Some(CONFIRM));

    // Any key but `y` cancels and does nothing else.
    let effects = machine.update(ProfilesInput::Key(KeyAction::Char('n')), 20);
    assert!(effects.is_empty());
    assert_eq!(machine.view().confirm, None);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Rescan), 30);
    let effects = machine.update(ProfilesInput::Key(KeyAction::Back), 30);
    assert!(effects.is_empty(), "leaving answers the confirmation");
    assert_eq!(machine.view().confirm, None);

    // Only `y` proceeds.
    let _ = machine.update(ProfilesInput::Key(KeyAction::Rescan), 40);
    let effects = machine.update(ProfilesInput::Key(KeyAction::Char('y')), 40);
    assert_eq!(effects, vec![ProfilesEffect::Rescan]);
    assert_eq!(machine.view().confirm, None);

    // One rescan at a time.
    let effects = machine.update(ProfilesInput::Key(KeyAction::Rescan), 50);
    assert!(effects.is_empty());
    assert_eq!(machine.view().confirm, None);
    assert_eq!(
        machine.view().notice.as_deref(),
        Some("A rescan is already running.")
    );

    // Every outcome shows its text and then refreshes.
    let outcomes: &[(RescanSummary, &str)] = &[
        (
            RescanSummary::Finished {
                added: 2,
                parked: 1,
                removed: 0,
            },
            "Rescan finished: 2 added, 1 parked, 0 removed.",
        ),
        (
            RescanSummary::Pending,
            "The gateway is still rescanning; this list refreshes when it finishes.",
        ),
        (
            RescanSummary::SingleProfile,
            "This gateway serves one profile only, so a rescan changes nothing.",
        ),
    ];
    let mut generation = 1;
    for (summary, text) in outcomes {
        let effects = machine.update(
            ProfilesInput::RescanAnswered(Ok(summary.clone())),
            60 + generation * 10,
        );
        assert_eq!(machine.view().notice.as_deref(), Some(*text));
        assert_eq!(
            effects,
            vec![
                ProfilesEffect::Discover { generation },
                ProfilesEffect::List { generation },
                ProfilesEffect::Topology { generation },
            ]
        );
        generation += 1;
        let _ = machine.update(ProfilesInput::Key(KeyAction::Rescan), 60 + generation * 10);
        let _ = machine.update(
            ProfilesInput::Key(KeyAction::Char('y')),
            60 + generation * 10,
        );
    }
    let failures: &[(RescanFailure, &str)] = &[
        (
            RescanFailure::UnknownVerb,
            "This gateway does not support rescanning from a client. Wait up to 30 seconds, or restart it.",
        ),
        (
            RescanFailure::Timeout,
            "The gateway did not answer the rescan within 10 seconds; it may still be rescanning. Press r to refresh.",
        ),
        (
            RescanFailure::Other("connection failed".to_owned()),
            "The rescan request failed: connection failed.",
        ),
    ];
    for (failure, text) in failures {
        let effects = machine.update(
            ProfilesInput::RescanAnswered(Err(failure.clone())),
            200 + generation * 10,
        );
        assert_eq!(machine.view().notice.as_deref(), Some(*text));
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, ProfilesEffect::Discover { .. }))
        );
        generation += 1;
        let _ = machine.update(ProfilesInput::Key(KeyAction::Rescan), 200 + generation * 10);
        let _ = machine.update(
            ProfilesInput::Key(KeyAction::Char('y')),
            200 + generation * 10,
        );
    }
    let _ = machine.update(
        ProfilesInput::RescanAnswered(Ok(RescanSummary::Finished {
            added: 0,
            parked: 0,
            removed: 0,
        })),
        500,
    );

    // With nothing open, leaving closes the overlay.
    assert_eq!(
        machine.update(ProfilesInput::Key(KeyAction::Back), 510),
        vec![ProfilesEffect::Close]
    );
}
