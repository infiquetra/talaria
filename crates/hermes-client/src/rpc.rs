//! Typed JSON-RPC calls and frame classification.
//!
//! Every outbound request is one JSON object. The integer id comes from one
//! counter for the life of the [`crate::Client`]. A call whose connection
//! dropped, or that waited out its deadline, is never reported as success.
//!
//! Seven choices here differ from Hermes' own channel
//! (`apps/shared/src/json-rpc-channel.ts` at v0.22.4):
//!
//! * A numeric id with a method other than a valid event is
//!   `Malformed(UnexpectedRequest)` (Hermes sends it to the pending table, L470).
//! * An event is an event whatever its id (Hermes checks the pending table first, L470 before L500).
//! * A timeout of zero is [`CallError::InvalidTimeout`] and nothing is sent
//!   (Hermes sets no timer, L298).
//! * A reply with a non-null id and neither `result` nor `error` stays pending
//!   (Hermes resolves it with `undefined`, L493).
//! * An `id: null` error frame is [`MalformedKind::GatewayError`]
//!   (Hermes ignores it).
//! * An event whose `params.type` is not a string is [`MalformedKind::UnexpectedRequest`]
//!   (Hermes ignores it, L500).
//! * A failed write of a server-request answer stays answerable
//!   ([`crate::RespondError::NotConnected`]). Hermes marks the responder settled
//!   before the write (L371), so a failed write is final there.

#![warn(missing_docs)]

use std::time::Duration;

use hermes_contract::native::NativeMethod;
use serde_json::Value;

use crate::channel::{Client, MalformedKind};

/// How long an ordinary call waits. Hermes' `DEFAULT_REQUEST_TIMEOUT_MS`.
pub const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(120);

/// How long `approval.respond` waits. Hermes' `APPROVAL_RESPOND_TIMEOUT_MS`.
pub const APPROVAL_RESPOND_TIMEOUT: Duration = Duration::from_secs(300);

/// Per-call overrides. `timeout: None` uses [`default_timeout`].
#[derive(Clone, Copy, Debug, Default)]
pub struct CallOptions {
    /// Replaces the method's default. Zero is rejected before anything is sent.
    pub timeout: Option<Duration>,
}

/// The deadline for `method`, before a caller passes [`CallOptions::timeout`].
pub fn default_timeout(method: &str) -> Duration {
    if method == "approval.respond" {
        APPROVAL_RESPOND_TIMEOUT
    } else {
        DEFAULT_CALL_TIMEOUT
    }
}

/// What a raw frame is, before the pending table is consulted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameClass {
    /// A string id and a string method other than `event`.
    ServerRequest,
    /// `method` is `event` and `params.type` is a string.
    Event,
    /// An id with a `result` or an `error`, and no string method.
    Reply,
    /// The frame is kept off the pending table and off the event stream's success path.
    Malformed(MalformedKind),
}

/// Classify one WebSocket payload. Total: it does not panic and it does not read the pending table.
///
/// Valid UTF-8 binary is text. A null or non-string `method` counts as no method.
#[doc(hidden)]
pub fn classify(raw: &[u8]) -> FrameClass {
    let Ok(value) = serde_json::from_slice::<Value>(raw) else {
        return FrameClass::Malformed(MalformedKind::NotJson);
    };
    let Some(object) = value.as_object() else {
        return FrameClass::Malformed(MalformedKind::NotObject);
    };
    let method = object.get("method").and_then(Value::as_str);
    let id = object.get("id");
    if id.is_some_and(Value::is_string) && method.is_some_and(|name| name != "event") {
        return FrameClass::ServerRequest;
    }
    if method == Some("event") && value.pointer("/params/type").is_some_and(Value::is_string) {
        return FrameClass::Event;
    }
    if method.is_some() {
        return FrameClass::Malformed(MalformedKind::UnexpectedRequest);
    }
    if id.is_some_and(|id| id.is_string() || id.is_i64() || id.is_u64())
        && (object.contains_key("result") || object.contains_key("error"))
    {
        return FrameClass::Reply;
    }
    if id.is_none_or(Value::is_null) && object.contains_key("error") {
        return FrameClass::Malformed(MalformedKind::GatewayError);
    }
    FrameClass::Malformed(MalformedKind::Unrecognized)
}

