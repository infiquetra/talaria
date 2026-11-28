//! The usage-limit card read out of the `error_surface` a failed turn stores on its notice.
//!
//! Each case builds a `serde_json::Value` shaped like the generated `UsageLimit` type and starts
//! no session.

use serde_json::{Value, json};
use talaria_core::session::{UsageLimitCard, usage_limit_card};

/// An `error_surface` object whose `usage_limit` is the sample: a message, a small model with no
/// provider, a medium model on `ollama-cloud`, and the `add_credits` action.
fn sample() -> Value {
    surface(json!({
        "credits": {"action": "add_credits"},
        "current_model": "big-1",
        "gauge_id": "daily",
        "limit_name": "Daily",
        "message": "Daily limit reached",
        "resets_at_ms": 1_800_000_000_000_i64,
        "scope": "account",
        "suggestions": [
            {"label": "Small", "note": "included", "model": "small-1"},
            {"label": "Medium", "note": "slower", "model": "med-1", "provider": "ollama-cloud"}
        ]
    }))
}

fn surface(usage_limit: Value) -> Value {
    json!({
        "code": "usage_limit",
        "layer": "provider",
        "retryable": false,
        "usage_limit": usage_limit
    })
}

fn limit_card(usage_limit: Value) -> UsageLimitCard {
    usage_limit_card(&surface(usage_limit)).expect("a usage limit makes a card")
}

#[test]
fn card_lists_every_suggestion_and_confirms_the_first() {
    let card = usage_limit_card(&sample()).expect("the sample makes a card");
    assert_eq!(card.title, "Limit reached");
    assert_eq!(
        card.lines,
        vec![
            "Daily limit reached".to_owned(),
            "Small (included)".to_owned(),
            "Medium (slower)".to_owned(),
            "Or add credits with /topup.".to_owned(),
        ]
    );
    assert_eq!(card.confirm.as_deref(), Some("Switch to Small (included)"));
    assert_eq!(card.dismiss, "Dismiss");
    assert_eq!(card.model, "small-1");
    assert_eq!(card.provider, None);
    assert_eq!(card.session_id, "");

    // No usage limit, or one that is not an object, makes no card.
    assert_eq!(
        usage_limit_card(&json!({"code": "refused", "layer": "gateway", "retryable": false})),
        None
    );
    assert_eq!(usage_limit_card(&json!({"usage_limit": "daily"})), None);
    assert_eq!(usage_limit_card(&json!({"usage_limit": null})), None);
    assert_eq!(usage_limit_card(&json!(null)), None);

    // Empty suggestions: the card is returned with dismiss only.
    let bare = limit_card(json!({
        "gauge_id": "daily",
        "limit_name": "Daily",
        "message": "Daily limit reached",
        "scope": "account",
        "suggestions": []
    }));
    assert_eq!(bare.confirm, None);
    assert_eq!(bare.dismiss, "Dismiss");
    assert_eq!(bare.model, "");
    assert_eq!(bare.provider, None);
    assert_eq!(bare.lines, vec!["Daily limit reached".to_owned()]);

    // A missing suggestions key behaves the same way.
    let missing = limit_card(json!({"message": "Daily limit reached"}));
    assert_eq!(missing.confirm, None);
    assert_eq!(missing.model, "");

    // A blank or missing message adds no line, and the card is still returned.
    let blank = limit_card(json!({
        "message": "   ",
        "suggestions": [{"label": "Small", "note": "", "model": "small-1"}]
    }));
    assert_eq!(blank.lines, vec!["Small".to_owned()]);
    assert_eq!(blank.confirm.as_deref(), Some("Switch to Small"));
    let nameless = limit_card(json!({"suggestions": [{"label": "Small", "model": "small-1"}]}));
    assert_eq!(nameless.lines, vec!["Small".to_owned()]);

    // Message and note are trimmed; label and model are kept exactly as received.
    let trimmed = limit_card(json!({
        "message": "  Daily limit reached \n",
        "suggestions": [{"label": " Small ", "note": "  included  ", "model": " small-1 "}]
    }));
    assert_eq!(trimmed.lines[0], "Daily limit reached");
    assert_eq!(trimmed.lines[1], " Small  (included)");
    assert_eq!(
        trimmed.confirm.as_deref(),
        Some("Switch to  Small  (included)")
    );
    assert_eq!(trimmed.model, " small-1 ");

    // A non-string note is empty.
    let numeric_note = limit_card(json!({
        "suggestions": [{"label": "Small", "note": 7, "model": "small-1"}]
    }));
    assert_eq!(numeric_note.lines, vec!["Small".to_owned()]);
    assert_eq!(numeric_note.confirm.as_deref(), Some("Switch to Small"));

    // A suggestion is listed only when its label and its model are strings that are not empty
    // after trimming; confirm switches to the first one that is listed.
    let skipped = limit_card(json!({
        "suggestions": [
            {"label": "", "note": "a", "model": "skip-1"},
            {"label": "No model", "note": "b", "model": "  "},
            {"label": "Numeric", "note": "c", "model": 3},
            {"label": 4, "note": "d", "model": "skip-2"},
            {"label": "Kept", "note": "e", "model": "kept-1", "provider": "nous"}
        ]
    }));
    assert_eq!(skipped.lines, vec!["Kept (e)".to_owned()]);
    assert_eq!(skipped.confirm.as_deref(), Some("Switch to Kept (e)"));
    assert_eq!(skipped.model, "kept-1");
    assert_eq!(skipped.provider.as_deref(), Some("nous"));

    // When every suggestion is skipped there is no confirm.
    let none_left = limit_card(json!({
        "message": "Daily limit reached",
        "suggestions": [{"label": " ", "note": "a", "model": "skip-1"}]
    }));
    assert_eq!(none_left.confirm, None);
    assert_eq!(none_left.model, "");
    assert_eq!(none_left.provider, None);
    assert_eq!(none_left.lines, vec!["Daily limit reached".to_owned()]);

    // A provider that is missing, not a string, or blank stays `None`; no provider is invented.
    for provider in [json!(null), json!(5), json!(""), json!("  ")] {
        let blank_provider = limit_card(json!({
            "suggestions": [{"label": "Small", "note": "x", "model": "small-1", "provider": provider}]
        }));
        assert_eq!(blank_provider.provider, None, "provider {provider}");
    }
    let spaced = limit_card(json!({
        "suggestions": [{"label": "Small", "note": "x", "model": "small-1", "provider": " nous "}]
    }));
    assert_eq!(spaced.provider.as_deref(), Some(" nous "));
}

