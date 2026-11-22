use proptest::prelude::*;
use talaria_core::{
    session::ids::SessionId,
    ultracode::{SubmitRoute, UltracodeCommand, UltracodeState, contains_keyword, route_submit},
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
