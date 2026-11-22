//! The `/effort` selector: a scale of Hermes's levels that Left and Right move along, with a
//! staged Ultracode switch and a save-for-new-sessions switch that Enter applies together.

use serde_json::json;
use talaria_core::models::{
    Effort, EffortScale, ModelsInput, ModelsState, SavedModels, ScaleEffect, ScaleFacts, ScaleKey,
};

fn facts(current: Option<Effort>) -> ScaleFacts {
    ScaleFacts {
        model: Some("model-a".into()),
        current,
        wire: None,
        offered: Some(true),
        can_disable: None,
        ultracode: false,
        profile: "work".into(),
    }
}

fn scale(current: Option<Effort>) -> EffortScale {
    EffortScale::new(facts(current))
}

fn press(scale: &mut EffortScale, keys: &[ScaleKey]) -> ScaleEffect {
    let mut last = ScaleEffect::Redraw;
    for key in keys {
        last = scale.update(*key);
    }
    last
}

#[test]
fn effort_all_matches_the_hermes_ladder() {
    let wire: Vec<_> = Effort::ALL.iter().map(|level| level.as_wire()).collect();
    assert_eq!(
        wire,
        [
            "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"
        ]
    );
    for level in Effort::ALL {
        assert!(!level.describe().is_empty(), "{level:?}");
    }
}

#[test]
fn scale_opens_on_the_current_level_or_on_medium_when_unread() {
    assert_eq!(scale(Some(Effort::High)).selected(), Effort::High);
    // Hermes reads an unset level as medium.
    assert_eq!(scale(None).selected(), Effort::Medium);
    assert_eq!(scale(Some(Effort::High)).levels(), Effort::ALL.to_vec());
}

#[test]
fn arrows_step_and_stop_at_the_ends() {
    let mut low = scale(Some(Effort::Low));
    press(&mut low, &[ScaleKey::Lower; 5]);
    assert_eq!(low.selected(), Effort::None);
    let mut max = scale(Some(Effort::Max));
    press(&mut max, &[ScaleKey::Higher; 3]);
    assert_eq!(max.selected(), Effort::Ultra);
    press(&mut max, &[ScaleKey::Lowest]);
    assert_eq!(max.selected(), Effort::None);
    press(&mut max, &[ScaleKey::Highest]);
    assert_eq!(max.selected(), Effort::Ultra);
}

#[test]
fn none_is_hidden_when_reasoning_cannot_be_disabled() {
    let mut mandatory = facts(Some(Effort::Low));
    mandatory.can_disable = Some(false);
    let mut scale = EffortScale::new(mandatory.clone());
    assert_eq!(scale.levels().len(), 7);
    assert_eq!(scale.levels()[0], Effort::Minimal);
    press(&mut scale, &[ScaleKey::Lowest]);
    assert_eq!(scale.selected(), Effort::Minimal);
    // A session already at none keeps it on the scale, so the marker shows the truth.
    mandatory.current = Some(Effort::None);
    assert_eq!(EffortScale::new(mandatory).levels()[0], Effort::None);
}

#[test]
fn enter_without_a_change_applies_nothing() {
    let mut scale = scale(Some(Effort::High));
    assert_eq!(press(&mut scale, &[ScaleKey::Apply]), ScaleEffect::Cancel);
    // Moving away and back is no change either.
    let mut scale = self::scale(Some(Effort::High));
    let effect = press(
        &mut scale,
        &[ScaleKey::Higher, ScaleKey::Lower, ScaleKey::Apply],
    );
    assert_eq!(effect, ScaleEffect::Cancel);
    // An unread level that was never moved is not sent as medium.
    let mut unread = self::scale(None);
    assert_eq!(press(&mut unread, &[ScaleKey::Apply]), ScaleEffect::Cancel);
}

#[test]
fn enter_after_a_move_sets_the_level_for_this_session() {
    let mut scale = scale(Some(Effort::High));
    let effect = press(&mut scale, &[ScaleKey::Higher, ScaleKey::Apply]);
    assert_eq!(
        effect,
        ScaleEffect::Apply {
            level: Some(Effort::Xhigh),
            save_default: false,
            ultracode: None,
        }
    );
}

#[test]
fn the_save_switch_saves_the_level_for_new_sessions() {
    let mut scale = scale(Some(Effort::High));
    assert_eq!(scale.save_line(), "Also save for new sessions on work: off");
    press(&mut scale, &[ScaleKey::ToggleSave]);
    assert!(scale.save_default());
    assert_eq!(scale.save_line(), "Also save for new sessions on work: on");
    let effect = press(&mut scale, &[ScaleKey::Apply]);
    assert_eq!(
        effect,
        ScaleEffect::Apply {
            level: Some(Effort::High),
            save_default: true,
            ultracode: None,
        }
    );
}

