use super::{CapKind, Change, Context, MainModelRow, McpSource, Row, RowStatus, Section};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub use crate::capabilities::CapabilityChange;
#[derive(Clone, Debug, PartialEq)]
pub enum ApplyStep {
    Settings {
        rows: Vec<(usize, String)>,
        changes: BTreeMap<String, Value>,
        baseline: Value,
    },
    MainModel {
        row: usize,
        provider: String,
        model: String,
        base_url: String,
        baseline: MainModelRow,
        confirmed: bool,
    },
    McpAdd {
        row: usize,
        name: String,
        preset: Option<String>,
        config: Option<Map<String, Value>>,
    },
    McpEnable {
        row: usize,
        name: String,
        enabled: bool,
    },
    McpRemove {
        row: usize,
        name: String,
    },
    Capability {
        row: usize,
        change: CapabilityChange,
    },
    CronAdd {
        row: usize,
        name: String,
        schedule: String,
        prompt: String,
        repeat: Option<u32>,
        deliver: String,
    },
    CronControl {
        row: usize,
        job_id: String,
        action: CronControl,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CronControl {
    Pause,
    Resume,
    Remove,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowOutcome {
    Applied,
    Adjusted(String),
    NoChange(String),
    NotApplied(String),
    Conflict(String),
    Failed(String),
    Unknown(String),
    NotAttempted,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepResult {
    Rows(Vec<(usize, RowOutcome)>),
    NeedsConfirm { row: usize, message: String },
}
pub fn plan_apply(rows: &[Row], selected: &[bool], ctx: &Context) -> Vec<ApplyStep> {
    let ready: Vec<_> = rows
        .iter()
        .enumerate()
        .filter(|(index, row)| {
            selected.get(*index) == Some(&true) && row.status == RowStatus::Ready
        })
        .collect();
    let mut result = Vec::new();
    let mut settings_rows = Vec::new();
    let mut changes = BTreeMap::new();
    for (index, row) in &ready {
        if let Change::Set { key, value, .. } = &row.change {
            settings_rows.push((*index, key.clone()));
            changes.insert(key.clone(), value.clone());
        }
    }
    if !changes.is_empty() {
        result.push(ApplyStep::Settings {
            rows: settings_rows,
            changes,
            baseline: ctx.effective.clone(),
        });
    }
    for (index, row) in &ready {
        if let Change::MainModel {
            provider, model, ..
        } = &row.change
        {
            let baseline = match &ctx.inventory.main_model {
                Section::Available(rows) => rows.first().cloned(),
                Section::Unavailable(_) => None,
            };
            let Some(baseline) = baseline else { continue };
            let base_url = match &ctx.inventory.routes {
                Section::Available(routes) => routes
                    .iter()
                    .find(|r| r.slug == *provider)
                    .map(|r| r.api_url.clone())
                    .unwrap_or_default(),
                Section::Unavailable(_) => String::new(),
            };
            result.push(ApplyStep::MainModel {
                row: *index,
                provider: provider.clone(),
                model: model.clone(),
                base_url,
                baseline,
                confirmed: false,
            });
        }
    }
    for (index, row) in &ready {
        match &row.change {
            Change::McpAdd { name, source, .. } => {
                let (preset, config) = match source {
                    McpSource::Preset(preset) => (Some(preset.clone()), None),
                    McpSource::Command { command, args } => (
                        None,
                        Some(Map::from_iter([
                            ("command".into(), Value::String(command.clone())),
                            (
                                "args".into(),
                                Value::Array(args.iter().cloned().map(Value::String).collect()),
                            ),
                        ])),
                    ),
                    McpSource::Url { url, oauth } => {
                        let mut config =
                            Map::from_iter([("url".into(), Value::String(url.clone()))]);
                        if *oauth {
                            config.insert("auth".into(), Value::String("oauth".into()));
                        }
                        (None, Some(config))
                    }
                    McpSource::Forbidden(_) => continue,
                };
                result.push(ApplyStep::McpAdd {
                    row: *index,
                    name: name.clone(),
                    preset,
                    config,
                });
            }
            Change::McpEnable { name, enabled, .. } => result.push(ApplyStep::McpEnable {
                row: *index,
                name: name.clone(),
                enabled: *enabled,
            }),
            Change::McpRemove { name, .. } => result.push(ApplyStep::McpRemove {
                row: *index,
                name: name.clone(),
            }),
            _ => {}
        }
    }
    for (index, row) in &ready {
        if let Change::Toggle {
            kind,
            name,
            enabled,
            ..
        } = &row.change
        {
            let change = match kind {
                CapKind::Skill => CapabilityChange::Skill {
                    name: name.clone(),
                    enabled: *enabled,
                },
                CapKind::Toolset => CapabilityChange::Toolset {
                    name: name.clone(),
                    enabled: *enabled,
                },
                CapKind::Plugin => CapabilityChange::Plugin {
                    key: name.clone(),
                    enabled: *enabled,
                },
            };
            result.push(ApplyStep::Capability {
                row: *index,
                change,
            });
        }
    }
    for (index, row) in ready {
        let step = match &row.change {
            Change::CronAdd {
                name,
                schedule,
                prompt,
                repeat,
                ..
            } => ApplyStep::CronAdd {
                row: index,
                name: name.clone(),
                schedule: schedule.clone(),
                prompt: prompt.clone(),
                repeat: *repeat,
                deliver: "local".into(),
            },
            Change::CronPause { job_id, .. } => ApplyStep::CronControl {
                row: index,
                job_id: job_id.clone(),
                action: CronControl::Pause,
            },
            Change::CronResume { job_id, .. } => ApplyStep::CronControl {
                row: index,
                job_id: job_id.clone(),
                action: CronControl::Resume,
            },
            Change::CronRemove { job_id, .. } => ApplyStep::CronControl {
                row: index,
                job_id: job_id.clone(),
                action: CronControl::Remove,
            },
            _ => continue,
        };
        result.push(step);
    }
    result
}
