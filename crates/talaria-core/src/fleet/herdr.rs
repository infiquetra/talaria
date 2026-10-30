//! Pure mapping of live terminal agents into fleet observations.

use super::{ExternalNeed, ExternalSession, FleetBackend, NeedSlot, Outcome, RowKey, RowState};
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

pub fn project(pane: &HerdrPane, visible: &str, at: f64) -> HerdrProjection {
    let clean = |value: &str| crate::text::sanitize(value).into_owned();
    let pane_id = clean(&pane.pane_id);
    let workspace = clean(&pane.workspace_id);
    let tab = clean(&pane.tab_id);
    let agent = clean(&pane.agent);
    let title = if pane.title.is_empty() {
        clean(&pane.name)
    } else {
        clean(&pane.title)
    };
    let visible = sanitize_visible(visible);
    let (state, outcome) = match pane.status.as_str() {
        "working" => (RowState::Working, None),
        "idle" => (RowState::Idle, None),
        "done" => (RowState::Done, Some(Outcome::Completed)),
        "blocked" => (RowState::NeedsYou, None),
        _ => (RowState::Idle, Some(Outcome::Unknown)),
    };
    let needs = if state == RowState::NeedsYou {
        vec![ExternalNeed {
            slot: NeedSlot::External(pane_id.clone()),
            text: if visible.is_empty() {
                "Terminal agent is awaiting input".to_owned()
            } else {
                visible.clone()
            },
            options: Vec::new(),
        }]
    } else {
        Vec::new()
    };
    HerdrProjection {
        session: ExternalSession {
            key: RowKey {
                profile: format!("herdr:{pane_id}"),
                session_id: pane_id.clone(),
            },
            backend: FleetBackend::Herdr,
            agent,
            title,
            model: None,
            activity: None,
            state,
            outcome,
            used: None,
            size: None,
            cost: None,
            needs,
            at,
        },
        workspace,
        tab,
        pane: pane_id,
        visible,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HerdrProjection {
    pub session: ExternalSession,
    pub workspace: String,
    pub tab: String,
    pub pane: String,
    pub visible: String,
}

impl super::FleetRegistry {
    pub fn observe_herdr(&mut self, projection: HerdrProjection) -> Vec<super::FleetEffect> {
        super::update(self, super::FleetInput::Herdr(projection))
    }
}
