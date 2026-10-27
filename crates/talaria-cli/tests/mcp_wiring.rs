use proptest::prelude::*;
use talaria_cli::mcp_wiring::McpWiring;
use talaria_core::mcp::{McpEffect, McpTab, McpView, SecretSlot};

fn focused(slot: SecretSlot) -> McpView {
    McpView {
        title: String::new(),
        tab: McpTab::Servers,
        banner: None,
        filter: String::new(),
        placeholder: None,
        rows: vec![],
        selected: None,
        status_line: None,
        card: None,
        pending_line: None,
        notice: None,
        footer: String::new(),
        secret_focus: Some(slot),
    }
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, .. ProptestConfig::default() })]
    #[test]
    fn secrets_reach_only_their_effect(value in "[a-zA-Z0-9]{1,30}") {
        let cases = [
            (SecretSlot::Token, McpEffect::AddServer { name: "example".into(), preset: None, config: None, token: true }),
            (SecretSlot::EnvValue(0), McpEffect::SetKey { name: "example".into(), env_var: Some("EXAMPLE".into()), slot: SecretSlot::EnvValue(0) }),
            (SecretSlot::KeyValue, McpEffect::SetKey { name: "example".into(), env_var: None, slot: SecretSlot::KeyValue }),
            (SecretSlot::Paste, McpEffect::RelayCallback { name: "example".into(), flow: "flow-example".into() }),
        ];
        for (slot, effect) in cases {
            let mut wiring = McpWiring::default();
            wiring.route_paste(&focused(slot), &value);
            prop_assert_eq!(wiring.secret_len(slot), value.len());
            let secret = wiring.secret_for(&effect);
            prop_assert_eq!(secret.as_deref().map(String::as_str), Some(value.as_str()));
            prop_assert!(wiring.secret_for(&McpEffect::LoadList).is_none());
            wiring.forget();
            prop_assert_eq!(wiring.secret_len(slot), 0);
        }
    }
}
