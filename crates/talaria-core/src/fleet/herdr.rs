//! Pure mapping of live terminal agents into fleet observations.

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HerdrPane {
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub agent: String,
    pub name: String,
    pub status: String,
    pub title: String,
}

fn required<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("missing or invalid agent field: {field}"))
}

fn optional(value: &Value, field: &str) -> Result<String, String> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(text)) => Ok(text.clone()),
        Some(_) => Err(format!("invalid agent field: {field}")),
    }
}

fn identifier<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    let text = required(value, field)?;
    if text.starts_with('-')
        || !text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '_' | '-' | '.'))
    {
        return Err(format!("invalid agent identifier: {field}"));
    }
    Ok(text)
}

fn decode(value: &Value) -> Result<HerdrPane, String> {
    Ok(HerdrPane {
        pane_id: identifier(value, "pane_id")?.to_owned(),
        workspace_id: identifier(value, "workspace_id")?.to_owned(),
        tab_id: identifier(value, "tab_id")?.to_owned(),
        agent: optional(value, "agent")?,
        name: optional(value, "name")?,
        status: optional(value, "agent_status")?,
        title: optional(value, "terminal_title_stripped")?,
    })
}

pub fn decode_list(value: &Value) -> Result<Vec<HerdrPane>, String> {
    let agents = value
        .pointer("/result/agents")
        .and_then(Value::as_array)
        .ok_or_else(|| "invalid agent list response".to_owned())?;
    agents.iter().map(decode).collect()
}

pub fn decode_agent(value: &Value) -> Result<HerdrPane, String> {
    let agent = value
        .pointer("/result/agent")
        .ok_or_else(|| "invalid agent response".to_owned())?;
    decode(agent)
}

/// Sanitize each line while retaining line boundaries for the peek panel.
pub fn sanitize_visible(text: &str) -> String {
    text.split('\n')
        .map(|line| crate::text::sanitize(line).into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}
