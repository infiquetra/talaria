//! A profile-routed gateway port with durable receipts and independent event delivery.
use super::{
    feed::{AgentFeedItem, AgentFeedReceiver},
    frames::{CREATE_KEYS, create_params, submit_params},
    wire::{Cursor, LinkWireFactory, Wire, WireError, WireFactory},
};
use crate::journal::{
    AdmissionState, CreateSession, GatewayPort, PortError, Receipt, SessionBound, SubmitTurn,
};
use futures::future::BoxFuture;
use hermes_contract::schemas::{
    AdmissionStatus, CanonicalSessionHandle, CanonicalSnapshot, PromptReceiptResult,
    RuntimeDescribeResult,
};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot, watch};
#[path = "state.rs"]
mod state;
use state::{Binding, Command, Key, View};

#[derive(Clone, Debug)]
pub struct PollConfig {
    pub start: Duration,
    pub cap: Duration,
}
impl Default for PollConfig {
    fn default() -> Self {
        Self {
            start: Duration::from_millis(250),
            cap: Duration::from_secs(2),
        }
    }
}
#[derive(Clone, Debug)]
pub struct RpcTimeouts {
    pub describe: Duration,
    pub resume: Duration,
    pub interrupt: Duration,
    pub cancel: Duration,
    pub list: Duration,
    pub poll: Duration,
    pub receipt: Duration,
    pub resolve_unknown: Duration,
    pub create: Duration,
    pub submit: Duration,
}
impl Default for RpcTimeouts {
    fn default() -> Self {
        let short = Duration::from_secs(10);
        let final_read = Duration::from_secs(30);
        Self {
            describe: short,
            resume: short,
            interrupt: short,
            cancel: short,
            list: short,
            poll: short,
            receipt: final_read,
            resolve_unknown: final_read,
            create: Duration::from_secs(120),
            submit: Duration::from_secs(120),
        }
    }
}
#[derive(Clone, Debug)]
pub struct PortConfig {
    pub home: PathBuf,
    pub poll: PollConfig,
    pub timeouts: RpcTimeouts,
}
impl Default for PortConfig {
    fn default() -> Self {
        Self {
            home: hermes_client::default_home(),
            poll: PollConfig::default(),
            timeouts: RpcTimeouts::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PortStats {
    pub runtime_describe: u64,
    pub session_create: u64,
    pub session_resume: u64,
    pub prompt_submit: u64,
    pub prompt_receipt: u64,
    pub prompt_cancel: u64,
    pub session_interrupt: u64,
    pub prompt_resolve_unknown: u64,
    pub session_list: u64,
    pub dropped_items: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionInfo {
    pub session_id: String,
    pub request_id: String,
    pub label: String,
    pub source: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub reasoning_effort: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopResult {
    Sent,
    NotRunning,
    Unknown,
}
#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    #[error("Hermes gateway not found")]
    NotFound(String),
    #[error("gateway unsupported: {missing:?}")]
    Unsupported { missing: Vec<String> },
    #[error("gateway connection failed")]
    Connect(String),
}
#[derive(Clone)]
pub struct HermesPort {
    pub(super) inner: Arc<Inner>,
}
pub(super) struct Inner {
    tx: mpsc::UnboundedSender<Command>,
    view: watch::Receiver<View>,
    factory: Arc<dyn WireFactory>,
    cfg: PortConfig,
    feed: mpsc::UnboundedSender<AgentFeedItem>,
    wakers: Mutex<BTreeMap<Key, watch::Sender<u64>>>,
}
impl Drop for Inner {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Shutdown);
    }
}
impl Inner {
    pub(super) fn wake(&self, key: &Key) {
        if let Ok(wakers) = self.wakers.lock()
            && let Some(waker) = wakers.get(key)
        {
            waker.send_modify(|n| *n = n.wrapping_add(1));
        }
    }
    fn subscribe(&self, key: Key) -> watch::Receiver<u64> {
        let mut wakers = self
            .wakers
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        wakers
            .entry(key)
            .or_insert_with(|| watch::channel(0).0)
            .subscribe()
    }
}
struct Pending {
    tx: mpsc::UnboundedSender<Command>,
    profile: String,
}
impl Drop for Pending {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::EndPending(self.profile.clone()));
    }
}
fn transport() -> PortError {
    PortError::Transport("gateway port closed".into())
}
fn refusal(reason: &str, field: Option<&str>, detail: Option<String>) -> PortError {
    PortError::Refused {
        reason: reason.into(),
        field: field.map(str::to_owned),
        detail,
    }
}
pub fn map_wire_error(method: &str, error: WireError) -> PortError {
    match error {
        WireError::Timeout => PortError::Timeout,
        WireError::Closed => PortError::Transport(format!("{method}: connection lost")),
        WireError::NotServed(_) => refusal("profile_unavailable", Some("profile"), None),
        WireError::Other(_) => PortError::Transport(format!("{method}: transport failure")),
        WireError::Refused {
            code,
            message,
            data,
        } => {
            if code != 4001 {
                return refusal(
                    &format!("rpc_error_{code}"),
                    None,
                    Some(format!("{method}: {message}")),
                );
            }
            let reason = data["reason"].as_str().unwrap_or(&message);
            let field = data["field"].as_str();
            let detail = if let Some(fields) = data["fields"].as_array() {
                Some(format!(
                    "{method}: fields={}",
                    fields
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(",")
                ))
            } else {
                data["detail"].as_str().map(|d| format!("{method}: {d}"))
            };
            refusal(reason, field, detail)
        }
    }
}
fn decode<T: DeserializeOwned>(method: &str, value: Value) -> Result<T, PortError> {
    serde_json::from_value(value)
        .map_err(|e| refusal("unreadable_result", None, Some(format!("{method}: {e}"))))
}
fn cursor(snapshot: &CanonicalSnapshot) -> Cursor {
    Cursor {
        replay_epoch: snapshot.replay_epoch.clone(),
        last_sequence: snapshot.last_sequence as u64,
    }
}
fn required(describe: &RuntimeDescribeResult) -> Vec<String> {
    let value = serde_json::to_value(describe).unwrap_or_default();
    let mut missing = Vec::new();
    if !describe
        .capabilities
        .iter()
        .any(|c| c == "durable-admission-v1")
    {
        missing.push("durable-admission-v1".into());
    }
    if !value["session_create"]["sources"]
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v == "tool"))
    {
        missing.push("tool".into());
    }
    for key in CREATE_KEYS {
        if !value["session_create"]["parameters"]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v == key))
        {
            missing.push(key.into());
        }
    }
    missing
}
impl HermesPort {
    pub async fn connect(
        cfg: PortConfig,
        run_profile: &str,
    ) -> Result<(Self, AgentFeedReceiver), SetupError> {
        let factory = Arc::new(LinkWireFactory::new(cfg.home.clone()));
        Self::with_wires(cfg, factory, run_profile).await
    }
    pub async fn with_wires(
        cfg: PortConfig,
        wires: Arc<dyn WireFactory>,
        run_profile: &str,
    ) -> Result<(Self, AgentFeedReceiver), SetupError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let (updates, view) = watch::channel(View::default());
        let (feed, reader) = mpsc::unbounded_channel();
        let inner = Arc::new(Inner {
            tx,
            view,
            factory: wires,
            cfg,
            feed: feed.clone(),
            wakers: Mutex::new(BTreeMap::new()),
        });
        state::start(rx, updates, feed, Arc::downgrade(&inner));
        let port = Self { inner };
        port.wire(run_profile).await.map_err(|e| match e {
            PortError::Refused { reason, detail, .. } if reason == "unsupported" => {
                SetupError::Unsupported {
                    missing: detail
                        .unwrap_or_default()
                        .trim_start_matches("missing: ")
                        .split(", ")
                        .map(str::to_owned)
                        .collect(),
                }
            }
            PortError::Refused { reason, .. } if reason == "profile_unavailable" => {
                SetupError::NotFound(reason)
            }
            other => SetupError::Connect(other.to_string()),
        })?;
        Ok((port, reader))
    }
    async fn request<T>(
        &self,
        command: impl FnOnce(oneshot::Sender<T>) -> Command,
    ) -> Result<T, PortError> {
        let (tx, rx) = oneshot::channel();
        self.inner.tx.send(command(tx)).map_err(|_| transport())?;
        rx.await.map_err(|_| transport())
    }
    async fn call(
        &self,
        wire: &Arc<dyn Wire>,
        method: &'static str,
        params: Map<String, Value>,
        timeout: Duration,
    ) -> Result<Value, PortError> {
        self.request(|reply| Command::Count(method, reply)).await?;
        wire.call(method, params, timeout)
            .await
            .map_err(|e| map_wire_error(method, e))
    }
    async fn wire(&self, profile: &str) -> Result<Arc<dyn Wire>, PortError> {
        let profile = profile.to_owned();
        let slot = self
            .request(|reply| Command::Slot(profile.clone(), reply))
            .await?;
        let wire = slot
            .get_or_try_init(|| async {
                let wire = self
                    .inner
                    .factory
                    .open(&profile)
                    .await
                    .map_err(|e| map_wire_error("connect", e))?;
                let described = self
                    .call(
                        &wire,
                        "runtime.describe",
                        Map::new(),
                        self.inner.cfg.timeouts.describe,
                    )
                    .await?;
                let described: RuntimeDescribeResult = decode("runtime.describe", described)?;
                let missing = required(&described);
                if !missing.is_empty() {
                    return Err(refusal(
                        "unsupported",
                        None,
                        Some(format!("missing: {}", missing.join(", "))),
                    ));
                }
                self.request(|reply| Command::Ready(profile.clone(), wire.clone(), reply))
                    .await?;
                if let Some(mut events) = wire.take_events() {
                    let tx = self.inner.tx.clone();
                    let profile = profile.clone();
                    tokio::spawn(async move {
                        while let Some(event) = events.recv().await {
                            if tx
                                .send(Command::Event(profile.clone(), Box::new(event)))
                                .is_err()
                            {
                                break;
                            }
                        }
                    });
                }
                Ok(wire)
            })
            .await?;
        Ok(wire.clone())
    }
    async fn pending(&self, profile: &str) -> Result<Pending, PortError> {
        self.request(|reply| Command::Pending(profile.into(), reply))
            .await?;
        Ok(Pending {
            tx: self.inner.tx.clone(),
            profile: profile.into(),
        })
    }
    async fn bind(
        &self,
        profile: &str,
        snapshot: CanonicalSnapshot,
        binding: Binding,
        generation: u64,
    ) -> Result<(), PortError> {
        self.request(|reply| Command::Snapshot {
            profile: profile.into(),
            binding,
            snapshot: Box::new(snapshot),
            generation,
            reply,
        })
        .await
    }
    async fn save_receipt(
        &self,
        profile: &str,
        receipt: Receipt,
        submitted: bool,
    ) -> Result<Receipt, PortError> {
        self.request(|reply| Command::Receipt(profile.into(), receipt.clone(), submitted, reply))
            .await?;
        Ok(receipt)
    }
    async fn read_receipt(
        &self,
        profile: &str,
        session_id: &str,
        admission_id: &str,
        include_result: bool,
    ) -> Result<Receipt, PortError> {
        let wire = self.wire(profile).await?;
        let mut params = json!({"session_id":session_id,"admission_id":admission_id})
            .as_object()
            .cloned()
            .unwrap_or_default();
        if include_result {
            params.insert("include_result".into(), json!(true));
        }
        let timeout = if include_result {
            self.inner.cfg.timeouts.receipt
        } else {
            self.inner.cfg.timeouts.poll
        };
        let value = self.call(&wire, "prompt.receipt", params, timeout).await?;
        let receipt = parse_receipt("prompt.receipt", value, false)?;
        self.save_receipt(profile, receipt, false).await
    }
    pub(super) async fn resume_snapshot(
        &self,
        profile: &str,
        session_id: &str,
        resync: bool,
    ) -> Result<(), PortError> {
        let key = (profile.into(), session_id.into());
        let gate = self
            .request(|reply| Command::Gate(key.clone(), reply))
            .await?;
        let _guard = gate.lock().await;
        let wire = self.wire(profile).await?;
        let known = self
            .request(|reply| Command::Lookup(key.clone(), reply))
            .await?;
        if !resync
            && known
                .as_ref()
                .is_some_and(|s| s.resumed_on == wire.generation())
        {
            return Ok(());
        }
        let _pending = self.pending(profile).await?;
        let value = self
            .call(
                &wire,
                "session.resume",
                json!({"session_id":session_id})
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
                self.inner.cfg.timeouts.resume,
            )
            .await?;
        let snapshot: CanonicalSnapshot = decode("session.resume", value)?;
        if snapshot.session_id != session_id {
            let _ = self.inner.tx.send(Command::Event(
                profile.into(),
                Box::new(super::wire::WireEvent::Gone {
                    session_id: session_id.into(),
                }),
            ));
            return Err(refusal(
                "unreadable_result",
                None,
                Some("session.resume: snapshot names another session".into()),
            ));
        }
        let position = cursor(&snapshot);
        self.bind(
            profile,
            snapshot.clone(),
            Binding::default(),
            wire.generation(),
        )
        .await?;
        if resync {
            wire.resync(session_id, position);
            let _ = self.inner.feed.send(AgentFeedItem::Resynced {
                profile: profile.into(),
                session_id: session_id.into(),
                snapshot: Box::new(snapshot),
            });
        } else {
            wire.track(session_id, position);
        }
        Ok(())
    }
    pub fn session_info(&self, session_id: &str) -> Option<SessionInfo> {
        self.inner
            .view
            .borrow()
            .sessions
            .values()
            .find(|s| s.info.session_id == session_id)
            .map(|s| s.info.clone())
    }
    pub fn stored_session_id(&self, session_id: &str) -> Option<String> {
        self.inner
            .view
            .borrow()
            .sessions
            .values()
            .find(|s| s.info.session_id == session_id)
            .map(|s| s.stored_id.clone())
    }
    pub fn execution_generation(&self, session_id: &str) -> Option<i64> {
        self.inner
            .view
            .borrow()
            .sessions
            .values()
            .find(|s| s.info.session_id == session_id)
            .map(|s| s.execution_generation)
    }
    pub fn stats(&self) -> PortStats {
        let view = self.inner.view.borrow();
        let mut stats = view.stats;
        stats.dropped_items = view.wires.values().map(|w| w.dropped_items()).sum();
        stats
    }
    pub async fn live_session_ids(&self, profile: &str) -> Result<Vec<String>, PortError> {
        let wire = self.wire(profile).await?;
        let value = self
            .call(
                &wire,
                "session.list",
                json!({"scope":"live"})
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
                self.inner.cfg.timeouts.list,
            )
            .await?;
        let rows = value
            .as_array()
            .or_else(|| value["sessions"].as_array())
            .ok_or_else(|| {
                refusal(
                    "unreadable_result",
                    None,
                    Some("session.list: expected session rows".into()),
                )
            })?;
        Ok(rows
            .iter()
            .filter_map(|v| v["session_id"].as_str())
            .map(str::to_owned)
            .collect())
    }
}
fn parse_receipt(method: &str, value: Value, submit: bool) -> Result<Receipt, PortError> {
    if submit && value["voice_stopped"] == true {
        return Err(refusal(
            "voice_stopped",
            None,
            Some("Hermes treated the prompt as a voice stop".into()),
        ));
    }
    if submit {
        let _: AdmissionStatus = decode(method, value.clone())?;
    } else {
        let _: PromptReceiptResult = decode(method, value.clone())?;
    }
    let status = match value["status"].as_str() {
        Some("queued") => AdmissionState::Queued,
        Some("started") => AdmissionState::Started,
        Some("unknown") => AdmissionState::Unknown,
        Some("terminal") => AdmissionState::Terminal,
        _ => {
            return Err(refusal(
                "unreadable_result",
                None,
                Some(format!("{method}: invalid status")),
            ));
        }
    };
    let result = value.get("result").filter(|v| !v.is_null()).cloned();
    let mut usage = Map::new();
    for key in ["input_tokens", "output_tokens", "total_tokens"] {
        if let Some(v) = value["usage"].get(key) {
            usage.insert(key.into(), v.clone());
        }
    }
    let mut cost = Map::new();
    if let Some(result) = &result {
        for key in ["estimated_cost_usd", "cost_status", "cost_source"] {
            if let Some(v) = result.get(key) {
                cost.insert(key.into(), v.clone());
            }
        }
    }
    if !cost.is_empty() {
        usage.insert("session_cost".into(), Value::Object(cost));
    }
    let usage = (!usage.is_empty()).then_some(Value::Object(usage));
    Ok(Receipt {
        admission_id: value["admission_id"].as_str().unwrap_or_default().into(),
        session_id: value["ref"]["session_id"]
            .as_str()
            .unwrap_or_default()
            .into(),
        status,
        outcome: value["outcome"].as_str().map(str::to_owned),
        authority_epoch: value["authority_epoch"].as_i64().unwrap_or_default(),
        execution_generation: value["execution_generation"].as_i64(),
        result,
        usage,
    })
}
impl GatewayPort for HermesPort {
    fn create_session(
        &self,
        req: CreateSession,
    ) -> BoxFuture<'static, Result<SessionBound, PortError>> {
        let port = self.clone();
        Box::pin(async move {
            let params = create_params(&req.launch, &req.request_id)
                .map_err(|e| refusal("invalid_params", None, Some(e.to_string())))?;
            let profile = &req.launch.profile;
            let wire = port.wire(profile).await?;
            let _pending = port.pending(profile).await?;
            let result = port
                .call(
                    &wire,
                    "session.create",
                    params,
                    port.inner.cfg.timeouts.create,
                )
                .await?;
            let snapshot: CanonicalSnapshot = decode("session.create", result)?;
            let bound = SessionBound {
                session_id: snapshot.session_id.clone(),
                stored_session_id: snapshot.stored_session_id.clone(),
            };
            let position = cursor(&snapshot);
            port.bind(
                profile,
                snapshot,
                Binding {
                    request_id: Some(req.request_id),
                    label: Some(req.label),
                    expected_model: Some(req.launch.model),
                    expected_provider: req.launch.provider,
                },
                wire.generation(),
            )
            .await?;
            wire.track(&bound.session_id, position);
            Ok(bound)
        })
    }
    fn attach(
        &self,
        profile: String,
        session_id: String,
    ) -> BoxFuture<'static, Result<(), PortError>> {
        let port = self.clone();
        Box::pin(async move { port.resume_snapshot(&profile, &session_id, false).await })
    }
    fn submit(&self, req: SubmitTurn) -> BoxFuture<'static, Result<Receipt, PortError>> {
        let port = self.clone();
        Box::pin(async move {
            let params = submit_params(&req.session_id, &req.submission_id, &req.text)
                .map_err(|e| refusal("invalid_params", None, Some(e.to_string())))?;
            let wire = port.wire(&req.profile).await?;
            let value = port
                .call(
                    &wire,
                    "prompt.submit",
                    params,
                    port.inner.cfg.timeouts.submit,
                )
                .await?;
            let receipt = parse_receipt("prompt.submit", value, true)?;
            port.save_receipt(&req.profile, receipt, true).await
        })
    }
    fn receipt(
        &self,
        profile: String,
        session_id: String,
        admission_id: String,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        let port = self.clone();
        Box::pin(async move {
            port.read_receipt(&profile, &session_id, &admission_id, true)
                .await
        })
    }
    fn wait_settled(
        &self,
        profile: String,
        session_id: String,
        admission_id: String,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        let port = self.clone();
        Box::pin(async move {
            let mut events = port.inner.subscribe((profile.clone(), session_id.clone()));
            let mut delay = port.inner.cfg.poll.start;
            loop {
                let woke = tokio::select! {_=tokio::time::sleep(delay)=>false,changed=events.changed()=>changed.is_ok()};
                let receipt = port
                    .read_receipt(&profile, &session_id, &admission_id, false)
                    .await?;
                match receipt.status {
                    AdmissionState::Terminal => {
                        return port
                            .read_receipt(&profile, &session_id, &admission_id, true)
                            .await;
                    }
                    AdmissionState::Unknown => return Ok(receipt),
                    _ => {
                        delay = if woke {
                            port.inner.cfg.poll.start
                        } else {
                            delay.saturating_mul(2).min(port.inner.cfg.poll.cap)
                        }
                    }
                }
            }
        })
    }
    fn resolve_unknown(
        &self,
        profile: String,
        session_id: String,
        admission_id: String,
        execution_generation: i64,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        let port = self.clone();
        Box::pin(async move {
            let wire = port.wire(&profile).await?;
            let value=port.call(&wire,"prompt.resolve_unknown",json!({"session_id":session_id,"admission_id":admission_id,"execution_generation":execution_generation}).as_object().cloned().unwrap_or_default(),port.inner.cfg.timeouts.resolve_unknown).await?;
            let receipt = parse_receipt("prompt.resolve_unknown", value, false)?;
            port.save_receipt(&profile, receipt, false).await
        })
    }
}
impl HermesPort {
    async fn interrupt(
        &self,
        profile: &str,
        session_id: &str,
        generation: i64,
    ) -> Result<(), PortError> {
        let wire = self.wire(profile).await?;
        let value = self
            .call(
                &wire,
                "session.interrupt",
                json!({"session_id":session_id,"execution_generation":generation})
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
                self.inner.cfg.timeouts.interrupt,
            )
            .await?;
        let _: CanonicalSessionHandle = decode("session.interrupt", value)?;
        Ok(())
    }
    pub async fn stop(&self, profile: &str, session_id: &str) -> StopResult {
        let Ok(known) = self
            .request(|reply| Command::Lookup((profile.into(), session_id.into()), reply))
            .await
        else {
            return StopResult::Unknown;
        };
        let Some(receipt) = known.and_then(|s| s.latest) else {
            return StopResult::NotRunning;
        };
        match receipt.status {
            AdmissionState::Terminal => StopResult::NotRunning,
            AdmissionState::Unknown => StopResult::Unknown,
            AdmissionState::Started => {
                let Some(g) = receipt.execution_generation else {
                    return StopResult::Unknown;
                };
                match self.interrupt(profile, session_id, g).await {
                    Ok(()) => StopResult::Sent,
                    Err(PortError::Refused { reason, .. }) if reason == "stale_generation" => {
                        match self
                            .read_receipt(profile, session_id, &receipt.admission_id, false)
                            .await
                        {
                            Ok(r)
                                if r.status == AdmissionState::Terminal
                                    || r.status == AdmissionState::Started
                                        && r.execution_generation != Some(g) =>
                            {
                                StopResult::NotRunning
                            }
                            _ => StopResult::Unknown,
                        }
                    }
                    Err(_) => StopResult::Unknown,
                }
            }
            AdmissionState::Queued => {
                let Ok(wire) = self.wire(profile).await else {
                    return StopResult::Unknown;
                };
                let answer = self
                    .call(
                        &wire,
                        "prompt.cancel",
                        json!({"session_id":session_id,"admission_id":receipt.admission_id})
                            .as_object()
                            .cloned()
                            .unwrap_or_default(),
                        self.inner.cfg.timeouts.cancel,
                    )
                    .await;
                let answer = match answer {
                    Ok(value) => parse_receipt("prompt.cancel", value, false),
                    Err(PortError::Refused { reason, .. }) if reason == "stale_generation" => {
                        self.read_receipt(profile, session_id, &receipt.admission_id, false)
                            .await
                    }
                    Err(e) => Err(e),
                };
                match answer {
                    Ok(r) => {
                        let _ = self.save_receipt(profile, r.clone(), false).await;
                        match r.status {
                            AdmissionState::Terminal => {
                                if r.outcome.as_deref() == Some("cancelled") {
                                    StopResult::Sent
                                } else {
                                    StopResult::NotRunning
                                }
                            }
                            AdmissionState::Started => {
                                if let Some(g) = r.execution_generation {
                                    if self.interrupt(profile, session_id, g).await.is_ok() {
                                        StopResult::Sent
                                    } else {
                                        StopResult::Unknown
                                    }
                                } else {
                                    StopResult::Unknown
                                }
                            }
                            _ => StopResult::Unknown,
                        }
                    }
                    Err(_) => StopResult::Unknown,
                }
            }
        }
    }
}
