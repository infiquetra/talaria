//! Fenced session changes shared by every caller.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

/// The revision and execution generation a later change must present.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fences {
    pub revision: Option<i64>,
    pub execution_generation: Option<i64>,
}

impl Fences {
    /// Keep the larger number. An empty report never clears a number already held.
    pub fn adopt(&mut self, revision: Option<i64>, execution_generation: Option<i64>) {
        if let Some(value) = revision {
            self.revision = Some(self.revision.map_or(value, |held| held.max(value)));
        }
        if let Some(value) = execution_generation {
            self.execution_generation = Some(
                self.execution_generation
                    .map_or(value, |held| held.max(value)),
            );
        }
    }

    /// Read integer fences from a `session.info` or `session.updated` payload for this session.
    pub fn observe(&mut self, session_id: &str, type_name: &str, payload: &Value) {
        if !matches!(type_name, "session.info" | "session.updated") {
            return;
        }
        let identity = payload
            .get("session_id")
            .or_else(|| payload.get("stored_session_id"))
            .and_then(Value::as_str);
        if identity.is_some_and(|identity| identity != session_id) {
            return;
        }
        let body = payload.get("payload").unwrap_or(payload);
        self.adopt(
            body.get("revision").and_then(Value::as_i64),
            body.get("execution_generation").and_then(Value::as_i64),
        );
    }

    /// Both numbers, once each has been seen.
    pub fn ready(&self) -> Option<(i64, i64)> {
        Some((self.revision?, self.execution_generation?))
    }
}

/// One `session.mutate` call: the six wire keys and nothing else.
#[derive(Clone, Debug, PartialEq)]
pub struct MutationRequest {
    pub session_id: String,
    pub request_id: String,
    pub expected_revision: i64,
    pub expected_generation: Option<i64>,
    pub operation: String,
    pub payload: Map<String, Value>,
}

impl MutationRequest {
    /// The params object. `expected_generation` is omitted only when it is empty.
    pub fn to_params(&self) -> Value {
        let mut params = Map::from_iter([
            ("session_id".into(), Value::from(self.session_id.as_str())),
            ("request_id".into(), Value::from(self.request_id.as_str())),
            (
                "expected_revision".into(),
                Value::from(self.expected_revision),
            ),
            ("operation".into(), Value::from(self.operation.as_str())),
            ("payload".into(), Value::Object(self.payload.clone())),
        ]);
        if let Some(generation) = self.expected_generation {
            params.insert("expected_generation".into(), Value::from(generation));
        }
        Value::Object(params)
    }
}

/// What one mutate call did, including the cases where the caller cannot know.
#[derive(Clone, Debug, PartialEq)]
pub enum MutationOutcome {
    Committed(Value),
    Deferred(Value),
    NeedsConfirmation(Confirmation),
    OtherStatus {
        status: String,
        result: Value,
    },
    Refused {
        reason: String,
        field: Option<String>,
        detail: Option<String>,
        rpc_code: i64,
    },
    FenceRefused {
        reason: String,
        revision: Option<i64>,
        execution_generation: Option<i64>,
    },
    Unknown,
    Unreadable,
    NotSent {
        reason: String,
    },
}

/// A switch Hermes will apply only when the same pick is sent again with this token.
#[derive(Clone, Debug, PartialEq)]
pub struct Confirmation {
    pub token: String,
    pub message: String,
    pub target_model: String,
    pub target_provider: Option<String>,
    pub pick: Map<String, Value>,
}

