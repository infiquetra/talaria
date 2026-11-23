//! The profiles state machine. Issue #253.

use talaria_core::profiles::{
    Applied, CallFailure, CreateRequest, CreatedProfile, DeleteOutcome, Description,
    DiscoveryFailure, FormField, FormView, GatewayView, HostTopology, KeyAction, KeyLabels, Panel,
    ProfilesEffect, ProfilesInput, ProfilesState, RawEdit, RegistryFailure, RescanFailure,
    RescanSummary, RosterEntry, RowTone, Section, TypedInput,
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
        "…",
        "a served row not yet described reads as being read"
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
                    "Keys: Enter or m models · c connect · e description · s soul · N rename · d delete · n new profile",
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
                detail.lines.get(1).map(String::as_str),
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
                [
                    "Keys: Enter or m models · c connect · e description · s soul · N rename · d delete · n new profile",
                    "Hermes refused: profile 'fixture' not found"
                ]
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
    parked_gateway.served.push("fixture-b".to_owned());
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
    let _ = machine.update(
        ProfilesInput::Described(0, "default".to_owned(), Err(CallFailure::NotSent)),
        1,
    );

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
    let _ = machine.update(
        ProfilesInput::ConfigureAnswered(
            "default".to_owned(),
            Section::Soul,
            Ok(Applied {
                ok: true,
                applied: true,
            }),
        ),
        120,
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
    let effects = machine.update(ProfilesInput::Key(KeyAction::EditSoul), 380);
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

const CONNECT_CONFIRM: &str = "Start a new session on profile fixture in this folder? Talaria then works on that profile; the current session keeps running in Hermes and /resume returns to it. Press y to confirm, any other key to cancel.";

/// Open and load the list, then select the named row.
fn loaded_at(name: &str) -> ProfilesState {
    let mut machine = state();
    machine.set_layout(120, 10);
    let _ = machine.update(ProfilesInput::Open, 0);
    full_load(&mut machine, 0, 0);
    for _ in 0..8 {
        let view = machine.view();
        if view
            .selected
            .is_some_and(|index| view.rows[index].name_cell == name)
        {
            return machine;
        }
        let _ = machine.update(ProfilesInput::Key(KeyAction::Down), 1);
    }
    panic!("no row named {name}");
}

fn has(effects: &[ProfilesEffect], wanted: &ProfilesEffect) -> bool {
    effects.iter().any(|effect| effect == wanted)
}

fn sends_nothing(effects: &[ProfilesEffect]) -> bool {
    !effects.iter().any(|effect| {
        matches!(
            effect,
            ProfilesEffect::OpenModelRoles { .. } | ProfilesEffect::StartSession { .. }
        )
    })
}

#[test]
fn models_key_and_enter_on_a_served_profile_open_its_model_roles() {
    let wanted = ProfilesEffect::OpenModelRoles {
        name: "fixture".to_owned(),
    };
    let mut machine = loaded_at("fixture");
    let effects = machine.update(ProfilesInput::Key(KeyAction::Models), 10);
    assert!(
        has(&effects, &wanted),
        "m opens the models page: {effects:?}"
    );

    // Selecting a profile with Enter configures it: the same page opens.
    let mut machine = loaded_at("fixture");
    let effects = machine.update(ProfilesInput::Key(KeyAction::Open), 10);
    assert!(
        has(&effects, &wanted),
        "Enter opens the models page: {effects:?}"
    );

    // The connected profile's models open too.
    let mut machine = loaded_at("default");
    let effects = machine.update(ProfilesInput::Key(KeyAction::Models), 10);
    assert!(has(
        &effects,
        &ProfilesEffect::OpenModelRoles {
            name: "default".to_owned()
        }
    ));
}

#[test]
fn enter_in_the_narrow_layout_shows_the_detail_first() {
    let mut machine = loaded_at("fixture");
    machine.set_layout(80, 10);
    let effects = machine.update(ProfilesInput::Key(KeyAction::Open), 10);
    assert!(sends_nothing(&effects), "{effects:?}");
    assert!(matches!(machine.view().panel, Panel::NarrowDetail(_)));
    let effects = machine.update(ProfilesInput::Key(KeyAction::Open), 20);
    assert!(has(
        &effects,
        &ProfilesEffect::OpenModelRoles {
            name: "fixture".to_owned()
        }
    ));
}

#[test]
fn models_and_connect_keys_on_an_unserved_profile_explain_and_send_nothing() {
    for key in [KeyAction::Models, KeyAction::Connect, KeyAction::Open] {
        let mut machine = loaded_at("fixture-b");
        let effects = machine.update(ProfilesInput::Key(key.clone()), 10);
        assert!(sends_nothing(&effects), "{key:?}: {effects:?}");
        assert_eq!(machine.view().confirm, None, "{key:?}");
        assert_eq!(
            machine.view().notice.as_deref(),
            Some(
                "Talaria cannot open the models of fixture-b or connect to it: this gateway does not serve it (parked). Next: fix the reason, then rescan with R in /profiles."
            ),
            "{key:?}"
        );
    }
}

#[test]
fn connect_key_asks_first_and_starts_only_on_y() {
    let mut machine = loaded_at("fixture");
    let effects = machine.update(ProfilesInput::Key(KeyAction::Connect), 10);
    assert!(sends_nothing(&effects), "{effects:?}");
    assert_eq!(machine.view().confirm.as_deref(), Some(CONNECT_CONFIRM));

    // Any other key cancels.
    let effects = machine.update(ProfilesInput::Key(KeyAction::Char('n')), 20);
    assert!(sends_nothing(&effects), "{effects:?}");
    assert_eq!(machine.view().confirm, None);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Connect), 30);
    let effects = machine.update(ProfilesInput::Key(KeyAction::Back), 30);
    assert!(sends_nothing(&effects), "{effects:?}");
    assert!(
        machine.is_open(),
        "Esc answers the question, not the overlay"
    );

    // `y` starts the session; the rescan is not asked for.
    let _ = machine.update(ProfilesInput::Key(KeyAction::Connect), 40);
    let effects = machine.update(ProfilesInput::Key(KeyAction::Char('y')), 40);
    assert_eq!(
        effects,
        vec![ProfilesEffect::StartSession {
            name: "fixture".to_owned()
        }]
    );
    assert_eq!(machine.view().confirm, None);
}

