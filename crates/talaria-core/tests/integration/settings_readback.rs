use serde_json::{Value, json};
use talaria_core::settings::{KeyOutcome, classify_readback};
#[test]
fn classify_each_key() {
    let sent = json!(true);
    let before = json!(false);
    assert_eq!(
        classify_readback(&sent, Some(&before), Some(&sent)),
        KeyOutcome::Applied
    );
    assert_eq!(
        classify_readback(&sent, Some(&before), Some(&before)),
        KeyOutcome::NotApplied
    );
    assert_eq!(
        classify_readback(
            &json!("sent"),
            Some(&json!("before")),
            Some(&json!("adjusted"))
        ),
        KeyOutcome::Adjusted(json!("adjusted"))
    );
    assert_eq!(
        classify_readback(&sent, Some(&before), Some(&json!("other"))),
        KeyOutcome::ChangedElsewhere(json!("other"))
    );
    assert_eq!(
        classify_readback(&sent, Some(&before), None),
        KeyOutcome::ChangedElsewhere(Value::Null)
    );
    assert_eq!(
        classify_readback(&Value::Null, Some(&before), None),
        KeyOutcome::Applied
    );
    assert_eq!(classify_readback(&sent, None, None), KeyOutcome::NotApplied);
    assert_eq!(
        classify_readback(&json!(1), Some(&json!(0)), Some(&json!(1.5))),
        KeyOutcome::Adjusted(json!(1.5))
    );
}
