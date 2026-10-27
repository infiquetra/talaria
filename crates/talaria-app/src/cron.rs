//! Scheduled job data conversion at the gateway boundary.

use std::time::Duration;

use hermes_client::{
    CallError, CallOptions, Client, cron_api,
    rest::{RestClient, RestError},
    scan_for_secrets,
};
use hermes_contract::{
    method::Method,
    methods::legacy::cron_manage,
    native::NativeMethod,
    overrides::{self, CronManageResult},
    schemas::{CronAction, IncludeDisabled, Repeat},
};
use serde_json::Value;
use talaria_core::cron::{
    AddReply, CallFailure, CronEffect, CronInput, DeliveryTarget, FireError, JobReply, JobRow,
    ListReply, RemoveReply, RestFailure, RunRow, TriggerOutcome,
};

struct CronManage;
impl Method for CronManage {
    const NAME: &'static str = cron_manage::NAME;
    type Params = cron_manage::Params;
    type Result = CronManageResult;
}
impl NativeMethod for CronManage {}

pub struct CronService {
    profile: String,
    generation: u64,
    rest: Option<RestClient>,
    client: Client,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CronTag {
    pub generation: u64,
    pub profile: String,
    pub run_name: Option<String>,
}

impl CronService {
    pub fn new(client: Client, rest: Option<RestClient>, profile: String, generation: u64) -> Self {
        Self {
            client,
            rest,
            profile,
            generation,
        }
    }
    pub fn rest_status(&self) -> Result<(), String> {
        self.rest
            .as_ref()
            .map(|_| ())
            .ok_or_else(|| "Hermes did not give a dashboard address (api_origin)".into())
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn profile(&self) -> &str {
        &self.profile
    }
    pub fn tag_for(&self, effect: &CronEffect) -> CronTag {
        CronTag {
            generation: self.generation,
            profile: self.profile.clone(),
            run_name: match effect {
                CronEffect::Trigger { name, .. } => Some(name.clone()),
                _ => None,
            },
        }
    }
    pub fn input_for_event(&self, type_name: &str) -> Option<CronInput> {
        (type_name == "cron.changed").then_some(CronInput::Changed)
    }
    async fn call(
        &self,
        params: cron_manage::Params,
        seconds: u64,
    ) -> Result<CronManageResult, CallFailure> {
        let result = self
            .client
            .call_with::<CronManage>(
                params,
                CallOptions {
                    timeout: Some(Duration::from_secs(seconds)),
                },
            )
            .await
            .map_err(|e| classify_call(&e))?;
        if result.success == Some(false) {
            return Err(CallFailure::Tool {
                error: field(&result.fields, "error").unwrap_or_default(),
                job_saved: boolean(&result.fields, "job_saved"),
                job_id: field(&result.fields, "job_id"),
                matches: result
                    .fields
                    .get("matches")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
            });
        }
        if result.success != Some(true) {
            return Err(CallFailure::BadResult("missing success".into()));
        }
        Ok(result)
    }
    pub async fn execute(&self, effect: CronEffect) -> Vec<CronInput> {
        let mut params = cron_manage::Params::default();
        let input = match effect {
            CronEffect::List { seq } => {
                params.action = CronAction::List;
                params.include_disabled = IncludeDisabled::Boolean(true);
                let result = self.call(params, 15).await.and_then(|r| {
                    let jobs = r
                        .jobs
                        .ok_or_else(bad_result)?
                        .into_iter()
                        .map(job_row)
                        .collect();
                    Ok(ListReply {
                        jobs,
                        warning: field(&r.fields, "warning"),
                    })
                });
                CronInput::Listed { seq, result }
            }
            CronEffect::Add(add) => {
                params.action = CronAction::Add;
                params.name = add.name;
                params.schedule = Some(add.schedule);
                params.prompt = Some(add.prompt);
                params.repeat = add
                    .repeat
                    .map_or(Repeat::Null, |n| Repeat::Integer(i64::from(n)));
                params.deliver = Some(add.deliver);
                let result = self.call(params, 30).await.and_then(|r| {
                    Ok(AddReply {
                        job_id: field(&r.fields, "job_id")
                            .or_else(|| r.job.as_ref().map(|j| j.job_id.clone()))
                            .ok_or_else(bad_result)?,
                        message: field(&r.fields, "message").unwrap_or_default(),
                        next_run_at: field(&r.fields, "next_run_at"),
                        job: r.job.map(job_row),
                    })
                });
                CronInput::Added(result)
            }
            CronEffect::SetPaused { job_id, paused } => {
                params.action = if paused {
                    CronAction::Pause
                } else {
                    CronAction::Resume
                };
                params.name = Some(job_id.clone());
                let result = self.call(params, 30).await.and_then(|r| {
                    Ok(JobReply {
                        job: job_row(r.job.ok_or_else(bad_result)?),
                    })
                });
                CronInput::PauseSet { job_id, result }
            }
            CronEffect::Remove { job_id } => {
                params.action = CronAction::Remove;
                params.name = Some(job_id.clone());
                let result = self.call(params, 30).await.and_then(|r| {
                    let removed = r
                        .fields
                        .get("removed_job")
                        .and_then(Value::as_object)
                        .ok_or_else(bad_result)?;
                    Ok(RemoveReply {
                        job_id: field(removed, "id").unwrap_or_else(|| job_id.clone()),
                        name: field(removed, "name").unwrap_or_default(),
                        message: field(&r.fields, "message").unwrap_or_default(),
                    })
                });
                CronInput::Removed { job_id, result }
            }
            CronEffect::Trigger { job_id, .. } => {
                let result = match &self.rest {
                    Some(rest) => cron_api::trigger_job(
                        rest,
                        &self.profile,
                        &job_id,
                        cron_api::trigger_opts(),
                    )
                    .await
                    .map_err(|e| classify_rest(&e)),
                    None => Err(no_rest()),
                };
                CronInput::Triggered {
                    job_id,
                    outcome: match result {
                        Ok(()) => TriggerOutcome::Accepted,
                        Err(e) => TriggerOutcome::Failed(e),
                    },
                }
            }
            CronEffect::Runs { job_id, seq } => {
                let result = match &self.rest {
                    Some(rest) => cron_api::job_runs(rest, &self.profile, &job_id, 20)
                        .await
                        .map_err(|e| classify_rest(&e))
                        .and_then(|v| decode_runs(&v)),
                    None => Err(no_rest()),
                };
                CronInput::RunsLoaded {
                    job_id,
                    seq,
                    result,
                }
            }
            CronEffect::DeliveryTargets => {
                let result = match &self.rest {
                    Some(rest) => cron_api::delivery_targets(rest)
                        .await
                        .map_err(|e| classify_rest(&e))
                        .and_then(|v| decode_targets(&v)),
                    None => Err(no_rest()),
                };
                CronInput::TargetsLoaded(result)
            }
            _ => return Vec::new(),
        };
        vec![input]
    }
}

fn bad_result() -> CallFailure {
    CallFailure::BadResult("result did not decode".into())
}
fn no_rest() -> RestFailure {
    RestFailure::NotSent("Hermes did not give a dashboard address (api_origin)".into())
}
fn field(map: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(Value::as_str).map(hide_secrets)
}
fn boolean(map: &serde_json::Map<String, Value>, key: &str) -> bool {
    map.get(key).and_then(Value::as_bool).unwrap_or(false)
}
fn job_row(row: overrides::CronJobRow) -> JobRow {
    let f = &row.fields;
    let s = |key| field(f, key);
    let fire = row.last_fire_error.map(|error| FireError {
        at: error.at.map(|v| hide_secrets(&v)),
        detail: hide_secrets(&error.detail),
    });
    JobRow {
        job_id: hide_secrets(&row.job_id),
        name: s("name").unwrap_or_default(),
        schedule: s("schedule").unwrap_or_default(),
        repeat: s("repeat")
            .or_else(|| {
                f.get("repeat")
                    .and_then(Value::as_i64)
                    .map(|n| n.to_string())
            })
            .unwrap_or_else(|| "forever".into()),
        deliver: s("deliver").unwrap_or_else(|| "local".into()),
        next_run_at: s("next_run_at"),
        last_run_at: s("last_run_at"),
        last_status: s("last_status"),
        last_error: s("last_error"),
        last_delivery_error: s("last_delivery_error"),
        last_fire_error: fire,
        enabled: f.get("enabled").and_then(Value::as_bool).unwrap_or(true),
        state: s("state").unwrap_or_default(),
        paused_at: s("paused_at"),
        paused_reason: s("paused_reason"),
        prompt_preview: s("prompt_preview").unwrap_or_default(),
        skills: f
            .get("skills")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(hide_secrets)
                    .collect()
            })
            .unwrap_or_default(),
        model: s("model"),
        provider: s("provider"),
        pinned: boolean(f, "pinned"),
        workdir: s("workdir"),
        script: f
            .get("script")
            .and_then(Value::as_str)
            .is_some_and(|v| !v.is_empty()),
        no_agent: boolean(f, "no_agent"),
        continuity: boolean(f, "continuity"),
    }
}