#[test]
fn connect_key_on_the_connected_profile_says_so_and_sends_nothing() {
    let mut machine = loaded_at("default");
    let effects = machine.update(ProfilesInput::Key(KeyAction::Connect), 10);
    assert!(sends_nothing(&effects), "{effects:?}");
    assert_eq!(machine.view().confirm, None);
    assert_eq!(
        machine.view().notice.as_deref(),
        Some("Talaria is already connected to profile default. /clear starts a new session here.")
    );
}

#[test]
fn detail_lists_the_actions_and_marks_the_connected_profile() {
    let detail_of = |machine: &ProfilesState| match machine.view().panel {
        Panel::Detail(detail) => detail,
        other => panic!("the list carries its detail: {other:?}"),
    };
    let machine = loaded_at("default");
    let detail = detail_of(&machine);
    assert_eq!(detail.title, "default (connected) · served");
    assert_eq!(
        detail.lines.first().map(String::as_str),
        Some("Keys: Enter or m models · e description · s soul · n new profile")
    );

    let machine = loaded_at("fixture");
    let detail = detail_of(&machine);
    assert_eq!(detail.title, "fixture · served");
    assert_eq!(
        detail.lines.first().map(String::as_str),
        Some(
            "Keys: Enter or m models · c connect · e description · s soul · N rename · d delete · n new profile"
        )
    );

    let machine = loaded_at("fixture-b");
    let detail = detail_of(&machine);
    assert_eq!(
        detail.lines.first().map(String::as_str),
        Some(
            "Keys: e description · s soul · n new profile. Models, connect, rename and delete need a profile this gateway serves."
        )
    );
}

#[test]
fn typing_m_or_c_in_the_filter_or_description_stays_text() {
    let mut machine = loaded_at("fixture");
    let _ = machine.update(ProfilesInput::Key(KeyAction::Filter), 10);
    assert!(machine.text_focus());
    for key in [KeyAction::Char('m'), KeyAction::Char('c')] {
        let effects = machine.update(ProfilesInput::Key(key), 10);
        assert!(sends_nothing(&effects), "{effects:?}");
    }
    assert_eq!(machine.view().filter.as_deref(), Some("mc"));
    assert_eq!(machine.view().confirm, None);
}

