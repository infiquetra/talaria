//! The card shown after Hermes refuses a turn because a usage limit was reached.
//!
//! The card is read from the `error_surface` detail stored on the failed turn's notice. It
//! names the models Hermes suggests and offers to switch to the first of them; it is not a
//! prompt and not a need, so the failed turn stays failed.

use serde_json::{Map, Value, json};

/// What the usage-limit card shows and which model its confirm switches to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageLimitCard {
    pub title: String,
    pub lines: Vec<String>,
    /// The confirm choice, or `None` when no suggested model can be offered.
    pub confirm: Option<String>,
    pub dismiss: String,
    /// The first listed suggestion's model, or empty when there is none.
    pub model: String,
    pub provider: Option<String>,
    /// The session the card belongs to; empty until the app assigns it.
    pub session_id: String,
}

/// The card for a failed turn's `error_surface` detail, or `None` when it names no usage limit.
pub fn usage_limit_card(detail: &Value) -> Option<UsageLimitCard> {
    let limit = detail.get("usage_limit")?.as_object()?;
    let mut lines = Vec::new();
    if let Some(message) = trimmed(limit.get("message")) {
        lines.push(message.to_owned());
    }
    let mut first: Option<(String, String, Option<String>)> = None;
    for suggestion in limit
        .get("suggestions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let (Some(label), Some(model)) = (
            present(suggestion.get("label")),
            present(suggestion.get("model")),
        ) else {
            continue;
        };
        let shown = match trimmed(suggestion.get("note")) {
            Some(note) => format!("{label} ({note})"),
            None => label.to_owned(),
        };
        if first.is_none() {
            first = Some((
                format!("Switch to {shown}"),
                model.to_owned(),
                present(suggestion.get("provider")).map(str::to_owned),
            ));
        }
        lines.push(shown);
    }
    if let Some(sentence) = limit
        .get("credits")
        .and_then(|credits| credits.get("action"))
        .and_then(Value::as_str)
        .and_then(credits_sentence)
    {
        lines.push(sentence.to_owned());
    }
    let (confirm, model, provider) = match first {
        Some((confirm, model, provider)) => (Some(confirm), model, provider),
        None => (None, String::new(), None),
    };
    Some(UsageLimitCard {
        title: "Limit reached".to_owned(),
        lines,
        confirm,
        dismiss: "Dismiss".to_owned(),
        model,
        provider,
        session_id: String::new(),
    })
}

/// The `session.mutate` payload that switches to the card's model, without saving a default.
pub fn usage_limit_model_payload(card: &UsageLimitCard) -> Map<String, Value> {
    let mut payload = Map::new();
    payload.insert("model".into(), json!(card.model));
    if let Some(provider) = &card.provider {
        payload.insert("provider".into(), json!(provider));
    }
    payload
}

/// Hermes's own sentence for a credits action, as its terminal interface words it.
fn credits_sentence(action: &str) -> Option<&'static str> {
    match action {
        "add_credits" => Some("Or add credits with /topup."),
        "add_credits_and_keep_going" => Some("Or add credits and turn on Keep going in /topup."),
        "use_credits" => Some("Or turn on Keep going in /topup to use your credits."),
        _ => None,
    }
}

/// A string that is not empty after trimming, trimmed.
fn trimmed(value: Option<&Value>) -> Option<&str> {
    value?
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// A string that is not empty after trimming, exactly as received.
fn present(value: Option<&Value>) -> Option<&str> {
    value?.as_str().filter(|text| !text.trim().is_empty())
}