pub fn hide_secrets(text: &str) -> String {
    if scan_for_secrets(text).is_empty() {
        text.to_owned()
    } else {
        "[hidden: looked like a credential]".into()
    }
}

fn detail(text: &str) -> String {
    hide_secrets(text).chars().take(200).collect()
}

pub fn classify_rest(error: &RestError) -> RestFailure {
    match error {
        RestError::Forbidden { detail: text } => RestFailure::Forbidden(detail(text)),
        RestError::NotFound { .. } => RestFailure::NotFound,
        RestError::Refused {
            status,
            detail: text,
        } => RestFailure::Refused {
            status: *status,
            detail: detail(text.as_deref().unwrap_or_default()),
        },
        RestError::Server { status } => RestFailure::Server(*status),
        RestError::Unauthorized => RestFailure::Unauthorized,
        error if error.outcome_unknown() => RestFailure::Unknown,
        RestError::BadBody(text) => RestFailure::BadBody(detail(text)),
        RestError::BodyTooLarge => RestFailure::BadBody(detail(&error.to_string())),
        _ => RestFailure::NotSent(detail(&error.to_string())),
    }
}

fn classify_call(error: &CallError) -> CallFailure {
    match error {
        CallError::Rpc(rpc) => CallFailure::Rpc {
            code: rpc.code,
            message: detail(&rpc.message),
        },
        error if error.outcome_unknown() => CallFailure::Unknown,
        CallError::BadResult { reason, .. } => CallFailure::BadResult(detail(reason)),
        _ => CallFailure::NotSent(detail(&error.to_string())),
    }
}

