#[path = "support/configure_ctx.rs"]
mod configure_ctx;
#[path = "support/events.rs"]
mod events;
#[path = "support/snapshot_json.rs"]
mod snapshot_json;
use configure_ctx::{context, detect};
use proptest::prelude::*;
use serde_json::json;
use talaria_core::configure::*;

fn forbidden_display(c: char) -> bool {
    (c.is_control() && c != '\n' && c != '\t')
        || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

proptest! {
    #[test]
    fn parser_never_panics_and_view_is_sanitised(text in ".{0,2000}") {
        let _ = parse_reply(&text);
        let state = ConfigureState::from_reply(context(), &text, detect);
        for line in state.view(std::time::Instant::now()).text_lines() { prop_assert!(!line.chars().any(forbidden_display)); }
    }

    #[test]
    fn no_secret_survives_into_view(prefix in ".{0,80}", suffix in ".{0,80}") {
        for token in ["sk-or-v1-talaria-sentinel-0000000000000000", "ghp_talariaSentinel000000000000000000000000"] {
            for reply in [json!({"kind":"answer","text":token}), json!({"kind":"cannot","reason":token}), json!({"kind":"proposal","summary":token,"changes":[{"op":"set","key":"display.compact","value":token,"why":token}]})] {
                let text = format!("{prefix}\n```talaria-configure\n{reply}\n```\n{suffix}");
                let state = ConfigureState::from_reply(context(), &text, detect);
                for line in state.view(std::time::Instant::now()).text_lines() { prop_assert!(!line.contains(token)); }
                prop_assert!(!format!("{:?}", state).contains(token), "state debug exposed a secret");
                if let Ok(Reply::Proposal(proposal)) = parse_reply(&text) {
                    if let Ok(rows) = validate(&proposal, &context(), detect) {
                        prop_assert!(!format!("{:?}", rows).contains(token), "row debug exposed a secret");
                    }
                }
            }
        }
    }
}

#[test]
fn secret_operator_text_never_becomes_an_outbound_effect() {
    let now = std::time::Instant::now();
    let token = "sk-or-v1-talaria-sentinel-0000000000000000";
    let reply = "```talaria-configure\n{\"kind\":\"proposal\",\"summary\":\"change\",\"changes\":[{\"op\":\"set\",\"key\":\"display.compact\",\"value\":true}]}\n```";
    let mut state = ConfigureState::from_reply(context(), reply, detect);
    state.update(ConfigureInput::Key(ConfigureKey::Char('r')), now);
    state.update(ConfigureInput::Key(ConfigureKey::Paste(token.into())), now);
    let effects = state.update(ConfigureInput::Key(ConfigureKey::Enter), now);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ConfigureEffect::ClearEditor))
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, ConfigureEffect::Submit { .. }))
    );
    assert!(!format!("{effects:?} {:?} {state:?}", state.view(now).text_lines()).contains(token));

    let mut state = ConfigureState::new(detect);
    state.update(
        ConfigureInput::Open {
            request: "hello".into(),
            profile: "fixture".into(),
            connected: true,
            model: None,
        },
        now,
    );
    state.update(ConfigureInput::Gathered(Box::new(context())), now);
    let mut value = snapshot_json::base(events::SESSION);
    value["info"] = json!({"desktop_protocol":"1","tools":{"clarify":["clarify"]},"yolo":false});
    let effects = state.update(
        ConfigureInput::Created(Box::new(snapshot_json::snapshot(value))),
        now,
    );
    assert!(!format!("{effects:?}").contains(token));
    state.update(
        ConfigureInput::Session(events::ev(
            1,
            "clarify.request",
            events::clarify_request("prompt-1", 1, "question"),
        )),
        now,
    );
    let effects = state.update(
        ConfigureInput::ClarifyAnswered {
            key: events::key("prompt-1", 1),
            answer: token.into(),
        },
        now,
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ConfigureEffect::ClearEditor))
    );
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        ConfigureEffect::RespondClarify { .. } | ConfigureEffect::Submit { .. }
    )));
    assert!(!format!("{effects:?} {:?} {state:?}", state.view(now).text_lines()).contains(token));
}

#[test]
fn split_secret_is_never_shown_before_a_complete_line() {
    let now = std::time::Instant::now();
    let mut state = ConfigureState::new(detect);
    state.update(
        ConfigureInput::Open {
            request: "hello".into(),
            profile: "fixture".into(),
            connected: true,
            model: None,
        },
        now,
    );
    state.update(ConfigureInput::Gathered(Box::new(context())), now);
    let mut value = snapshot_json::base(events::SESSION);
    value["info"] = json!({"desktop_protocol":"1","tools":{"clarify":["clarify"]},"yolo":false});
    state.update(
        ConfigureInput::Created(Box::new(snapshot_json::snapshot(value))),
        now,
    );
    for (seq, part) in [(1, "sk-or-v1-talaria"), (2, "-sentinel-0000000000000000")] {
        let effects = state.update(
            ConfigureInput::Session(events::ev(seq, "message.delta", json!({"text":part}))),
            now,
        );
        let visible = format!("{:?} {:?}", state.view(now).text_lines(), effects);
        assert!(!visible.contains("sk-or-v1-talaria"));
    }
    state.update(
        ConfigureInput::Session(events::ev(3, "message.delta", json!({"text":"\n"}))),
        now,
    );
    let visible = state.view(now).text_lines().join("\n");
    assert!(visible.contains(WITHHELD));
    assert!(!visible.contains("sk-or-v1-talaria-sentinel-0000000000000000"));
    let effects = state.update(
        ConfigureInput::SubmitFailed("ghp_talariaSentinel000000000000000000000000".into()),
        now,
    );
    let exposed = format!("{:?} {:?}", state.view(now).text_lines(), effects);
    assert!(!exposed.contains("ghp_talariaSentinel000000000000000000000000"));
}