#[test]
fn tab_stages_ultracode_and_enter_applies_it() {
    let mut scale = scale(Some(Effort::High));
    press(&mut scale, &[ScaleKey::ToggleUltracode]);
    assert!(scale.ultracode());
    assert!(scale.ultracode_changed());
    assert_eq!(
        press(&mut scale.clone(), &[ScaleKey::Apply]),
        ScaleEffect::Apply {
            level: None,
            save_default: false,
            ultracode: Some(true),
        }
    );
    press(&mut scale, &[ScaleKey::ToggleUltracode]);
    assert!(!scale.ultracode_changed());
    assert_eq!(press(&mut scale, &[ScaleKey::Apply]), ScaleEffect::Cancel);
    // Effort and the switch together.
    let mut both = self::scale(Some(Effort::High));
    let effect = press(
        &mut both,
        &[ScaleKey::Lower, ScaleKey::ToggleUltracode, ScaleKey::Apply],
    );
    assert_eq!(
        effect,
        ScaleEffect::Apply {
            level: Some(Effort::Medium),
            save_default: false,
            ultracode: Some(true),
        }
    );
}

#[test]
fn escape_cancels_whatever_was_staged() {
    let mut scale = scale(Some(Effort::High));
    let effect = press(
        &mut scale,
        &[
            ScaleKey::Higher,
            ScaleKey::ToggleUltracode,
            ScaleKey::ToggleSave,
            ScaleKey::Cancel,
        ],
    );
    assert_eq!(effect, ScaleEffect::Cancel);
}

#[test]
fn a_late_read_moves_an_untouched_marker_only() {
    let mut scale = scale(None);
    scale.set_current(Some(Effort::Max), None);
    assert_eq!(scale.selected(), Effort::Max);
    press(&mut scale, &[ScaleKey::Lower]);
    scale.set_current(Some(Effort::Low), None);
    assert_eq!(scale.selected(), Effort::Xhigh, "a moved marker stays");
    assert_eq!(scale.current(), Some(Effort::Low));
}

#[test]
fn the_detail_line_describes_the_selected_level() {
    let mut scale = scale(Some(Effort::High));
    assert_eq!(scale.detail(), "More thinking for harder tasks  (current)");
    press(&mut scale, &[ScaleKey::Higher]);
    assert_eq!(
        scale.detail(),
        "Extended thinking for complex work  (current: high)"
    );
    assert_eq!(
        self::scale(None).detail(),
        Effort::Medium.describe(),
        "no current mark when the level could not be read"
    );
}

#[test]
fn title_and_notes() {
    assert_eq!(scale(Some(Effort::High)).title(), "Effort · model-a");
    let mut unnamed = facts(Some(Effort::High));
    unnamed.model = None;
    assert_eq!(EffortScale::new(unnamed).title(), "Effort");

    let mut not_offered = facts(Some(Effort::High));
    not_offered.offered = Some(false);
    assert_eq!(
        EffortScale::new(not_offered).note().as_deref(),
        Some("model-a does not offer effort levels, so a level set here has no effect on it.")
    );
    assert_eq!(
        scale(None).note().as_deref(),
        Some("effort: could not be read")
    );
    let mut clamped = facts(Some(Effort::Max));
    clamped.wire = Some("medium".into());
    assert_eq!(
        EffortScale::new(clamped).note().as_deref(),
        Some("Hermes sends medium on this route.")
    );
    let mut same = facts(Some(Effort::Max));
    same.wire = Some("max".into());
    assert_eq!(EffortScale::new(same).note(), None);
    assert_eq!(scale(Some(Effort::High)).note(), None);
}

#[test]
fn caps_arriving_later_hide_none() {
    let mut scale = scale(Some(Effort::High));
    scale.set_caps(Some(true), Some(false));
    assert_eq!(scale.levels()[0], Effort::Minimal);
}

#[test]
fn the_catalogue_says_whether_reasoning_can_be_disabled() {
    let mut state = ModelsState::new("L", Ok(SavedModels::empty()));
    let result = serde_json::from_value(json!({"model":"m", "provider":"p", "providers":[
        {"slug":"p", "name":"P", "models":["m", "n", "o"], "authenticated":true, "capabilities":{
            "m":{"fast":false, "reasoning":true, "can_disable_reasoning":false},
            "n":{"fast":false, "reasoning":true, "can_disable_reasoning":true},
            "o":{"fast":false, "reasoning":true}
        }}
    ]}))
    .unwrap();
    state.update(ModelsInput::OptionsLoaded {
        session_id: "s".into(),
        result,
    });
    assert_eq!(state.can_disable_reasoning("s", "p", "m"), Some(false));
    assert_eq!(state.can_disable_reasoning("s", "p", "n"), Some(true));
    assert_eq!(state.can_disable_reasoning("s", "p", "o"), None);
    assert_eq!(state.can_disable_reasoning("s", "p", "missing"), None);
    assert_eq!(state.can_disable_reasoning("t", "p", "m"), None);
}
