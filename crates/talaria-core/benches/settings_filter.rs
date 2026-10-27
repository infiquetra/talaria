use std::{hint::black_box, time::Instant};
use talaria_core::settings::{
    ConfigSchemaView, EditKey, SETTINGS_LINKS, SettingsAction, SettingsInput, SettingsKey,
    SettingsSnapshot, SettingsState,
};

fn main() {
    let schema = include_str!("../../../fixtures/rest/config-schema.json");
    let read = |text: &str| serde_json::from_str(text).expect("fixture JSON");
    let snapshot = SettingsSnapshot {
        profile: "fixture".to_owned(),
        schema: ConfigSchemaView::from_json_str(schema).expect("schema"),
        effective: read(include_str!("../../../fixtures/rest/config-effective.json")),
        raw: read(include_str!("../../../fixtures/rest/config-raw.json")),
        defaults: read(include_str!("../../../fixtures/rest/config-defaults.json")),
    };
    let mut state = SettingsState::new(SETTINGS_LINKS);
    state.update(SettingsInput::Open {
        profile: "fixture".to_owned(),
        filter: None,
    });
    state.update(SettingsInput::Loaded(snapshot));
    state.update(SettingsInput::Key(SettingsKey::Action(
        SettingsAction::Filter,
    )));
    let mut samples = Vec::with_capacity(200);
    for index in 0..220 {
        let start = Instant::now();
        state.update(SettingsInput::Key(SettingsKey::Char('a')));
        black_box(state.view());
        let elapsed = start.elapsed().as_micros();
        if index >= 20 {
            samples.push(elapsed);
        }
        state.update(SettingsInput::Key(SettingsKey::Edit(EditKey::Backspace)));
    }
    samples.sort_unstable();
    let median = (samples[99] + samples[100]) / 2;
    println!("median_us={median}");
    if median > 2_000 {
        std::process::exit(1);
    }
}
