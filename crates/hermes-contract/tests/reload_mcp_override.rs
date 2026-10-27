use hermes_contract::{
    native::NativeMethod,
    overrides::reload_mcp::{Marker, Params},
};
use serde_json::json;

#[test]
fn params_never_carry_always_or_rev() {
    fn needs_native<M: NativeMethod>() {}
    needs_native::<Marker>();
    for (session_id, confirm, expected) in [
        (Some("s"), None, json!({"session_id":"s"})),
        (
            Some("s"),
            Some(true),
            json!({"session_id":"s","confirm":true}),
        ),
        (None, Some(true), json!({"confirm":true})),
        (None, None, json!({})),
    ] {
        let params = Params {
            session_id: session_id.map(str::to_string),
            confirm,
        };
        assert_eq!(serde_json::to_value(params).unwrap(), expected);
    }
}
