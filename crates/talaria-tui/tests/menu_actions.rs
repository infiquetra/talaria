use talaria_app::app::FLEET_DISABLED;
use talaria_core::commands::LocalCommandTable;
use talaria_core::voice::{VoiceCommand, parse_voice_args};
use talaria_tui::keys::Action;
use talaria_tui::workspace::menu::{MenuBinding, MenuItem, MenuKey, MenuState};

const REMOTE_VOICE: &str = "Voice needs a local Hermes gateway for now, because a remote gateway \
                            would record and play on its own machine.";

struct Expected {
    menu: &'static str,
    label: &'static str,
    actions: &'static [&'static str],
    commands: &'static [&'static str],
}

const EXPECTED: &[Expected] = &[
    Expected {
        menu: "Session",
        label: "Sessions",
        actions: &["session_picker_open"],
        commands: &["resume"],
    },
    Expected {
        menu: "Session",
        label: "Fleet",
        actions: &["fleet_view_open"],
        commands: &["fleet"],
    },
    Expected {
        menu: "Session",
        label: "Next waiting request",
        actions: &["attention_next"],
        commands: &[],
    },
    Expected {
        menu: "Model",
        label: "Choose model",
        actions: &["model_picker"],
        commands: &["model"],
    },
    Expected {
        menu: "Model",
        label: "Choose reasoning effort",
        actions: &[],
        commands: &["effort"],
    },
    Expected {
        menu: "View",
        label: "Right work panel",
        actions: &["layout_panel"],
        commands: &[],
    },
    Expected {
        menu: "View",
        label: "Top bar",
        actions: &["toggle_menu_bar"],
        commands: &[],
    },
    Expected {
        menu: "View",
        label: "Tasks",
        actions: &["toggle_tasks"],
        commands: &[],
    },
    Expected {
        menu: "View",
        label: "Agents",
        actions: &["agents_panel"],
        commands: &[],
    },
    Expected {
        menu: "View",
        label: "Transcript",
        actions: &["transcript_open"],
        commands: &[],
    },
    Expected {
        menu: "Voice",
        label: "Record or stop",
        actions: &["voice_record"],
        commands: &[],
    },
    Expected {
        menu: "Settings",
        label: "Talaria settings",
        actions: &[],
        commands: &["config"],
    },
    Expected {
        menu: "Settings",
        label: "Providers",
        actions: &[],
        commands: &["providers"],
    },
    Expected {
        menu: "Settings",
        label: "MCP servers",
        actions: &[],
        commands: &["mcp"],
    },
    Expected {
        menu: "Settings",
        label: "Profiles",
        actions: &[],
        commands: &["profiles"],
    },
    Expected {
        menu: "Help",
        label: "Gateway health check",
        actions: &[],
        commands: &["doctor"],
    },
];

fn find<'a>(items: &'a [MenuItem], menu: &str, label: &str) -> &'a MenuItem {
    items
        .iter()
        .find(|item| item.menu == menu && item.label == label)
        .unwrap_or_else(|| panic!("missing {menu} / {label}"))
}

fn binding_is_known(item: &MenuItem, actions: &[&str], commands: &[&str]) {
    let table = LocalCommandTable::builtin();
    match item.binding {
        MenuBinding::KeyAction(name) => {
            assert!(actions.contains(&name), "{} uses {name}", item.label);
            assert!(
                Action::from_config_name(name).is_some(),
                "{name} must already exist"
            );
        }
        MenuBinding::LocalCommand(name) => {
            assert!(commands.contains(&name), "{} uses {name}", item.label);
            assert!(table.find(name).is_some(), "/{name} must already exist");
        }
        MenuBinding::VoiceRead => panic!("{} is not a voice read", item.label),
    }
}

#[test]
fn every_item_maps_to_an_existing_action() {
    let items = MenuState::shown().items();
    assert!(!items.is_empty(), "the menu lists its items");
    for item in &items {
        match item.binding {
            MenuBinding::KeyAction(name) => assert!(
                Action::from_config_name(name).is_some(),
                "{name} is not an existing key action"
            ),
            MenuBinding::LocalCommand(name) => assert!(
                LocalCommandTable::builtin().find(name).is_some(),
                "/{name} is not an existing local command"
            ),
            MenuBinding::VoiceRead => {
                assert_eq!(parse_voice_args("read"), Ok(VoiceCommand::Read(None)));
                let label = item.label.to_lowercase();
                assert!(
                    !label.contains("on") && !label.contains("recording"),
                    "{}",
                    item.label
                );
            }
        }
    }
    for expected in EXPECTED {
        let item = find(&items, expected.menu, expected.label);
        binding_is_known(item, expected.actions, expected.commands);
    }
    let read = items
        .iter()
        .find(|item| item.menu == "Voice" && item.binding == MenuBinding::VoiceRead)
        .expect("Voice read-aloud");
    let label = read.label.to_lowercase();
    assert!(
        !label.contains("on") && !label.contains("recording"),
        "{label}"
    );

    let mut on = MenuState::shown();
    on.panel_on = true;
    let on_items = on.items();
    let checked = find(&on_items, "View", "Right work panel");
    assert!(
        checked.checked,
        "the panel item is checked while the panel is on"
    );
    let off_items = MenuState::shown().items();
    let unchecked = find(&off_items, "View", "Right work panel");
    assert!(!unchecked.checked);
}

#[test]
fn disabled_items_give_a_reason() {
    let mut state = MenuState::shown();
    state.fleet_enabled = false;
    state.remote_gateway = true;
    state.dropdown_open = true;
    let items = state.items();
    assert!(
        items.iter().any(|item| !item.enabled),
        "a disabled preference disables its item"
    );
    for item in items.iter().filter(|item| !item.enabled) {
        assert!(!item.reason.trim().is_empty(), "{}", item.label);
    }
    let fleet = find(&items, "Session", "Fleet");
    assert!(!fleet.enabled);
    assert_eq!(fleet.reason, FLEET_DISABLED);
    let session: Vec<_> = items.iter().filter(|item| item.menu == "Session").collect();
    state.menu_index = 0;
    state.item_index = session
        .iter()
        .position(|item| item.label == "Fleet")
        .expect("Fleet");
    let outcome = state.on_key(MenuKey::Enter);
    assert!(outcome.activated.is_none(), "Enter runs nothing");
    let notice = outcome.notice.unwrap_or_default();
    assert!(notice.contains(FLEET_DISABLED), "{notice}");

    let record = find(&items, "Voice", "Record or stop");
    assert!(!record.enabled);
    assert_eq!(record.reason, REMOTE_VOICE);
    let read = items
        .iter()
        .find(|item| item.menu == "Voice" && item.binding == MenuBinding::VoiceRead)
        .expect("Voice read-aloud");
    assert!(!read.enabled);
    assert_eq!(read.reason, REMOTE_VOICE);
}