fn text(row: &Value, key: &str) -> Option<String> {
    row.get(key).and_then(Value::as_str).map(hide_secrets)
}

fn decode_runs(value: &Value) -> Result<Vec<RunRow>, RestFailure> {
    let rows = value
        .get("runs")
        .and_then(Value::as_array)
        .ok_or_else(|| RestFailure::BadBody("missing runs array".into()))?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let count = |key| row.get(key).and_then(Value::as_u64).unwrap_or(0);
            let flag = |key| row.get(key).and_then(Value::as_bool).unwrap_or(false);
            Some(RunRow {
                id: text(row, "id")?,
                source: text(row, "source")?,
                title: text(row, "title"),
                preview: text(row, "preview"),
                started_at: row.get("started_at").and_then(Value::as_f64),
                ended_at: row.get("ended_at").and_then(Value::as_f64),
                end_reason: text(row, "end_reason"),
                input_tokens: count("input_tokens"),
                output_tokens: count("output_tokens"),
                message_count: count("message_count"),
                tool_call_count: count("tool_call_count"),
                model: text(row, "model"),
                is_active: flag("is_active"),
                scheduler_owned: flag("scheduler_owned"),
            })
        })
        .collect())
}

fn decode_targets(value: &Value) -> Result<Vec<DeliveryTarget>, RestFailure> {
    let rows = value
        .get("targets")
        .and_then(Value::as_array)
        .ok_or_else(|| RestFailure::BadBody("missing targets array".into()))?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let id = text(row, "id")?;
            Some(DeliveryTarget {
                name: text(row, "name").unwrap_or_else(|| id.clone()),
                home_target_set: id == "local"
                    || row
                        .get("home_target_set")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                id,
            })
        })
        .collect())
}