fn describes(effects: &[ProfilesEffect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            ProfilesEffect::Describe { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn model_cell(machine: &ProfilesState, name: &str) -> String {
    machine
        .view()
        .rows
        .into_iter()
        .find(|row| row.name_cell == name)
        .map(|row| row.model_cell)
        .unwrap_or_else(|| panic!("no row {name}"))
}

/// Hermes lists every profile but the default one with an empty model, so the screen
/// reads the model of each served row in the background, one request at a time.
#[test]
fn served_rows_without_a_listed_model_are_described_in_the_background() {
    let mut machine = state();
    let _ = machine.update(ProfilesInput::Open, 0);
    let effects = full_load(&mut machine, 0, 0);
    assert_eq!(
        describes(&effects),
        ["default"],
        "the selected row is read first and alone: {effects:?}"
    );
    assert_eq!(model_cell(&machine, "fixture"), "…", "a row being read");
    assert_eq!(model_cell(&machine, "fixture-a"), "…");
    assert_eq!(
        model_cell(&machine, "fixture-b"),
        "-",
        "a parked row is not read"
    );

    let effects = machine.update(
        ProfilesInput::Described(0, "default".to_owned(), Ok(description("default"))),
        20,
    );
    assert_eq!(describes(&effects), ["fixture"], "list order: {effects:?}");

    // Hide the detail panel; a background answer must not open it again.
    let _ = machine.update(ProfilesInput::Key(KeyAction::New), 25);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Back), 25);
    assert_eq!(machine.view().panel, Panel::None);

    let mut fixture = description("fixture");
    fixture.model_default = "fixture-model".to_owned();
    fixture.model_provider = "fixture-provider".to_owned();
    let effects = machine.update(
        ProfilesInput::Described(0, "fixture".to_owned(), Ok(fixture)),
        30,
    );
    assert_eq!(describes(&effects), ["fixture-a"], "{effects:?}");
    assert_eq!(
        model_cell(&machine, "fixture"),
        "fixture-model · fixture-provider"
    );
    let view = machine.view();
    assert_eq!(view.selected, Some(0), "the selection stays on default");
    assert_eq!(view.panel, Panel::None, "the detail stays closed");

    // A failure leaves the dash and prints no notice.
    let effects = machine.update(
        ProfilesInput::Described(0, "fixture-a".to_owned(), Err(CallFailure::Unknown)),
        40,
    );
    assert!(effects.is_empty(), "no notice and no retry: {effects:?}");
    assert_eq!(machine.view().notice, None);
    assert_eq!(model_cell(&machine, "fixture-a"), "-");

    // Selecting a row read in the background does not ask again.
    for now in [50, 60] {
        let effects = machine.update(ProfilesInput::Key(KeyAction::Down), now);
        assert!(effects.is_empty(), "the cache answers: {effects:?}");
    }
    let _ = machine.update(ProfilesInput::Key(KeyAction::First), 70);

    // A refresh reads the rows again, but not the one that failed in this opening.
    let _ = machine.update(ProfilesInput::Refresh, 100);
    let effects = full_load(&mut machine, 1, 100);
    assert_eq!(describes(&effects), ["default"]);
    let effects = machine.update(
        ProfilesInput::Described(1, "default".to_owned(), Ok(description("default"))),
        110,
    );
    assert_eq!(describes(&effects), ["fixture"], "{effects:?}");
    assert_eq!(model_cell(&machine, "fixture-a"), "-");

    // Closing stops the queue; a late answer is dropped and asks for nothing.
    let _ = machine.update(ProfilesInput::Closed, 120);
    let effects = machine.update(
        ProfilesInput::Described(1, "fixture".to_owned(), Ok(description("fixture"))),
        130,
    );
    assert!(effects.is_empty(), "{effects:?}");

    // A new opening tries the failed row again.
    let effects = machine.update(ProfilesInput::Open, 200);
    assert_eq!(describes(&effects), Vec::<String>::new());
    let _ = full_load(&mut machine, 2, 200);
    let _ = machine.update(
        ProfilesInput::Described(2, "default".to_owned(), Ok(description("default"))),
        210,
    );
    let effects = machine.update(
        ProfilesInput::Described(2, "fixture".to_owned(), Ok(description("fixture"))),
        220,
    );
    assert_eq!(describes(&effects), ["fixture-a"], "{effects:?}");
}

/// A background model read that starts while the selection waits to be described
/// must leave the selection's own describe in place.
#[test]
fn a_background_read_keeps_the_selections_pending_describe() {
    let mut machine = state();
    let _ = machine.update(ProfilesInput::Open, 0);
    let _ = full_load(&mut machine, 0, 0);
    let effects = machine.update(
        ProfilesInput::Described(0, "default".to_owned(), Ok(description("default"))),
        10,
    );
    assert_eq!(describes(&effects), ["fixture"], "{effects:?}");

    // Select the parked row; its describe waits for the selection to settle.
    let mut token = None;
    for now in [20, 21, 22] {
        let effects = machine.update(ProfilesInput::Key(KeyAction::Down), now);
        if let Some(found) = effects.iter().find_map(|effect| match effect {
            ProfilesEffect::ScheduleTick { token, .. } => Some(*token),
            _ => None,
        }) {
            token = Some(found);
        }
    }
    let token = token.expect("the selection schedules its describe");
    assert_eq!(
        machine.view().selected,
        Some(3),
        "the parked row is selected"
    );

    // The background answer arrives inside the settle window and reads the next row.
    let effects = machine.update(
        ProfilesInput::Described(0, "fixture".to_owned(), Ok(description("fixture"))),
        30,
    );
    assert_eq!(describes(&effects), ["fixture-a"], "{effects:?}");

    let effects = machine.update(ProfilesInput::Tick { token }, 200);
    assert_eq!(
        describes(&effects),
        ["fixture-b"],
        "the selected parked row is described when its timer fires: {effects:?}"
    );
}

fn keys_line(machine: &ProfilesState) -> String {
    match machine.view().panel {
        Panel::Detail(detail) => detail.lines[0].clone(),
        other => panic!("the list carries its detail: {other:?}"),
    }
}

/// The detail and header hints name the keys the user bound, and one word per action.
#[test]
fn key_hints_name_the_bound_keys() {
    let machine = loaded_at("fixture");
    assert_eq!(
        machine.view().hints,
        [
            "n new profile",
            "m models",
            "c connect",
            "r refresh",
            "R rescan",
            "e description",
            "s soul",
            "N rename",
            "d delete"
        ],
        "the header and the detail call profiles_description the same word"
    );

    let mut machine = loaded_at("fixture");
    machine.set_key_labels(KeyLabels {
        models: Some("F5".to_owned()),
        connect: None,
        description: Some("Ctrl+E".to_owned()),
        ..KeyLabels::default()
    });
    assert_eq!(
        keys_line(&machine),
        "Keys: Enter or F5 models · Ctrl+E description · s soul · N rename · d delete · n new profile"
    );
    assert_eq!(
        machine.view().hints,
        [
            "n new profile",
            "F5 models",
            "r refresh",
            "R rescan",
            "Ctrl+E description",
            "s soul",
            "N rename",
            "d delete"
        ]
    );

    // With neither Enter nor the models key bound, the hint leaves models out.
    machine.set_key_labels(KeyLabels {
        open: None,
        models: None,
        ..KeyLabels::default()
    });
    assert_eq!(
        keys_line(&machine),
        "Keys: c connect · e description · s soul · N rename · d delete · n new profile"
    );

    // A row the gateway does not serve names only the keys that work on it.
    let mut machine = loaded_at("fixture-b");
    machine.set_key_labels(KeyLabels {
        soul: Some("F6".to_owned()),
        ..KeyLabels::default()
    });
    assert_eq!(
        keys_line(&machine),
        "Keys: e description · F6 soul · n new profile. Models, connect, rename and delete need a profile this gateway serves."
    );
    machine.set_key_labels(KeyLabels {
        description: None,
        soul: None,
        new: None,
        ..KeyLabels::default()
    });
    assert_eq!(
        keys_line(&machine),
        "Models, connect, rename and delete need a profile this gateway serves."
    );
}

fn typed_of(machine: &ProfilesState) -> TypedInput {
    match machine.view().panel {
        Panel::Delete(input) | Panel::Rename(input) => input,
        other => panic!("no delete or rename panel open: {other:?}"),
    }
}

fn notice_of(machine: &ProfilesState) -> String {
    machine.view().notice.unwrap_or_default()
}

fn registry_sent(effects: &[ProfilesEffect]) -> bool {
    effects.iter().any(|effect| {
        matches!(
            effect,
            ProfilesEffect::Delete { .. } | ProfilesEffect::Rename { .. }
        )
    })
}

/// Creating a profile is named in the header, in every detail line and in an empty list.
#[test]
fn the_create_key_is_named_wherever_the_user_looks() {
    let machine = loaded_at("fixture");
    let hints = machine.view().hints;
    assert_eq!(
        hints.first().map(String::as_str),
        Some("n new profile"),
        "the create hint comes first, so a narrow header keeps it: {hints:?}"
    );
    assert!(hints.contains(&"d delete".to_owned()), "{hints:?}");
    assert!(hints.contains(&"N rename".to_owned()), "{hints:?}");
    assert_eq!(
        keys_line(&machine),
        "Keys: Enter or m models · c connect · e description · s soul · N rename · d delete · n new profile"
    );
    // The connected profile cannot be renamed or deleted, so its line leaves both out.
    assert_eq!(
        keys_line(&loaded_at("default")),
        "Keys: Enter or m models · e description · s soul · n new profile"
    );
    assert_eq!(
        keys_line(&loaded_at("fixture-b")),
        "Keys: e description · s soul · n new profile. Models, connect, rename and delete need a profile this gateway serves."
    );

    // A list with no rows says how to create one.
    let mut machine = state();
    let _ = machine.update(ProfilesInput::Open, 0);
    let _ = machine.update(
        ProfilesInput::Discovered(
            0,
            Ok(GatewayView {
                served: vec![],
                parked: vec![],
            }),
        ),
        0,
    );
    let _ = machine.update(ProfilesInput::Listed(0, Ok(vec![])), 0);
    let _ = machine.update(
        ProfilesInput::Topology(
            0,
            Ok(HostTopology {
                stopped: vec![],
                ..topo()
            }),
        ),
        0,
    );
    assert!(machine.view().rows.is_empty());
    assert_eq!(
        machine.view().empty.as_deref(),
        Some("No profiles. Press n to create one.")
    );
    assert_eq!(loaded_at("fixture").view().empty, None);
}

/// A list that is empty because a read failed never claims there are no profiles.
#[test]
fn an_empty_list_after_a_failed_load_names_the_failure() {
    // No gateway: discovery, the roster and topology all fail, and no notice shows.
    let mut machine = state();
    let _ = machine.update(ProfilesInput::Open, 0);
    let _ = machine.update(ProfilesInput::Discovered(0, Err(discovery_failure())), 0);
    let _ = machine.update(ProfilesInput::Listed(0, Err("not connected".to_owned())), 0);
    let _ = machine.update(
        ProfilesInput::Topology(0, Err("not connected".to_owned())),
        0,
    );
    let view = machine.view();
    assert!(view.rows.is_empty());
    assert_eq!(
        view.empty.as_deref(),
        Some(
            "no Hermes gateway is running for this home Run `hermes gateway ensure --json` to start the Hermes gateway, then try again."
        )
    );
    assert!(!view.empty.unwrap_or_default().contains("No profiles"));

    // The gateway answered with nothing served but the profile list did not come back.
    let mut machine = state();
    let _ = machine.update(ProfilesInput::Open, 0);
    let _ = machine.update(
        ProfilesInput::Discovered(
            0,
            Ok(GatewayView {
                served: vec![],
                parked: vec![],
            }),
        ),
        0,
    );
    let _ = machine.update(ProfilesInput::Listed(0, Err("no answer".to_owned())), 0);
    let _ = machine.update(
        ProfilesInput::Topology(
            0,
            Ok(HostTopology {
                stopped: vec![],
                ..topo()
            }),
        ),
        0,
    );
    let view = machine.view();
    assert!(view.rows.is_empty());
    assert_eq!(
        view.empty.as_deref(),
        Some("Hermes did not return the profile list. Press r to try again.")
    );
}

/// Delete asks for the exact name, lists what Hermes removes, and sends only on a match.
#[test]
fn delete_lists_what_goes_and_needs_the_typed_name() {
    let mut machine = loaded_at("fixture");
    let effects = machine.update(ProfilesInput::Key(KeyAction::Delete), 10);
    assert!(!registry_sent(&effects), "{effects:?}");
    assert!(machine.text_focus(), "the typed name is text");
    let panel = typed_of(&machine);
    assert_eq!(panel.name, "fixture");
    assert_eq!(
        panel.lines,
        [
            "Delete profile fixture",
            "Hermes permanently removes its config, API keys, memories, sessions, skills and cron jobs. This cannot be undone.",
            "fixture is served, so Hermes stops its gateway first.",
            "Type fixture to confirm.",
        ]
    );
    assert_eq!(panel.footer, "Enter delete · Esc cancel");

    // A wrong name sends nothing and says what to type; the letters d, n and c stay text.
    type_text(&mut machine, "fixtur", 20);
    let effects = machine.update(ProfilesInput::Key(KeyAction::Enter), 20);
    assert!(!registry_sent(&effects), "{effects:?}");
    assert_eq!(
        typed_of(&machine).error.as_deref(),
        Some("Type fixture exactly, or press Esc to cancel.")
    );
    type_text(&mut machine, "e", 30);
    assert_eq!(typed_of(&machine).text, "fixture");
    assert_eq!(typed_of(&machine).error, None);
    let effects = machine.update(ProfilesInput::Key(KeyAction::Enter), 40);
    assert_eq!(
        effects
            .iter()
            .filter(|effect| registry_sent(std::slice::from_ref(effect)))
            .collect::<Vec<_>>(),
        [&ProfilesEffect::Delete {
            name: "fixture".to_owned()
        }]
    );
    assert!(!machine.text_focus(), "the panel closes while Hermes works");

    // A second delete while one runs is refused.
    let mut other = loaded_at("fixture-a");
    let _ = other.update(ProfilesInput::Key(KeyAction::Delete), 10);
    type_text(&mut other, "fixture-a", 10);
    let _ = other.update(ProfilesInput::Key(KeyAction::Enter), 10);
    let _ = other.update(ProfilesInput::Key(KeyAction::Down), 11);
    let effects = other.update(ProfilesInput::Key(KeyAction::Delete), 12);
    assert!(!registry_sent(&effects));
    assert_eq!(
        notice_of(&other),
        "A delete or rename is already in progress."
    );

    // Success refreshes the list and says so.
    let effects = machine.update(
        ProfilesInput::DeleteAnswered("fixture".to_owned(), Ok(DeleteOutcome::Deleted)),
        50,
    );
    assert!(effects.iter().any(is_fetch), "{effects:?}");
    assert_eq!(notice_of(&machine), "Deleted profile fixture.");

    // Esc leaves the panel without sending.
    let mut machine = loaded_at("fixture");
    let _ = machine.update(ProfilesInput::Key(KeyAction::Delete), 10);
    let effects = machine.update(ProfilesInput::Key(KeyAction::Back), 10);
    assert!(!registry_sent(&effects));
    assert!(matches!(
        machine.view().panel,
        Panel::None | Panel::Detail(_)
    ));
}

/// Each answer reads as one line in Hermes's own words where it gave some.
#[test]
fn delete_answers_name_the_outcome() {
    let cases = [
        (
            Ok(DeleteOutcome::SettlementPending {
                retry_command: "hermes profile purge-identity fixture".to_owned(),
            }),
            "Deleted profile fixture, but Hermes has not finished clearing its session identity. Run: hermes profile purge-identity fixture",
            true,
        ),
        (
            Err(RegistryFailure::Refused(
                "a running gateway still owns this profile; retry the delete once it settles"
                    .to_owned(),
            )),
            "Hermes refused to delete fixture: a running gateway still owns this profile; retry the delete once it settles",
            false,
        ),
        (
            Err(RegistryFailure::NoTicket(
                "the gateway does not serve that profile".to_owned(),
            )),
            "Talaria could not get a ticket to delete fixture: the gateway does not serve that profile",
            false,
        ),
        (
            Err(RegistryFailure::Unknown),
            "Talaria lost the answer to the delete request. Hermes may have deleted fixture; the list below is fresh.",
            true,
        ),
        (
            Err(RegistryFailure::NotSent(
                "no Hermes gateway is running for this home".to_owned(),
            )),
            "Not sent: no Hermes gateway is running for this home. Nothing changed.",
            false,
        ),
    ];
    for (answer, words, refreshes) in cases {
        let mut machine = loaded_at("fixture");
        let _ = machine.update(ProfilesInput::Key(KeyAction::Delete), 10);
        type_text(&mut machine, "fixture", 10);
        let _ = machine.update(ProfilesInput::Key(KeyAction::Enter), 10);
        let effects = machine.update(
            ProfilesInput::DeleteAnswered("fixture".to_owned(), answer),
            20,
        );
        assert_eq!(notice_of(&machine), words);
        assert_eq!(effects.iter().any(is_fetch), refreshes, "{words}");
        if machine
            .view()
            .rows
            .iter()
            .all(|row| row.name_cell != "fixture")
        {
            // Hermes deleted it, so the row is gone at once.
            continue;
        }
        // The next delete is allowed again.
        let _ = machine.update(ProfilesInput::Key(KeyAction::Delete), 30);
        assert!(
            matches!(machine.view().panel, Panel::Delete(_)),
            "{words}: {:?}",
            machine.view().notice
        );
    }
}

/// The default profile, the connected one, one the gateway does not serve, and any
/// profile behind a remote gateway are refused with one line that says what to do.
#[test]
fn delete_and_rename_refuse_what_hermes_or_talaria_cannot_do() {
    let cases = [
        (
            "default",
            false,
            KeyAction::Delete,
            "Hermes never deletes the default profile: it is the Hermes home itself. To remove everything, run: hermes uninstall",
        ),
        (
            "default",
            false,
            KeyAction::Rename,
            "The default profile keeps its name: Hermes can change only its display name, which Talaria does not offer.",
        ),
        (
            "fixture-b",
            false,
            KeyAction::Delete,
            "Talaria can delete only a profile this gateway serves; fixture-b is parked. Run: hermes profile delete fixture-b",
        ),
        (
            "fixture-b",
            false,
            KeyAction::Rename,
            "Talaria can rename only a profile this gateway serves; fixture-b is parked. Run: hermes profile rename fixture-b NEW-NAME",
        ),
        (
            "fixture",
            true,
            KeyAction::Delete,
            "Talaria deletes profiles only through a gateway on this machine. On the gateway's machine, run: hermes profile delete fixture",
        ),
        (
            "fixture",
            true,
            KeyAction::Rename,
            "Talaria renames profiles only through a gateway on this machine. On the gateway's machine, run: hermes profile rename fixture NEW-NAME",
        ),
    ];
    for (row, remote, key, words) in cases {
        let mut machine = loaded_at(row);
        machine.set_remote_gateway(remote);
        let effects = machine.update(ProfilesInput::Key(key.clone()), 10);
        assert!(!registry_sent(&effects), "{words}");
        assert!(!machine.text_focus(), "{words}");
        assert_eq!(notice_of(&machine), words);
    }

    // The connected profile: connected to fixture, with fixture selected.
    for (key, words) in [
        (
            KeyAction::Delete,
            "Talaria is connected to profile fixture, so it cannot delete it. Connect to another profile first, then delete fixture.",
        ),
        (
            KeyAction::Rename,
            "Talaria is connected to profile fixture, so it cannot rename it. Connect to another profile first, then rename fixture.",
        ),
    ] {
        let mut machine = loaded_at("fixture");
        machine.set_connected_profile("fixture");
        let effects = machine.update(ProfilesInput::Key(key), 10);
        assert!(!registry_sent(&effects), "{words}");
        assert_eq!(notice_of(&machine), words);
    }
}

/// Rename checks the new name as create does, then sends it and selects the new name.
#[test]
fn rename_validates_the_new_name_and_reports_the_result() {
    let mut machine = loaded_at("fixture");
    let _ = machine.update(ProfilesInput::Key(KeyAction::Rename), 10);
    assert!(machine.text_focus());
    let panel = typed_of(&machine);
    assert_eq!(
        panel.lines,
        [
            "Rename profile fixture",
            "fixture is served, so Hermes stops its gateway while it moves the profile.",
            "New name:",
        ]
    );
    assert_eq!(panel.footer, "Enter rename · Esc cancel");

    for (typed, error) in [
        ("", "Enter a name."),
        (
            "Bad Name",
            "Use lower-case letters, digits, - or _, starting with a letter or digit, up to 64 characters.",
        ),
        ("sudo", "sudo is reserved by Hermes. Choose another name."),
        ("fixture-a", "A profile named fixture-a already exists."),
    ] {
        let _ = machine.update(ProfilesInput::Key(KeyAction::ClearLine), 20);
        type_text(&mut machine, typed, 20);
        let effects = machine.update(ProfilesInput::Key(KeyAction::Enter), 20);
        assert!(!registry_sent(&effects), "{typed}");
        assert_eq!(typed_of(&machine).error.as_deref(), Some(error), "{typed}");
    }

    let _ = machine.update(ProfilesInput::Key(KeyAction::ClearLine), 30);
    type_text(&mut machine, "Fixture-R ", 30);
    let effects = machine.update(ProfilesInput::Key(KeyAction::Enter), 30);
    assert!(
        has(
            &effects,
            &ProfilesEffect::Rename {
                name: "fixture".to_owned(),
                new_name: "fixture-r".to_owned(),
            }
        ),
        "{effects:?}"
    );

    let effects = machine.update(
        ProfilesInput::RenameAnswered("fixture".to_owned(), Ok("fixture-r".to_owned())),
        40,
    );
    assert!(effects.iter().any(is_fetch), "{effects:?}");
    assert_eq!(notice_of(&machine), "Renamed profile fixture to fixture-r.");

    let mut machine = loaded_at("fixture");
    let _ = machine.update(ProfilesInput::Key(KeyAction::Rename), 10);
    type_text(&mut machine, "fixture-r", 10);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Enter), 10);
    let _ = machine.update(
        ProfilesInput::RenameAnswered(
            "fixture".to_owned(),
            Err(RegistryFailure::Refused(
                "A profile named 'fixture-r' already exists.".to_owned(),
            )),
        ),
        20,
    );
    assert_eq!(
        notice_of(&machine),
        "Hermes refused to rename fixture: A profile named 'fixture-r' already exists."
    );
}

