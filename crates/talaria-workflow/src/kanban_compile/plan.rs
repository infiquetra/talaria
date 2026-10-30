//! Versioned identity, result contracts, and dependency planning.
use super::{CapturedCall, capture::sentinel};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallLine {
    pub digest: String,
    pub occurrence: u32,
    pub assignee: String,
    pub tenant: String,
    pub model: String,
    pub provider: String,
    pub effort: String,
    pub workspace_kind: String,
    pub workspace_path: String,
    pub workflow_path: String,
}
pub(super) fn canonical(v: &Value) -> String {
    match v {
        Value::Object(map) => format!(
            "{{{}}}",
            map.iter()
                .map(|(k, v)| format!("{}:{}", Value::String(k.clone()), canonical(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        _ => v.to_string(),
    }
}
fn digest(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}
pub fn compile_id(
    script_source: &str,
    args: &Value,
    calls: &[CallLine],
    children: &[&str],
) -> String {
    let mut lines = vec![
        "kanban-compile/1".into(),
        digest(script_source),
        canonical(args),
    ];
    for q in calls {
        lines.push(format!(
            "{}:{}|{}|{}|{}/{}@{}|{}:{}|{}",
            q.digest,
            q.occurrence,
            q.assignee,
            q.tenant,
            q.model,
            q.provider,
            q.effort.to_lowercase(),
            q.workspace_kind,
            q.workspace_path,
            q.workflow_path
        ));
    }
    for source in children {
        lines.push(digest(source));
    }
    digest(&lines.join("\n"))[..16].into()
}
pub fn card_body(
    script_name: &str,
    compile_id: &str,
    label: &str,
    phase: &str,
    prompt: &str,
    schema: Option<&Value>,
) -> String {
    let contract = match schema {
        Some(schema) => format!(
            "Put the result JSON in metadata key `result_json`, matching this schema:\n```json\n{}\n```\n",
            canonical(schema)
        ),
        None => "The summary is the result.\n".into(),
    };
    let artifact = if schema.is_some() {
        "If your result JSON would exceed 3,500 UTF-8 bytes, write it to a file, declare that file as a completion artifact, and put {\"result_artifact\": \"<file name>\"} in the completion metadata instead of `result_json`. Downstream cards: when a parent's metadata carries `result_artifact`, read that parent's declared artifact (the kanban show tool lists its attachments) instead of expecting `result_json`.\n"
    } else {
        ""
    };
    format!(
        "## Result contract\nFinish with the kanban completion handoff: a 1-3 sentence summary plus metadata.\n{contract}Declare every deliverable file as a completion artifact; your workspace may be deleted when this card completes.\n{artifact}If a required parent result is missing from the handoffs (for example a parent was archived), stop: add a comment naming the missing input and block this card. Never invent a value.\n\n## Workflow\nCompiled by Talaria from `{script_name}` (compile `{compile_id}`, call `{label}`, phase `{phase}`).\n\n## Prompt\n{prompt}"
    )
}
pub(super) fn markers(call: &CapturedCall) -> [String; 2] {
    [
        sentinel(&call.call_id),
        (900_000_000_u64 + u64::from(call.call_id.occurrence)).to_string(),
    ]
}
fn boundary(c: Option<char>) -> bool {
    c.is_none_or(|c| !c.is_alphanumeric() && c != '_')
}
pub(super) fn replace_marker(text: &str, marker: &str, replacement: &str) -> String {
    let mut result = String::new();
    let mut start = 0;
    for (at, _) in text.match_indices(marker) {
        let end = at + marker.len();
        let numeric = marker.as_bytes()[0].is_ascii_digit();
        if !numeric
            || boundary(text[..at].chars().next_back()) && boundary(text[end..].chars().next())
        {
            result.push_str(&text[start..at]);
            result.push_str(replacement);
            start = end;
        }
    }
    result.push_str(&text[start..]);
    result
}
pub(super) fn parents(calls: &[CapturedCall]) -> Vec<Vec<usize>> {
    let mut batches: BTreeMap<Vec<String>, (Vec<usize>, Vec<usize>)> = BTreeMap::new();
    let mut result = Vec::new();
    for (i, call) in calls.iter().enumerate() {
        let (previous, current) = batches.entry(call.workflow_path.clone()).or_default();
        if !call.outstanding && !current.is_empty() {
            *previous = std::mem::take(current);
        }
        let mut edges: BTreeSet<_> = previous.iter().copied().collect();
        for (j, parent) in calls[..i].iter().enumerate() {
            if markers(parent)
                .iter()
                .any(|marker| replace_marker(&call.prompt, marker, "") != call.prompt)
            {
                edges.insert(j);
            }
        }
        current.push(i);
        result.push(edges.into_iter().collect());
    }
    result
}
pub(super) fn title(call: &CapturedCall, previous: &[CapturedCall]) -> String {
    let default = call
        .prompt
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
        .chars()
        .take(60)
        .collect::<String>();
    let base = if !call.label.is_empty() && call.label != default {
        call.label.clone()
    } else {
        let mut prompt = call.prompt.clone();
        while let Some(start) = prompt.find("⟨wf:") {
            let Some(end) = prompt[start..].find('⟩') else {
                break;
            };
            prompt.replace_range(start..start + end + '⟩'.len_utf8(), "<result>");
        }
        for parent in previous {
            for marker in markers(parent) {
                prompt = replace_marker(&prompt, &marker, "<result>");
            }
        }
        prompt.chars().take(80).collect()
    };
    if call.workflow_path.is_empty() {
        base
    } else {
        let path = call
            .workflow_path
            .iter()
            .map(|part| {
                part.strip_prefix("name:")
                    .or_else(|| part.strip_prefix("path:"))
                    .unwrap_or(part)
                    .split('@')
                    .next()
                    .unwrap_or(part)
            })
            .collect::<Vec<_>>()
            .join("/");
        format!("{path}/{base}")
    }
}