/// Encode one request. Keys are `jsonrpc`, `id`, `method`, `params`, in that order.
///
/// Null `params` is sent as `{}`. Any other non-object is [`CallError::Encode`].
#[doc(hidden)]
pub fn encode_request(id: u64, method: &str, params: &Value) -> Result<String, CallError> {
    #[derive(serde::Serialize)]
    struct Request<'a> {
        jsonrpc: &'static str,
        id: u64,
        method: &'a str,
        params: &'a Value,
    }
    let empty = Value::Object(serde_json::Map::new());
    let params = match params {
        Value::Null => &empty,
        Value::Object(_) => params,
        _ => return Err(CallError::Encode),
    };
    serde_json::to_string(&Request {
        jsonrpc: "2.0",
        id,
        method,
        params,
    })
    .map_err(|_| CallError::Encode)
}

/// Why a call did not return a decoded result.
#[derive(Clone)]
pub enum CallError {
    /// No connection was current when the actor took the call. Nothing was sent.
    NotConnected,
    /// `params` could not be encoded, or was neither an object nor null. Nothing was sent.
    Encode,
    /// The timeout was zero. Nothing was sent.
    InvalidTimeout,
    /// The gateway answered with `error`. `result` beside it does not change that.
    Rpc(RpcError),
    /// The gateway answered with `result`, and that value does not decode.
    ///
    /// `reason` is the fixed phrase `result did not decode`. The gateway did the work.
    BadResult {
        /// The method that was called.
        method: String,
        /// Always `result did not decode`.
        reason: &'static str,
    },
    /// No reply arrived before the deadline. The gateway may still apply the call.
    TimedOut {
        /// The method that was called.
        method: String,
        /// How long the caller waited, including any time queued behind a replace.
        after: Duration,
    },
    /// The connection closed, was replaced, or the write failed after the call was registered.
    Unknown {
        /// The method that was called.
        method: String,
    },
}

impl CallError {
    /// `true` only for [`TimedOut`](Self::TimedOut) and [`Unknown`](Self::Unknown).
    pub fn outcome_unknown(&self) -> bool {
        matches!(self, Self::TimedOut { .. } | Self::Unknown { .. })
    }

    /// One sentence telling a person what to do next. It contains no reply text.
    pub fn next_step(&self) -> &'static str {
        match self {
            Self::NotConnected => "Connect before retrying; nothing was sent.",
            Self::Encode => "Correct the parameters before retrying; nothing was sent.",
            Self::InvalidTimeout => "Choose a nonzero timeout; nothing was sent.",
            Self::Rpc(_) => "Check the refusal reason before deciding whether to retry.",
            Self::BadResult { .. } => {
                "Check the gateway result contract; the gateway did the work."
            }
            Self::TimedOut { .. } | Self::Unknown { .. } => {
                "Check the gateway state before retrying; the gateway may have applied the call."
            }
        }
    }
}

impl std::fmt::Debug for CallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConnected => formatter.write_str("NotConnected"),
            Self::Encode => formatter.write_str("Encode"),
            Self::InvalidTimeout => formatter.write_str("InvalidTimeout"),
            Self::Rpc(error) => formatter.debug_tuple("Rpc").field(error).finish(),
            Self::BadResult { method, .. } => formatter
                .debug_struct("BadResult")
                .field("method", method)
                .field("reason", &"result did not decode")
                .finish(),
            Self::TimedOut { method, after } => formatter
                .debug_struct("TimedOut")
                .field("method", method)
                .field("after", after)
                .finish(),
            Self::Unknown { method } => formatter
                .debug_struct("Unknown")
                .field("method", method)
                .finish(),
        }
    }
}