#[test]
fn other_credit_actions_use_the_hermes_sentences() {
    let with_action = |action: Value| {
        limit_card(json!({
            "message": "Daily limit reached",
            "credits": {"action": action},
            "suggestions": [{"label": "Small", "note": "included", "model": "small-1"}]
        }))
        .lines
    };
    let base = vec![
        "Daily limit reached".to_owned(),
        "Small (included)".to_owned(),
    ];
    let with_sentence = |sentence: &str| {
        let mut lines = base.clone();
        lines.push(sentence.to_owned());
        lines
    };

    assert_eq!(
        with_action(json!("add_credits")),
        with_sentence("Or add credits with /topup.")
    );
    assert_eq!(
        with_action(json!("add_credits_and_keep_going")),
        with_sentence("Or add credits and turn on Keep going in /topup.")
    );
    assert_eq!(
        with_action(json!("use_credits")),
        with_sentence("Or turn on Keep going in /topup to use your credits.")
    );
    assert_eq!(with_action(json!("other")), base);
    assert_eq!(with_action(json!(null)), base);

    // Missing `credits` adds no line.
    let no_credits = limit_card(json!({
        "message": "Daily limit reached",
        "suggestions": [{"label": "Small", "note": "included", "model": "small-1"}]
    }));
    assert_eq!(no_credits.lines, base);

    // The credits sentence is text only: the card still confirms the first suggestion.
    let credits_only = limit_card(json!({
        "message": "Daily limit reached",
        "credits": {"action": "use_credits"}
    }));
    assert_eq!(credits_only.confirm, None);
    assert_eq!(
        credits_only.lines,
        vec![
            "Daily limit reached".to_owned(),
            "Or turn on Keep going in /topup to use your credits.".to_owned()
        ]
    );
}
