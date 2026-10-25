//! Fenced session changes shared by every caller. Issue 230.

use std::collections::BTreeMap;

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
        let _ = (revision, execution_generation);
        todo!()
    }

    /// Read integer fences from a `session.info` or `session.updated` payload for this session.
    pub fn observe(&mut self, session_id: &str, type_name: &str, payload: &Value) {
        let _ = (session_id, type_name, payload);
        todo!()
    }

    /// Both numbers, once each has been seen.
    pub fn ready(&self) -> Option<(i64, i64)> {
        let _ = self;
        todo!()
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
        let _ = self;
        todo!()
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
    let _ = value;
    todo!()
}

/// Sort a JSON-RPC error. Fence refusals keep the numbers the gateway reported.
pub fn classify_error(code: i64, message: &str, data: Option<&Value>) -> MutationOutcome {
    let _ = (code, message, data);
    todo!()
}

/// Per-launch request ids and the fences and unanswered requests of each session.
#[derive(Debug)]
pub struct MutationLedger {
    launch_id: String,
    next_index: u64,
    fences: BTreeMap<String, Fences>,
    kept: Vec<MutationRequest>,
}

impl MutationLedger {
    pub fn new(launch_id: &str) -> Self {
        Self {
            launch_id: launch_id.to_string(),
            next_index: 1,
            fences: BTreeMap::new(),
            kept: Vec::new(),
        }
    }

    pub fn launch_id(&self) -> &str {
        &self.launch_id
    }

    pub fn fences_mut(&mut self, session_id: &str) -> &mut Fences {
        self.fences.entry(session_id.to_string()).or_default()
    }

    /// The next request, or the kept request when this change is the one whose reply was lost.
    pub fn build(
        &mut self,
        session_id: &str,
        operation: &str,
        payload: Map<String, Value>,
        with_generation: bool,
    ) -> Result<MutationRequest, MutationOutcome> {
        let _ = (
            session_id,
            operation,
            payload,
            with_generation,
            self.next_index,
        );
        todo!()
    }

    /// Remember a lost reply. Forget the request for every other outcome and adopt fences.
    pub fn settle(&mut self, request: &MutationRequest, outcome: &MutationOutcome) {
        let _ = (request, outcome, &mut self.kept);
        todo!()
    }

    /// One new request after a fence refusal, at the larger fences. A second refusal returns none.
    pub fn retry_after_fence(
        &mut self,
        request: &MutationRequest,
        outcome: &MutationOutcome,
    ) -> Option<MutationRequest> {
        let _ = (request, outcome);
        todo!()
    }
}