/// Sort a mutate result into one outcome. A result that is not an object is unreadable.
pub fn classify_result(value: &Value) -> MutationOutcome {
    let Some(object) = value.as_object() else {
        return MutationOutcome::Unreadable;
    };
    if object.get("session_id").and_then(Value::as_str).is_none()
        || object.get("operation").and_then(Value::as_str).is_none()
    {
        return MutationOutcome::Unreadable;
    }
    match object.get("status") {
        None => MutationOutcome::Committed(value.clone()),
        Some(Value::String(status)) if status == "deferred" => {
            MutationOutcome::Deferred(value.clone())
        }
        Some(Value::String(status)) if status == "confirmation_required" => {
            let Some(token) = object.get("confirm").and_then(Value::as_str) else {
                return MutationOutcome::Unreadable;
            };
            let Some(model) = object.get("target_model").and_then(Value::as_str) else {
                return MutationOutcome::Unreadable;
            };
            MutationOutcome::NeedsConfirmation(Confirmation {
                token: token.to_string(),
                message: object
                    .get("confirm_message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                target_model: model.to_string(),
                target_provider: object
                    .get("target_provider")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                pick: object
                    .get("payload")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default(),
            })
        }
        Some(Value::String(status)) => MutationOutcome::OtherStatus {
            status: status.clone(),
            result: value.clone(),
        },
        Some(_) => MutationOutcome::Unreadable,
    }
}

/// Sort a JSON-RPC error. Fence refusals keep the numbers the gateway reported.
pub fn classify_error(code: i64, message: &str, data: Option<&Value>) -> MutationOutcome {
    let reason = data
        .and_then(|value| value.get("reason"))
        .and_then(Value::as_str)
        .unwrap_or(message)
        .to_string();
    if matches!(reason.as_str(), "revision_conflict" | "stale_generation") {
        return MutationOutcome::FenceRefused {
            reason,
            revision: data
                .and_then(|value| value.get("revision"))
                .and_then(Value::as_i64),
            execution_generation: data
                .and_then(|value| value.get("execution_generation"))
                .and_then(Value::as_i64),
        };
    }
    MutationOutcome::Refused {
        reason,
        field: data
            .and_then(|value| value.get("field"))
            .and_then(Value::as_str)
            .map(str::to_string),
        detail: data
            .and_then(|value| value.get("detail"))
            .and_then(Value::as_str)
            .map(str::to_string),
        rpc_code: code,
    }
}

/// Per-launch request ids and the fences and unanswered requests of each session.
#[derive(Debug)]
pub struct MutationLedger {
    launch_id: String,
    next_index: u64,
    fences: BTreeMap<String, Fences>,
    kept: Vec<MutationRequest>,
    fence_retries: BTreeSet<String>,
}

impl MutationLedger {
    pub fn new(launch_id: &str) -> Self {
        Self {
            launch_id: launch_id.to_string(),
            next_index: 1,
            fences: BTreeMap::new(),
            kept: Vec::new(),
            fence_retries: BTreeSet::new(),
        }
    }

    pub fn launch_id(&self) -> &str {
        &self.launch_id
    }

    pub fn fences_mut(&mut self, session_id: &str) -> &mut Fences {
        self.fences.entry(session_id.to_string()).or_default()
    }

    /// The fences observed for a session, without creating a ledger entry.
    pub fn fences(&self, session_id: &str) -> Fences {
        self.fences.get(session_id).copied().unwrap_or_default()
    }

    /// The next request, or the kept request when this change is the one whose reply was lost.
    pub fn build(
        &mut self,
        session_id: &str,
        operation: &str,
        payload: Map<String, Value>,
        with_generation: bool,
    ) -> Result<MutationRequest, MutationOutcome> {
        if let Some(kept) = self.kept.iter().find(|kept| {
            kept.session_id == session_id
                && kept.operation == operation
                && kept.payload == payload
                && kept.expected_generation.is_some() == with_generation
        }) {
            return Ok(kept.clone());
        }
        self.kept.retain(|kept| kept.session_id != session_id);
        let fences = self.fences.get(session_id).copied().unwrap_or_default();
        let Some(revision) = fences.revision else {
            return Err(MutationOutcome::NotSent {
                reason: "session fences are unknown".into(),
            });
        };
        if with_generation && fences.execution_generation.is_none() {
            return Err(MutationOutcome::NotSent {
                reason: "session fences are unknown".into(),
            });
        }
        let request = MutationRequest {
            session_id: session_id.to_string(),
            request_id: self.next_request_id(),
            expected_revision: revision,
            expected_generation: if with_generation {
                fences.execution_generation
            } else {
                None
            },
            operation: operation.to_string(),
            payload,
        };
        Ok(request)
    }

    /// Remember a lost reply. Forget the request for every other outcome and adopt fences.
    pub fn settle(&mut self, request: &MutationRequest, outcome: &MutationOutcome) {
        self.observe_request_id(&request.request_id);
        self.kept
            .retain(|kept| kept.session_id != request.session_id);
        match outcome {
            MutationOutcome::Unknown => self.kept.push(request.clone()),
            MutationOutcome::Committed(result)
            | MutationOutcome::Deferred(result)
            | MutationOutcome::OtherStatus { result, .. } => {
                self.fences_mut(&request.session_id).adopt(
                    result.get("revision").and_then(Value::as_i64),
                    result.get("execution_generation").and_then(Value::as_i64),
                );
            }
            MutationOutcome::FenceRefused {
                revision,
                execution_generation,
                ..
            } => {
                self.fences_mut(&request.session_id)
                    .adopt(*revision, *execution_generation);
            }
            _ => {}
        }
        if !matches!(
            outcome,
            MutationOutcome::FenceRefused { .. } | MutationOutcome::Unknown
        ) {
            self.fence_retries.remove(&request.request_id);
        }
    }

    /// One new request after a fence refusal, at the larger fences. A second refusal returns none.
    pub fn retry_after_fence(
        &mut self,
        request: &MutationRequest,
        outcome: &MutationOutcome,
    ) -> Option<MutationRequest> {
        let MutationOutcome::FenceRefused {
            revision,
            execution_generation,
            ..
        } = outcome
        else {
            return None;
        };
        self.fences_mut(&request.session_id)
            .adopt(*revision, *execution_generation);
        if self.fence_retries.contains(&request.request_id) {
            return None;
        }
        self.observe_request_id(&request.request_id);
        self.kept
            .retain(|kept| kept.session_id != request.session_id);
        let retry = self
            .build(
                &request.session_id,
                &request.operation,
                request.payload.clone(),
                request.expected_generation.is_some(),
            )
            .ok()?;
        self.fence_retries.insert(retry.request_id.clone());
        Some(retry)
    }

    fn next_request_id(&mut self) -> String {
        let id = format!("{}-m{}", self.launch_id, self.next_index);
        self.next_index += 1;
        id
    }

    fn observe_request_id(&mut self, id: &str) {
        if let Some(index) = id
            .strip_prefix(&format!("{}-m", self.launch_id))
            .and_then(|value| value.parse::<u64>().ok())
        {
            self.next_index = self.next_index.max(index.saturating_add(1));
        }
    }
}