fn tick_after(effects: &[ProfilesEffect], wanted: u64) -> u64 {
    effects
        .iter()
        .find_map(|effect| match effect {
            ProfilesEffect::ScheduleTick { after_ms, token } if *after_ms == wanted => Some(*token),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no tick after {wanted} ms: {effects:?}"))
}

fn progress_of(machine: &ProfilesState) -> Option<String> {
    match machine.view().panel {
        Panel::Progress(line) => Some(line),
        _ => None,
    }
}

fn row_of(machine: &ProfilesState, name: &str) -> Option<(String, String)> {
    machine
        .view()
        .rows
        .into_iter()
        .find(|row| row.name_cell == name)
        .map(|row| (row.label, row.next_step))
}

/// While Hermes deletes or renames a profile, the panel says so with the seconds waited, the
/// row is marked, and every key that acts on that row refuses in one line until the answer.
#[test]
fn a_delete_in_flight_shows_progress_and_the_row_refuses_every_action() {
    let mut machine = loaded_at("fixture");
    let _ = machine.update(ProfilesInput::Key(KeyAction::Delete), 10_000);
    type_text(&mut machine, "fixture", 10_000);
    let effects = machine.update(ProfilesInput::Key(KeyAction::Enter), 10_000);
    assert!(registry_sent(&effects));
    assert_eq!(
        progress_of(&machine).as_deref(),
        Some("Deleting fixture: Hermes is stopping its gateway (0 s).")
    );
    assert_eq!(
        row_of(&machine, "fixture"),
        Some(("deleting".to_owned(), "wait for Hermes".to_owned()))
    );

    // The line counts the seconds on its own.
    let token = tick_after(&effects, 1_000);
    let effects = machine.update(ProfilesInput::Tick { token }, 13_400);
    assert!(has(&effects, &ProfilesEffect::Redraw), "{effects:?}");
    let _ = tick_after(&effects, 1_000);
    assert_eq!(
        progress_of(&machine).as_deref(),
        Some("Deleting fixture: Hermes is stopping its gateway (3 s).")
    );

    // Every action on the row is refused with one line and sends nothing.
    for key in [
        KeyAction::Connect,
        KeyAction::Models,
        KeyAction::Enter,
        KeyAction::EditDescription,
        KeyAction::EditSoul,
        KeyAction::Rename,
        KeyAction::Delete,
    ] {
        let effects = machine.update(ProfilesInput::Key(key.clone()), 14_000);
        assert!(sends_nothing(&effects), "{key:?}: {effects:?}");
        assert!(!registry_sent(&effects), "{key:?}");
        if matches!(key, KeyAction::EditDescription | KeyAction::EditSoul) {
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, ProfilesEffect::Describe { .. })),
                "{key:?}: {effects:?}"
            );
        }
        assert_eq!(machine.view().confirm, None, "{key:?}");
        assert!(!machine.text_focus(), "{key:?}");
        assert_eq!(
            notice_of(&machine),
            "fixture is being deleted. Wait until Hermes answers.",
            "{key:?}"
        );
        let _ = machine.update(ProfilesInput::UnboundKey, 14_000);
    }

    // Closing and opening the list again keeps the line and its clock.
    let _ = machine.update(ProfilesInput::Closed, 15_000);
    let effects = machine.update(ProfilesInput::Open, 16_000);
    let _ = tick_after(&effects, 1_000);
    full_load(&mut machine, 1, 16_000);
    assert_eq!(
        progress_of(&machine).as_deref(),
        Some("Deleting fixture: Hermes is stopping its gateway (6 s).")
    );

    // The answer ends it: the line goes, and the row acts again.
    let _ = machine.update(
        ProfilesInput::DeleteAnswered(
            "fixture".to_owned(),
            Err(RegistryFailure::Refused("busy".to_owned())),
        ),
        17_000,
    );
    assert_eq!(progress_of(&machine), None);
    let _ = machine.update(ProfilesInput::UnboundKey, 17_000);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Connect), 17_000);
    assert_eq!(machine.view().confirm.as_deref(), Some(CONNECT_CONFIRM));

    // A rename names both names; another row still says one request runs at a time.
    let mut machine = loaded_at("fixture");
    let _ = machine.update(ProfilesInput::Key(KeyAction::Rename), 0);
    type_text(&mut machine, "fixture-r", 0);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Enter), 0);
    assert_eq!(
        progress_of(&machine).as_deref(),
        Some("Renaming fixture to fixture-r: Hermes is stopping its gateway (0 s).")
    );
    assert_eq!(row_of(&machine, "fixture").unwrap().0, "renaming");
    let _ = machine.update(ProfilesInput::Key(KeyAction::Models), 0);
    assert_eq!(
        notice_of(&machine),
        "fixture is being renamed. Wait until Hermes answers."
    );
    let _ = machine.update(ProfilesInput::Key(KeyAction::Down), 0);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Delete), 0);
    assert_eq!(
        notice_of(&machine),
        "A delete or rename is already in progress."
    );
}