impl std::fmt::Display for CallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConnected => formatter.write_str("not connected"),
            Self::Encode => formatter.write_str("request parameters did not encode"),
            Self::InvalidTimeout => formatter.write_str("timeout must be nonzero"),
            Self::Rpc(error) => write!(formatter, "{error}"),
            Self::BadResult { method, .. } => write!(formatter, "{method}: result did not decode"),
            Self::TimedOut { method, after } => write!(
                formatter,
                "{method}: timed out after {after:?}; the gateway may have applied the call"
            ),
            Self::Unknown { method } => write!(
                formatter,
                "{method}: outcome unknown; the gateway may have applied the call"
            ),
        }
    }
}

impl std::error::Error for CallError {}

/// An error object from a reply. Branch on [`reason`](Self::reason), never on code 4001 alone.
///
/// `Debug` and `Display` show the code, the reason, and the lengths of `message` and `data`.
/// They do not show `message` or `data`.
#[derive(Clone)]
pub struct RpcError {
    /// The integer code. A non-integer code becomes -32603.
    pub code: i64,
    /// The gateway's message. Callers may read it. Formatting this error does not show it.
    pub message: String,
    /// `data.reason` when that field is a string.
    pub reason: Option<String>,
    /// The whole `data` value, when it was a JSON object.
    pub data: Option<Value>,
}

