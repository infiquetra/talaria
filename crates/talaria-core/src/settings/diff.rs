use super::{
    SettingsSnapshot, apply_effect, contains_secret_key, is_secret_key,
    values::{lookup, put, same_kind},
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};
pub fn build_patch(changes: &BTreeMap<String, Value>) -> Value {
    let mut patch = Value::Object(Default::default());
    for (key, value) in changes {
        put(&mut patch, key, value.clone());
    }
    patch
}
#[derive(Clone, PartialEq)]
pub enum KeyOutcome {
    Applied,
    NotApplied,
    Adjusted(Value),
    ChangedElsewhere(Value),
}
impl fmt::Debug for KeyOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Applied => "Applied",
            Self::NotApplied => "NotApplied",
            Self::Adjusted(_) => "Adjusted",
            Self::ChangedElsewhere(_) => "ChangedElsewhere",
        })
    }
}
pub fn classify_readback(
    sent: &Value,
    before: Option<&Value>,
    after: Option<&Value>,
) -> KeyOutcome {
    let after = after.unwrap_or(&Value::Null);
    if after == sent {
        KeyOutcome::Applied
    } else if after == before.unwrap_or(&Value::Null) {
        KeyOutcome::NotApplied
    } else if same_kind(after, sent) {
        KeyOutcome::Adjusted(after.clone())
    } else {
        KeyOutcome::ChangedElsewhere(after.clone())
    }
}
#[derive(Clone, PartialEq)]
pub struct Conflict {
    pub key: String,
    pub yours: Value,
    pub baseline: Option<Value>,
    pub now: Option<Value>,
}
impl fmt::Debug for Conflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Conflict")
    }
}
pub fn find_conflicts(
    changes: &BTreeMap<String, Value>,
    baseline: &Value,
    fresh: &Value,
) -> (Vec<Conflict>, Vec<String>) {
    let conflicts = changes
        .iter()
        .filter(|(key, _)| lookup(baseline, key) != lookup(fresh, key))
        .map(|(key, yours)| Conflict {
            key: key.clone(),
            yours: yours.clone(),
            baseline: lookup(baseline, key).cloned(),
            now: lookup(fresh, key).cloned(),
        })
        .collect();
    fn leaves(value: &Value, prefix: &str, out: &mut BTreeSet<String>) {
        if let Value::Object(map) = value {
            if map.is_empty() && !prefix.is_empty() {
                out.insert(prefix.into());
            }
            for (key, value) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                leaves(value, &path, out);
            }
        } else {
            out.insert(prefix.into());
        }
    }
    let mut keys = BTreeSet::new();
    leaves(baseline, "", &mut keys);
    leaves(fresh, "", &mut keys);
    let outside = keys
        .into_iter()
        .filter(|key| !changes.contains_key(key) && lookup(baseline, key) != lookup(fresh, key))
        .collect();
    (conflicts, outside)
}
#[derive(Clone, PartialEq)]
pub struct ReviewRow {
    pub key: String,
    pub old: String,
    pub new: String,
    pub effect_label: String,
    pub warnings: Vec<String>,
}
impl fmt::Debug for ReviewRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReviewRow")
            .field("warnings", &self.warnings.len())
            .finish_non_exhaustive()
    }
}
pub(crate) fn format_value(key: &str, value: Option<&Value>) -> String {
    let value = value.unwrap_or(&Value::Null);
    if value.is_null() || (is_secret_key(key) && value.as_str() == Some("")) {
        return "(not set)".into();
    }
    if is_secret_key(key) || contains_secret_key(value) {
        return "•••••••• (r reveals once)".into();
    }
    if let Value::Array(items) = value {
        return format!(
            "[{}]",
            items
                .iter()
                .map(super::values::option_text)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    value.to_string()
}
pub fn review_rows(
    snapshot: &SettingsSnapshot,
    pending: &BTreeMap<String, Value>,
) -> Vec<ReviewRow> {
    snapshot
        .schema
        .ordered_fields()
        .into_iter()
        .filter_map(|field| {
            pending.get(&field.key).map(|new| {
                let old = lookup(&snapshot.effective, &field.key);
                let secret = is_secret_key(&field.key)
                    || contains_secret_key(new)
                    || old.is_some_and(contains_secret_key);
                let mut warnings = Vec::new();
                if new.as_array().is_some_and(Vec::is_empty)
                    && old.and_then(Value::as_array).is_some_and(|v| !v.is_empty())
                {
                    warnings.push("Empties this list.".into());
                    if field.key == "toolsets" {
                        warnings.push("The agent loses every toolset.".into());
                    }
                }
                ReviewRow {
                    key: field.key.clone(),
                    old: if secret {
                        "••••".into()
                    } else {
                        format_value(&field.key, old)
                    },
                    new: if secret {
                        "•••• (changed)".into()
                    } else {
                        format_value(&field.key, Some(new))
                    },
                    effect_label: apply_effect(&field.key).0.label().into(),
                    warnings,
                }
            })
        })
        .collect()
}