/// Serve a just-created profile and select it; returns the create's scheduled ticks.
fn created_and_served(machine: &mut ProfilesState) -> Vec<(u64, u64)> {
    machine.set_layout(120, 10);
    let _ = machine.update(ProfilesInput::Open, 0);
    full_load(machine, 0, 0);
    let _ = submit_create(machine, "fixture-new", 500);
    let effects = machine.update(
        ProfilesInput::CreateAnswered(Ok(CreatedProfile {
            name: "fixture-new".to_owned(),
            keys_copied: true,
        })),
        1_000,
    );
    let ticks = effects
        .iter()
        .filter_map(|effect| match effect {
            ProfilesEffect::ScheduleTick { after_ms, token } => Some((*after_ms, *token)),
            _ => None,
        })
        .collect();
    let effects = machine.update(ProfilesInput::Refresh, 2_000);
    answer_refresh(machine, &effects, 2_000, &["fixture-new"]);
    for _ in 0..10 {
        let view = machine.view();
        if view
            .selected
            .is_some_and(|index| view.rows[index].name_cell == "fixture-new")
        {
            break;
        }
        let _ = machine.update(ProfilesInput::Key(KeyAction::Down), 2_100);
    }
    ticks
}

/// Answer the refresh these effects asked for, with `extra` served and on disk.
fn answer_refresh(
    machine: &mut ProfilesState,
    effects: &[ProfilesEffect],
    at: u64,
    extra: &[&str],
) {
    let generation = effects
        .iter()
        .find_map(|effect| match effect {
            ProfilesEffect::Discover { generation } => Some(*generation),
            _ => None,
        })
        .expect("a refresh");
    let mut served = gateway();
    let mut listed = roster();
    for name in extra {
        served.served.push((*name).to_owned());
        listed.push(RosterEntry {
            name: (*name).to_owned(),
            display_name: (*name).to_owned(),
            ..RosterEntry::default()
        });
    }
    let _ = machine.update(ProfilesInput::Discovered(generation, Ok(served)), at);
    let _ = machine.update(ProfilesInput::Listed(generation, Ok(listed)), at);
    let _ = machine.update(ProfilesInput::Topology(generation, Ok(topo())), at);
}