impl RpcError {
    /// Read a reply's `error` value.
    ///
    /// A non-integer `code`, or an `error` that is not an object, becomes code -32603
    /// and message `unreadable error reply`.
    pub fn from_value(error: &Value) -> Self {
        let Some(code) = error
            .as_object()
            .and_then(|object| object.get("code"))
            .and_then(Value::as_i64)
        else {
            return Self {
                code: -32603,
                message: "unreadable error reply".into(),
                reason: None,
                data: None,
            };
        };
        let data = error.get("data").filter(|data| data.is_object()).cloned();
        Self {
            code,
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            reason: data
                .as_ref()
                .and_then(|data| data.get("reason"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            data,
        }
    }

    /// The native socket's unknown-method reply: code -32601 and reason `unknown_method`.
    pub fn is_method_not_found(&self) -> bool {
        self.code == -32601 && self.reason.as_deref() == Some("unknown_method")
    }
}

impl std::fmt::Debug for RpcError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RpcError")
            .field("code", &self.code)
            .field("reason", &self.reason)
            .field("message_length", &self.message.len())
            .field(
                "data_length",
                &self.data.as_ref().map_or(0, |data| data.to_string().len()),
            )
            .finish()
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for RpcError {}

impl Client {
    /// Send one typed request and wait for the decoded result or a [`CallError`].
    ///
    /// ```no_run
    /// # async fn inspect(client: &hermes_client::Client) -> Result<(), hermes_client::CallError> {
    /// use hermes_contract::{methods::canonical::runtime_describe, schemas::RuntimeDescribeParams};
    /// let description = client.call::<runtime_describe::Marker>(
    ///     RuntimeDescribeParams(serde_json::Map::new()),
    /// ).await?;
    /// # let _ = description;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn call<M: NativeMethod>(&self, params: M::Params) -> Result<M::Result, CallError> {
        self.call_with::<M>(params, CallOptions::default()).await
    }

    /// [`call`](Self::call) with an explicit timeout.
    pub async fn call_with<M: NativeMethod>(
        &self,
        params: M::Params,
        options: CallOptions,
    ) -> Result<M::Result, CallError> {
        let started = tokio::time::Instant::now();
        let timeout = options.timeout.unwrap_or_else(|| default_timeout(M::NAME));
        if timeout.is_zero() {
            return Err(CallError::InvalidTimeout);
        }
        let params = serde_json::to_value(params).map_err(|_| CallError::Encode)?;
        if !params.is_null() && !params.is_object() {
            return Err(CallError::Encode);
        }
        let result = self.execute_call(M::NAME, params, timeout, started).await?;
        serde_json::from_value(result).map_err(|_| CallError::BadResult {
            method: M::NAME.into(),
            reason: "result did not decode",
        })
    }

    /// Send a method that has no typed binding. Tests, diagnostics, and `gateway.ping` use this.
    ///
    /// The id still comes from the client's one counter.
    #[doc(hidden)]
    pub async fn call_raw(
        &self,
        method: &str,
        params: Value,
        options: CallOptions,
    ) -> Result<Value, CallError> {
        let started = tokio::time::Instant::now();
        let timeout = options.timeout.unwrap_or_else(|| default_timeout(method));
        if timeout.is_zero() {
            return Err(CallError::InvalidTimeout);
        }
        if !params.is_null() && !params.is_object() {
            return Err(CallError::Encode);
        }
        self.execute_call(method, params, timeout, started).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_timeout_table() {
        assert_eq!(
            default_timeout("approval.respond"),
            Duration::from_secs(300)
        );
        for name in ["prompt.submit", "runtime.describe", "not.in.contract", ""] {
            assert_eq!(default_timeout(name), Duration::from_secs(120), "{name}");
        }
    }

    #[test]
    fn classify_frame_covers_every_shape() {
        let cases: &[(&[u8], FrameClass)] = &[
            (b"not json", FrameClass::Malformed(MalformedKind::NotJson)),
            (&[0xff, 0xfe], FrameClass::Malformed(MalformedKind::NotJson)),
            (b"[1,2]", FrameClass::Malformed(MalformedKind::NotObject)),
            (b"\"x\"", FrameClass::Malformed(MalformedKind::NotObject)),
            (b"3", FrameClass::Malformed(MalformedKind::NotObject)),
            (
                br#"{"id":"srq-0123456789ab","method":"sudo","params":{}}"#,
                FrameClass::ServerRequest,
            ),
            (
                br#"{"method":"event","params":{"type":"message.delta","payload":{}}}"#,
                FrameClass::Event,
            ),
            (
                br#"{"method":"event","id":"x","params":{"type":"message.delta","payload":{}}}"#,
                FrameClass::Event,
            ),
            (
                br#"{"method":"event","id":5,"params":{"type":"message.delta","payload":{}}}"#,
                FrameClass::Event,
            ),
            (
                br#"{"id":5,"method":"x"}"#,
                FrameClass::Malformed(MalformedKind::UnexpectedRequest),
            ),
            (
                br#"{"method":"event","params":{}}"#,
                FrameClass::Malformed(MalformedKind::UnexpectedRequest),
            ),
            (br#"{"id":3,"result":{}}"#, FrameClass::Reply),
            (br#"{"id":"hb-1","result":{"ok":true}}"#, FrameClass::Reply),
            (
                br#"{"id":3,"error":{"code":1,"message":"m"}}"#,
                FrameClass::Reply,
            ),
            (
                br#"{"id":null,"error":{"code":-32700,"message":"parse error"}}"#,
                FrameClass::Malformed(MalformedKind::GatewayError),
            ),
            (
                br#"{"error":{"code":1,"message":"m"}}"#,
                FrameClass::Malformed(MalformedKind::GatewayError),
            ),
            (b"{}", FrameClass::Malformed(MalformedKind::Unrecognized)),
            (
                br#"{"id":3}"#,
                FrameClass::Malformed(MalformedKind::Unrecognized),
            ),
            (
                br#"{"id":null,"result":1}"#,
                FrameClass::Malformed(MalformedKind::Unrecognized),
            ),
            (
                br#"{"jsonrpc":"2.0"}"#,
                FrameClass::Malformed(MalformedKind::Unrecognized),
            ),
            (
                br#"{"id":3,"method":null}"#,
                FrameClass::Malformed(MalformedKind::Unrecognized),
            ),
            (br#"{"id":3,"method":null,"result":{}}"#, FrameClass::Reply),
        ];
        let mut saw = std::collections::BTreeSet::new();
        for (frame, expected) in cases {
            let class = classify(frame);
            assert_eq!(class, *expected);
            if let FrameClass::Malformed(kind) = class {
                saw.insert(kind);
            }
        }
        for kind in [
            MalformedKind::NotJson,
            MalformedKind::NotObject,
            MalformedKind::UnexpectedRequest,
            MalformedKind::GatewayError,
            MalformedKind::Unrecognized,
        ] {
            assert!(saw.contains(&kind), "classifier never produced {kind:?}");
        }
    }

    #[test]
    fn error_reply_keeps_code_message_reason_and_data() {
        let shapes = [
            json!({"code": 4001, "message": "not_found", "data": {"reason": "not_found"}}),
            json!({"code": -32601, "message": "unknown method: session.compress", "data": {"reason": "unknown_method"}}),
            json!({"code": 4001, "message": "stale_generation", "data": {"reason": "stale_generation", "execution_generation": 7}}),
            json!({"code": 5001, "message": "storage_unavailable", "data": {"reason": "storage_unavailable"}}),
            json!({"code": 4001, "message": "invalid_params", "data": {"reason": "invalid_params", "fields": ["limit"]}}),
            json!({"code": -32700, "message": "parse error"}),
        ];
        let parsed: Vec<RpcError> = shapes.iter().map(RpcError::from_value).collect();
        assert_eq!(parsed[0].code, 4001);
        assert_eq!(parsed[0].message, "not_found");
        assert_eq!(parsed[0].reason.as_deref(), Some("not_found"));
        assert_eq!(parsed[1].code, -32601);
        assert_eq!(parsed[1].reason.as_deref(), Some("unknown_method"));
        assert_eq!(parsed[2].reason.as_deref(), Some("stale_generation"));
        assert_eq!(
            parsed[2]
                .data
                .as_ref()
                .and_then(|data| data.get("execution_generation")),
            Some(&json!(7))
        );
        assert_eq!(parsed[3].reason.as_deref(), Some("storage_unavailable"));
        assert_eq!(parsed[4].reason.as_deref(), Some("invalid_params"));
        assert_eq!(
            parsed[4].data.as_ref().and_then(|data| data.get("fields")),
            Some(&json!(["limit"]))
        );
        assert_eq!(parsed[5].code, -32700);
        assert_eq!(parsed[5].message, "parse error");
        assert_eq!(parsed[5].reason, None);
        assert_eq!(parsed[5].data, None);

        let reason_not_string =
            RpcError::from_value(&json!({"code": 4001, "message": "x", "data": {"reason": 1}}));
        assert_eq!(reason_not_string.reason, None);
        assert!(reason_not_string.data.is_some());

        let data_not_object =
            RpcError::from_value(&json!({"code": 1, "message": "x", "data": "nope"}));
        assert_eq!(data_not_object.reason, None);
        assert_eq!(data_not_object.data, None);

        for bad in [
            json!({"code": "4001", "message": "x"}),
            json!("nope"),
            json!(1),
        ] {
            let error = RpcError::from_value(&bad);
            assert_eq!(error.code, -32603);
            assert_eq!(error.message, "unreadable error reply");
        }

        assert!(
            CallError::TimedOut {
                method: "m".into(),
                after: Duration::from_secs(1)
            }
            .outcome_unknown()
        );
        assert!(CallError::Unknown { method: "m".into() }.outcome_unknown());
        let rpc = CallError::Rpc(parsed[0].clone());
        assert!(!rpc.outcome_unknown());
        assert!(!CallError::NotConnected.outcome_unknown());
        assert!(!CallError::Encode.outcome_unknown());
        assert!(!CallError::InvalidTimeout.outcome_unknown());
        assert!(
            !CallError::BadResult {
                method: "m".into(),
                reason: "result did not decode"
            }
            .outcome_unknown()
        );
    }

    #[test]
    fn request_frame_is_compact_json_with_integer_id() {
        let empty = encode_request(3, "runtime.describe", &json!({})).expect("object");
        let null = encode_request(3, "runtime.describe", &Value::Null).expect("null");
        let exact = r#"{"jsonrpc":"2.0","id":3,"method":"runtime.describe","params":{}}"#;
        assert_eq!(empty, exact);
        assert_eq!(null, exact);

        let listed =
            encode_request(3, "session.list", &json!({"limit": 5, "scope": "live"})).expect("list");
        let prefix = r#"{"jsonrpc":"2.0","id":3,"method":"session.list","params":"#;
        assert!(listed.starts_with(prefix), "envelope prefix missing");
        let params: Value =
            serde_json::from_str(&listed[prefix.len()..listed.len() - 1]).expect("params");
        assert_eq!(params, json!({"limit": 5, "scope": "live"}));

        assert!(matches!(
            encode_request(3, "runtime.describe", &json!([1])),
            Err(CallError::Encode)
        ));
    }

    const SENTINEL: &str = "SENTINEL-TEXT-123";

    fn shows(label: &str, text: &str, token: &str) {
        assert!(
            text.contains(token),
            "{label} omitted {token}; length {}",
            text.len()
        );
        assert!(
            !text.contains(SENTINEL),
            "{label} leaked hidden text; length {}",
            text.len()
        );
    }

    #[test]
    fn debug_and_display_never_show_frame_text() {
        let rpc = RpcError {
            code: 4001,
            message: SENTINEL.into(),
            reason: Some("not_found".into()),
            data: Some(json!({"reason": "not_found", "detail": SENTINEL})),
        };
        for rendered in [format!("{rpc:?}"), format!("{rpc}")] {
            shows("RpcError", &rendered, "not_found");
            shows("RpcError", &rendered, "4001");
        }

        let call = CallError::Rpc(rpc);
        for rendered in [format!("{call:?}"), format!("{call}")] {
            shows("CallError", &rendered, "not_found");
            assert!(
                rendered.contains("runtime.describe")
                    || rendered.contains("Rpc")
                    || rendered.contains("4001")
            );
        }

        let params = json!({
            "type": "message.delta",
            "session_id": "sess-shown",
            "seq": 9,
            "payload": {"text": SENTINEL}
        });
        let event = crate::InboundEvent {
            generation: crate::Generation(1),
            frame: hermes_contract::events::EventFrame::from_params(&params).expect("event"),
            raw: params,
        };
        for rendered in [format!("{event:?}"), format!("{event}")] {
            shows("InboundEvent", &rendered, "message.delta");
            shows("InboundEvent", &rendered, "sess-shown");
        }

        let request_value = json!({
            "jsonrpc": "2.0",
            "id": "srq-0123456789ab",
            "method": "sudo",
            "params": {"session_id": "sess-shown", "command": SENTINEL}
        });
        let decoded = hermes_contract::events::ServerRequestFrame::from_request(&request_value)
            .expect("sudo");
        let request = crate::ServerRequest {
            generation: crate::Generation(1),
            id: "srq-0123456789ab".into(),
            session_id: Some("sess-shown".into()),
            request: decoded.request,
            responder: crate::Responder::detached(std::sync::Arc::new(
                std::sync::atomic::AtomicU64::new(0),
            )),
        };
        for rendered in [format!("{request:?}"), format!("{request}")] {
            shows("ServerRequest", &rendered, "srq-0123456789ab");
            shows("ServerRequest", &rendered, "sess-shown");
        }
        for rendered in [
            format!("{:?}", request.responder),
            format!("{}", request.responder),
        ] {
            shows("Responder", &rendered, "state");
        }

        let malformed = crate::MalformedFrame {
            generation: crate::Generation(1),
            kind: MalformedKind::NotJson,
            length: SENTINEL.len(),
            label: Some("message.delta".into()),
        };
        for rendered in [format!("{malformed:?}"), format!("{malformed}")] {
            shows("MalformedFrame", &rendered, "message.delta");
            assert!(
                !rendered.contains(SENTINEL),
                "malformed leaked text; length {}",
                rendered.len()
            );
        }

        let refused = crate::RespondError::Encode;
        for rendered in [format!("{refused:?}"), format!("{refused}")] {
            assert!(
                !rendered.contains(SENTINEL),
                "RespondError leaked text; length {}",
                rendered.len()
            );
        }
    }
}
