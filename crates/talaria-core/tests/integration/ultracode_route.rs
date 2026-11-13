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