fn names_of(machine: &ProfilesState) -> Vec<String> {
    machine
        .view()
        .rows
        .into_iter()
        .map(|row| row.name_cell)
        .collect()
}

/// A profile deleted or renamed soon after its create never comes back as "starting".
#[test]
fn a_deleted_or_renamed_new_profile_does_not_come_back() {
    let mut machine = state();
    let ticks = created_and_served(&mut machine);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Delete), 3_000);
    type_text(&mut machine, "fixture-new", 3_000);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Enter), 3_000);
    let effects = machine.update(
        ProfilesInput::DeleteAnswered("fixture-new".to_owned(), Ok(DeleteOutcome::Deleted)),
        4_000,
    );
    // Gone at once, before the refresh answers.
    assert!(!names_of(&machine).contains(&"fixture-new".to_owned()));
    answer_refresh(&mut machine, &effects, 4_500, &[]);
    assert!(!names_of(&machine).contains(&"fixture-new".to_owned()));
    // The create's own later refresh does not bring it back either.
    let token = ticks.iter().find(|(after, _)| *after == 35_000).unwrap().1;
    let effects = machine.update(ProfilesInput::Tick { token }, 36_000);
    answer_refresh(&mut machine, &effects, 36_000, &[]);
    assert!(
        !names_of(&machine).contains(&"fixture-new".to_owned()),
        "{:?}",
        names_of(&machine)
    );

    // A rename leaves no row under the old name.
    let mut machine = state();
    let _ = created_and_served(&mut machine);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Rename), 3_000);
    type_text(&mut machine, "fixture-good", 3_000);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Enter), 3_000);
    let effects = machine.update(
        ProfilesInput::RenameAnswered("fixture-new".to_owned(), Ok("fixture-good".to_owned())),
        4_000,
    );
    assert!(!names_of(&machine).contains(&"fixture-new".to_owned()));
    assert!(names_of(&machine).contains(&"fixture-good".to_owned()));
    answer_refresh(&mut machine, &effects, 4_500, &["fixture-good"]);
    assert!(
        !names_of(&machine).contains(&"fixture-new".to_owned()),
        "{:?}",
        names_of(&machine)
    );
    assert_eq!(row_of(&machine, "fixture-good").unwrap().0, "served");

    // A lost answer may have deleted it, so the list trusts Hermes alone.
    let mut machine = state();
    let _ = created_and_served(&mut machine);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Delete), 3_000);
    type_text(&mut machine, "fixture-new", 3_000);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Enter), 3_000);
    let effects = machine.update(
        ProfilesInput::DeleteAnswered("fixture-new".to_owned(), Err(RegistryFailure::Unknown)),
        4_000,
    );
    answer_refresh(&mut machine, &effects, 4_500, &[]);
    assert!(!names_of(&machine).contains(&"fixture-new".to_owned()));
}

