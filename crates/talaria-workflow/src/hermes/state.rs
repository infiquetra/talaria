//! One actor owns profile connections, session metadata and feed ordering.
use super::super::{
    feed::AgentFeedItem,
    port::{HermesPort, Inner, PortStats, SessionInfo},
    wire::{Cursor, Wire, WireEvent},
};
use crate::journal::Receipt;
use hermes_contract::schemas::CanonicalSnapshot;
use std::{
    collections::BTreeMap,
    sync::{Arc, Weak},
};
use tokio::sync::{Mutex, OnceCell, mpsc, oneshot, watch};

pub(super) type Key = (String, String);
pub(super) type Slot = Arc<OnceCell<Arc<dyn Wire>>>;
#[derive(Clone, Default)]
pub(super) struct Warnings {
    config: bool,
    model: bool,
    provider: bool,
}
#[derive(Clone)]
pub(super) struct Session {
    pub info: SessionInfo,
    pub stored_id: String,
    pub execution_generation: i64,
    pub resumed_on: u64,
    pub cursor: Option<Cursor>,
    pub latest: Option<Receipt>,
    pub recovering: bool,
    pub warnings: Warnings,
    pub observing: bool,
    pub expected_model: Option<String>,
    pub expected_provider: Option<String>,
}
#[derive(Clone, Default)]
pub(super) struct View {
    pub sessions: BTreeMap<Key, Session>,
    pub stats: PortStats,
    pub wires: BTreeMap<String, Arc<dyn Wire>>,
}
#[derive(Default)]
pub(super) struct Binding {
    pub request_id: Option<String>,
    pub label: Option<String>,
    pub expected_model: Option<String>,
    pub expected_provider: Option<String>,
}
pub(super) enum Command {
    Slot(String, oneshot::Sender<Slot>),
    Ready(String, Arc<dyn Wire>, oneshot::Sender<()>),
    Gate(Key, oneshot::Sender<Arc<Mutex<()>>>),
    Lookup(Key, oneshot::Sender<Option<Session>>),
    Pending(String, oneshot::Sender<()>),
    EndPending(String),
    Snapshot {
        profile: String,
        binding: Binding,
        snapshot: Box<CanonicalSnapshot>,
        generation: u64,
        reply: oneshot::Sender<()>,
    },
    Receipt(String, Receipt, bool, oneshot::Sender<()>),
    Count(&'static str, oneshot::Sender<()>),
    Event(String, Box<WireEvent>),
    RecoveryDone(Key),
    Shutdown,
}
struct State {
    view: View,
    slots: BTreeMap<String, Slot>,
    gates: BTreeMap<Key, Arc<Mutex<()>>>,
    pending: BTreeMap<String, u32>,
    held: BTreeMap<Key, Vec<WireEvent>>,
    updates: watch::Sender<View>,
    feed: mpsc::UnboundedSender<AgentFeedItem>,
    owner: Weak<Inner>,
}
impl State {
    fn publish(&self) {
        self.updates.send_replace(self.view.clone());
    }
    fn event(&mut self, profile: String, event: WireEvent) {
        let session_id = match &event {
            WireEvent::Event { session_id, .. }
            | WireEvent::Gap { session_id, .. }
            | WireEvent::Gone { session_id }
            | WireEvent::Reattached { session_id } => session_id.clone(),
            WireEvent::Reconnected { .. } => return,
        };
        let key = (profile.clone(), session_id.clone());
        let Some(session) = self.view.sessions.get_mut(&key) else {
            if self.pending.get(&profile).copied().unwrap_or(0) > 0 {
                self.held.entry(key).or_default().push(event);
            }
            return;
        };
        if !session.observing {
            if self.pending.get(&profile).copied().unwrap_or(0) > 0 {
                self.held.entry(key).or_default().push(event);
            }
            return;
        }
        match event {
            WireEvent::Event { frame, .. } => {
                if frame.type_name() == Some("message.complete")
                    && let Some(owner) = self.owner.upgrade()
                {
                    owner.wake(&key);
                }
                if let (Some(seq), Some(epoch)) = (frame.seq, frame.replay_epoch.clone()) {
                    session.cursor = Some(Cursor {
                        last_sequence: seq,
                        replay_epoch: epoch,
                    });
                }
                let _ = self.feed.send(AgentFeedItem::Event {
                    profile,
                    session_id,
                    request_id: session.info.request_id.clone(),
                    label: session.info.label.clone(),
                    frame,
                });
            }
            WireEvent::Gap { reason, .. } => {
                let _ = self.feed.send(AgentFeedItem::Gap {
                    profile: profile.clone(),
                    session_id: session_id.clone(),
                    reason,
                });
                if !session.recovering {
                    session.recovering = true;
                    if let Some(inner) = self.owner.upgrade() {
                        let port = HermesPort { inner };
                        tokio::spawn(async move {
                            let _ = port.resume_snapshot(&profile, &session_id, true).await;
                            let _ = port
                                .inner
                                .tx
                                .send(Command::RecoveryDone((profile, session_id)));
                        });
                    }
                }
            }
            WireEvent::Gone { .. } => {
                session.cursor = None;
                session.resumed_on = 0;
                let _ = self.feed.send(AgentFeedItem::Gone {
                    profile,
                    session_id,
                });
            }
            WireEvent::Reattached { .. } => {
                session.resumed_on = self
                    .view
                    .wires
                    .get(&profile)
                    .map_or(0, |wire| wire.generation());
            }
            WireEvent::Reconnected { .. } => {}
        }
    }
    fn snapshot(
        &mut self,
        profile: String,
        snapshot: CanonicalSnapshot,
        generation: u64,
        binding: Binding,
    ) {
        let Binding {
            request_id,
            label,
            expected_model,
            expected_provider,
        } = binding;
        let key = (profile.clone(), snapshot.session_id.clone());
        let previous = self.view.sessions.get(&key);
        let expected_model =
            expected_model.or_else(|| previous.and_then(|s| s.expected_model.clone()));
        let expected_provider =
            expected_provider.or_else(|| previous.and_then(|s| s.expected_provider.clone()));
        let mut warnings = previous
            .map(|session| session.warnings.clone())
            .unwrap_or_default();
        let raw = serde_json::to_value(&snapshot.info).unwrap_or_default();
        let string = |name: &str| raw[name].as_str().map(str::to_owned);
        let info = SessionInfo {
            session_id: snapshot.session_id.clone(),
            request_id: request_id
                .or_else(|| previous.map(|s| s.info.request_id.clone()))
                .unwrap_or_default(),
            label: label
                .or_else(|| previous.map(|s| s.info.label.clone()))
                .unwrap_or_default(),
            source: string("source"),
            model: string("model"),
            provider: string("provider"),
            reasoning_effort: string("reasoning_effort"),
        };
        if !warnings.config
            && raw["config_warning"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
        {
            warnings.config = true;
            tracing::warn!(
                config_warning = true,
                "Hermes reported a configuration warning"
            );
        }
        if !warnings.model
            && expected_model
                .as_ref()
                .is_some_and(|m| Some(m) != info.model.as_ref())
        {
            warnings.model = true;
            tracing::warn!("Hermes model differs from the requested model");
        }
        if !warnings.provider
            && expected_provider
                .as_ref()
                .is_some_and(|p| Some(p) != info.provider.as_ref())
        {
            warnings.provider = true;
            tracing::warn!("Hermes provider differs from the requested provider");
        }
        let latest = previous.and_then(|s| s.latest.clone());
        self.view.sessions.insert(
            key.clone(),
            Session {
                info,
                stored_id: snapshot.stored_session_id,
                execution_generation: snapshot.execution_generation,
                resumed_on: generation,
                cursor: Some(Cursor {
                    replay_epoch: snapshot.replay_epoch,
                    last_sequence: snapshot.last_sequence as u64,
                }),
                latest,
                recovering: false,
                warnings,
                observing: true,
                expected_model,
                expected_provider,
            },
        );
        if let Some(events) = self.held.remove(&key) {
            for event in events {
                self.event(profile.clone(), event);
            }
        }
    }
}
pub(super) fn start(
    mut rx: mpsc::UnboundedReceiver<Command>,
    updates: watch::Sender<View>,
    feed: mpsc::UnboundedSender<AgentFeedItem>,
    owner: Weak<Inner>,
) {
    tokio::spawn(async move {
        let mut state = State {
            view: View::default(),
            slots: BTreeMap::new(),
            gates: BTreeMap::new(),
            pending: BTreeMap::new(),
            held: BTreeMap::new(),
            updates,
            feed,
            owner,
        };
        while let Some(command) = rx.recv().await {
            match command {
                Command::Slot(profile, reply) => {
                    let slot = state.slots.entry(profile).or_default().clone();
                    let _ = reply.send(slot);
                }
                Command::Ready(profile, wire, reply) => {
                    state.view.wires.insert(profile, wire);
                    state.publish();
                    let _ = reply.send(());
                }
                Command::Gate(key, reply) => {
                    let gate = state
                        .gates
                        .entry(key)
                        .or_insert_with(|| Arc::new(Mutex::new(())))
                        .clone();
                    let _ = reply.send(gate);
                }
                Command::Lookup(key, reply) => {
                    let _ = reply.send(state.view.sessions.get(&key).cloned());
                }
                Command::Pending(profile, reply) => {
                    *state.pending.entry(profile).or_default() += 1;
                    let _ = reply.send(());
                }
                Command::EndPending(profile) => {
                    if let Some(count) = state.pending.get_mut(&profile) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            state.held.retain(|(p, _), _| p != &profile);
                        }
                    }
                }
                Command::Snapshot {
                    profile,
                    binding,
                    snapshot,
                    generation,
                    reply,
                } => {
                    state.snapshot(profile, *snapshot, generation, binding);
                    state.publish();
                    let _ = reply.send(());
                }
                Command::Receipt(profile, receipt, submitted, reply) => {
                    let session = state
                        .view
                        .sessions
                        .entry((profile, receipt.session_id.clone()))
                        .or_insert_with(|| Session {
                            info: SessionInfo {
                                session_id: receipt.session_id.clone(),
                                request_id: String::new(),
                                label: String::new(),
                                source: None,
                                model: None,
                                provider: None,
                                reasoning_effort: None,
                            },
                            stored_id: String::new(),
                            execution_generation: receipt.execution_generation.unwrap_or(0),
                            resumed_on: 0,
                            cursor: None,
                            latest: None,
                            recovering: false,
                            warnings: Warnings::default(),
                            observing: false,
                            expected_model: None,
                            expected_provider: None,
                        });
                    if submitted
                        || session
                            .latest
                            .as_ref()
                            .is_none_or(|latest| latest.admission_id == receipt.admission_id)
                    {
                        if let Some(generation) = receipt.execution_generation {
                            session.execution_generation = generation;
                        }
                        session.latest = Some(receipt);
                    }
                    state.publish();
                    let _ = reply.send(());
                }
                Command::Count(method, reply) => {
                    let s = &mut state.view.stats;
                    match method {
                        "runtime.describe" => s.runtime_describe += 1,
                        "session.create" => s.session_create += 1,
                        "session.resume" => s.session_resume += 1,
                        "prompt.submit" => s.prompt_submit += 1,
                        "prompt.receipt" => s.prompt_receipt += 1,
                        "prompt.cancel" => s.prompt_cancel += 1,
                        "session.interrupt" => s.session_interrupt += 1,
                        "prompt.resolve_unknown" => s.prompt_resolve_unknown += 1,
                        "session.list" => s.session_list += 1,
                        _ => {}
                    }
                    state.publish();
                    let _ = reply.send(());
                }
                Command::Event(profile, event) => {
                    state.event(profile, *event);
                    state.publish();
                }
                Command::RecoveryDone(key) => {
                    if let Some(session) = state.view.sessions.get_mut(&key) {
                        session.recovering = false;
                    }
                    state.publish();
                }
                Command::Shutdown => break,
            }
        }
    });
}