/// Answer a refresh after `fixture` became `fixture-r`, with the new name served or not yet.
fn answer_renamed(machine: &mut ProfilesState, effects: &[ProfilesEffect], at: u64, served: bool) {
    let generation = effects
        .iter()
        .find_map(|effect| match effect {
            ProfilesEffect::Discover { generation } => Some(*generation),
            _ => None,
        })
        .expect("a refresh");
    let mut gateway = gateway();
    gateway.served.retain(|name| name != "fixture");
    if served {
        gateway.served.push("fixture-r".to_owned());
    }
    let mut listed = roster();
    for entry in &mut listed {
        if entry.name == "fixture" {
            "fixture-r".clone_into(&mut entry.name);
            "fixture-r".clone_into(&mut entry.display_name);
        }
    }
    let _ = machine.update(ProfilesInput::Discovered(generation, Ok(gateway)), at);
    let _ = machine.update(ProfilesInput::Listed(generation, Ok(listed)), at);
    let _ = machine.update(ProfilesInput::Topology(generation, Ok(topo())), at);
}

/// A renamed profile reads "starting" under its new name and turns "served" once the gateway
/// serves that name, without a key press.
#[test]
fn a_renamed_profile_is_followed_until_the_gateway_serves_it() {
    let mut machine = loaded_at("fixture");
    let _ = machine.update(ProfilesInput::Key(KeyAction::Rename), 100_000);
    type_text(&mut machine, "fixture-r", 100_000);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Enter), 100_000);
    let effects = machine.update(
        ProfilesInput::RenameAnswered("fixture".to_owned(), Ok("fixture-r".to_owned())),
        101_000,
    );
    assert_eq!(row_of(&machine, "fixture-r").unwrap().0, "starting");
    // The gateway has not rescanned yet.
    answer_renamed(&mut machine, &effects, 101_500, false);
    assert_eq!(row_of(&machine, "fixture-r").unwrap().0, "starting");
    // A later refresh reads the list again on its own and finds the new name served.
    let token = tick_after(&effects, 5_000);
    let refresh = machine.update(ProfilesInput::Tick { token }, 106_000);
    answer_renamed(&mut machine, &refresh, 106_200, true);
    assert_eq!(row_of(&machine, "fixture-r").unwrap().0, "served");
    assert_eq!(row_of(&machine, "fixture"), None);
    // Never served: after 40 seconds the row says so instead of "starting".
    let _ = tick_after(&effects, 15_000);
    let _ = tick_after(&effects, 35_000);
    let _ = tick_after(&effects, 40_000);
}

/// In the narrow layout a running delete does not hide the detail: the first Enter on another
/// row shows its detail, which carries the progress line, and opens nothing.
#[test]
fn the_narrow_detail_shows_while_a_delete_runs() {
    let mut machine = loaded_at("fixture");
    machine.set_layout(80, 10);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Delete), 10_000);
    type_text(&mut machine, "fixture", 10_000);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Enter), 10_000);
    let progress = "Deleting fixture: Hermes is stopping its gateway (0 s).";
    assert_eq!(progress_of(&machine).as_deref(), Some(progress));

    let _ = machine.update(ProfilesInput::Key(KeyAction::Down), 10_000);
    let effects = machine.update(ProfilesInput::Key(KeyAction::Open), 10_000);
    assert!(sends_nothing(&effects), "{effects:?}");
    match machine.view().panel {
        Panel::NarrowDetail(detail) => {
            assert!(detail.title.starts_with("fixture-a"), "{detail:?}");
            assert_eq!(detail.lines.first().map(String::as_str), Some(progress));
        }
        other => panic!("no narrow detail: {other:?}"),
    }
    // Esc goes back to the list and its progress line.
    let _ = machine.update(ProfilesInput::Key(KeyAction::Back), 10_000);
    assert_eq!(progress_of(&machine).as_deref(), Some(progress));
    // The wide layout keeps the progress line in the panel.
    machine.set_layout(120, 10);
    let _ = machine.update(ProfilesInput::Key(KeyAction::OpenDetail), 10_000);
    assert_eq!(progress_of(&machine).as_deref(), Some(progress));
}

/// The line that waits for the gateway goes once the list shows the new profile served.
#[test]
fn the_waiting_line_goes_once_the_new_profile_is_served() {
    let mut machine = state();
    machine.set_layout(120, 10);
    let _ = machine.update(ProfilesInput::Open, 0);
    full_load(&mut machine, 0, 0);
    let _ = submit_create(&mut machine, "fixture-new", 500);
    let effects = machine.update(
        ProfilesInput::CreateAnswered(Ok(CreatedProfile {
            name: "fixture-new".to_owned(),
            keys_copied: true,
        })),
        1_000,
    );
    assert_eq!(
        notice_of(&machine),
        "Created profile fixture-new. Provider keys copied: yes. Waiting for the gateway to serve it..."
    );
    // Not served yet: the line stays.
    let token = tick_after(&effects, 0);
    let refresh = machine.update(ProfilesInput::Tick { token }, 1_000);
    answer_refresh(&mut machine, &refresh, 1_200, &[]);
    assert!(notice_of(&machine).contains("Waiting for the gateway"));
    // Served: the waiting part goes and the rest stays.
    let token = tick_after(&effects, 5_000);
    let refresh = machine.update(ProfilesInput::Tick { token }, 6_000);
    answer_refresh(&mut machine, &refresh, 6_200, &["fixture-new"]);
    assert_eq!(row_of(&machine, "fixture-new").unwrap().0, "served");
    assert_eq!(
        notice_of(&machine),
        "Created profile fixture-new. Provider keys copied: yes."
    );
}

/// A delete or rename of a profile that is already gone says so and reads the list again.
#[test]
fn a_profile_already_gone_refreshes_the_list() {
    for (key, answer) in [
        (
            KeyAction::Delete,
            ProfilesInput::DeleteAnswered(
                "fixture".to_owned(),
                Err(RegistryFailure::Gone(
                    "the gateway no longer serves it".to_owned(),
                )),
            ),
        ),
        (
            KeyAction::Rename,
            ProfilesInput::RenameAnswered(
                "fixture".to_owned(),
                Err(RegistryFailure::Gone(
                    "Profile 'fixture' does not exist".to_owned(),
                )),
            ),
        ),
    ] {
        let mut machine = loaded_at("fixture");
        let _ = machine.update(ProfilesInput::Key(key.clone()), 10);
        type_text(&mut machine, "fixture", 10);
        if key == KeyAction::Rename {
            type_text(&mut machine, "-r", 10);
        }
        let _ = machine.update(ProfilesInput::Key(KeyAction::Enter), 10);
        let effects = machine.update(answer, 20);
        assert!(effects.iter().any(is_fetch), "{key:?}: {effects:?}");
        let verb = if key == KeyAction::Delete {
            "delete"
        } else {
            "rename"
        };
        let words = if key == KeyAction::Delete {
            "the gateway no longer serves it"
        } else {
            "Profile 'fixture' does not exist"
        };
        assert_eq!(
            notice_of(&machine),
            format!("Could not {verb} fixture: {words}. The list below is fresh.")
        );
    }
}

/// A model saved on the model roles page shows in the list at once, closed or open.
#[test]
fn a_saved_main_model_shows_in_the_list_at_once() {
    let mut machine = loaded_at("fixture");
    let _ = machine.update(
        ProfilesInput::Described(0, "fixture".to_owned(), Ok(description("fixture"))),
        10,
    );
    assert_eq!(
        model_cell(&machine, "fixture"),
        "minimax-m3.1-flash · ollama-cloud"
    );
    let _ = machine.update(ProfilesInput::Closed, 20);
    let _ = machine.update(
        ProfilesInput::ModelSaved {
            name: "fixture".to_owned(),
            provider: "fixture-provider".to_owned(),
            model: "fixture-model".to_owned(),
        },
        30,
    );
    // Reopened, the list shows the saved model while Hermes is read again.
    let _ = machine.update(ProfilesInput::Open, 40);
    assert_eq!(
        model_cell(&machine, "fixture"),
        "fixture-model · fixture-provider"
    );
    match machine.view().panel {
        Panel::Detail(detail) => assert!(
            detail
                .lines
                .contains(&"Model: fixture-provider · fixture-model".to_owned()),
            "{detail:?}"
        ),
        other => panic!("no detail: {other:?}"),
    }
}
