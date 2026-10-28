//! Scripted, profile-scoped gateway conversations for offline binding tests.
use super::wire::{Cursor, EventFrame, Wire, WireError, WireEvent, WireFactory};
use futures::future::BoxFuture;
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

#[derive(Clone, Debug)]
pub struct OutboundFrame {
    pub profile: String,
    pub method: String,
    pub params: Map<String, Value>,
}
#[derive(Clone, Debug)]
pub struct TrackRecord {
    pub profile: String,
    pub session_id: String,
    pub cursor: Cursor,
    pub resync: bool,
}
#[derive(Default)]
struct History {
    outbound: Vec<OutboundFrame>,
    tracks: Vec<TrackRecord>,
}
enum Command {
    Open(String, oneshot::Sender<Result<Arc<dyn Wire>, WireError>>),
    Call {
        profile: String,
        method: &'static str,
        params: Map<String, Value>,
        reply: oneshot::Sender<Result<Value, WireError>>,
    },
    Track(String, String, Cursor),
    Emit(String, Box<WireEvent>),
    Shutdown,
}
pub struct ScriptedWire {
    tx: mpsc::UnboundedSender<Command>,
    history: Arc<Mutex<History>>,
}
impl Drop for ScriptedWire {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Shutdown);
    }
}
struct ProfileWire {
    profile: String,
    tx: mpsc::UnboundedSender<Command>,
    generation: Arc<AtomicU64>,
    events: Mutex<Option<mpsc::Receiver<WireEvent>>>,
    history: Arc<Mutex<History>>,
}
struct Profile {
    steps: VecDeque<Value>,
    wire: Arc<ProfileWire>,
    events: mpsc::Sender<WireEvent>,
    tracked: BTreeSet<String>,
    reconnect: bool,
    gaps: BTreeSet<String>,
    barrier: Vec<PendingReply>,
}
type PendingReply = (
    oneshot::Sender<Result<Value, WireError>>,
    Result<Value, WireError>,
);
impl ScriptedWire {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|_| "script is unreadable")?;
        let script = serde_json::from_str(&text).map_err(|_| "script is not JSON")?;
        let parent = path.parent().ok_or("script parent missing")?;
        let root = if parent.file_name().is_some_and(|s| s == "scripts") {
            parent.parent().unwrap_or(parent)
        } else {
            parent
        };
        Self::from_value(root, script)
    }
    pub fn from_file(path: &Path) -> Result<Self, String> {
        Self::load(path)
    }
    pub fn from_value(root: &Path, script: Value) -> Result<Self, String> {
        let scripts = if let Some(steps) = script.as_array() {
            BTreeMap::from([("fixture".into(), steps.clone())])
        } else if let Some(steps) = script["steps"].as_array() {
            BTreeMap::from([("fixture".into(), steps.clone())])
        } else {
            let object = script
                .get("profiles")
                .unwrap_or(&script)
                .as_object()
                .ok_or("script must contain profiles or steps")?;
            let mut profiles = BTreeMap::new();
            for (profile, steps) in object {
                profiles.insert(
                    profile.clone(),
                    steps
                        .as_array()
                        .or_else(|| steps["steps"].as_array())
                        .ok_or("profile steps must be an array")?
                        .clone(),
                );
            }
            profiles
        };
        for steps in scripts.values() {
            for step in steps {
                if step["expect"].as_str().is_none() || step.get("reply").is_none() {
                    return Err("script step needs expect and reply".into());
                }
            }
        }
        let (tx, mut rx) = mpsc::unbounded_channel();
        let history = Arc::new(Mutex::new(History::default()));
        let mut profiles = BTreeMap::new();
        for (profile, steps) in scripts {
            let (events, receiver) = mpsc::channel(16_384);
            let wire = Arc::new(ProfileWire {
                profile: profile.clone(),
                tx: tx.clone(),
                generation: Arc::new(AtomicU64::new(1)),
                events: Mutex::new(Some(receiver)),
                history: history.clone(),
            });
            profiles.insert(
                profile,
                Profile {
                    steps: steps.into(),
                    wire,
                    events,
                    tracked: BTreeSet::new(),
                    reconnect: false,
                    gaps: BTreeSet::new(),
                    barrier: Vec::new(),
                },
            );
        }
        let root = root.to_owned();
        tokio::spawn(async move {
            type CacheKey = (String, String, String, String);
            type CachedReply = (Map<String, Value>, Value);
            let mut cache: BTreeMap<CacheKey, CachedReply> = BTreeMap::new();
            while let Some(command) = rx.recv().await {
                match command {
                    Command::Shutdown => break,
                    Command::Open(profile, reply) => {
                        let result = profiles
                            .get(&profile)
                            .map(|p| p.wire.clone() as Arc<dyn Wire>)
                            .ok_or_else(|| WireError::NotServed("profile is not scripted".into()));
                        let _ = reply.send(result);
                    }
                    Command::Track(profile, session, _cursor) => {
                        if let Some(state) = profiles.get_mut(&profile) {
                            state.tracked.insert(session);
                        }
                    }
                    Command::Emit(profile, event) => {
                        if let Some(state) = profiles.get(&profile) {
                            let _ = state.events.send(*event).await;
                        }
                    }
                    Command::Call {
                        profile,
                        method,
                        params,
                        reply,
                    } => {
                        let Some(state) = profiles.get_mut(&profile) else {
                            let _ = reply
                                .send(Err(WireError::NotServed("profile is not scripted".into())));
                            continue;
                        };
                        if state.reconnect {
                            state.reconnect = false;
                            let generation =
                                state.wire.generation.fetch_add(1, Ordering::SeqCst) + 1;
                            let _ = state
                                .events
                                .send(WireEvent::Reconnected { generation })
                                .await;
                            for session in &state.tracked {
                                let event = if state.gaps.remove(session) {
                                    WireEvent::Gap {
                                        session_id: session.clone(),
                                        reason: "replay_failed".into(),
                                    }
                                } else {
                                    WireEvent::Reattached {
                                        session_id: session.clone(),
                                    }
                                };
                                let _ = state.events.send(event).await;
                            }
                        }
                        let id = if method == "session.create" {
                            params.get("request_id")
                        } else if method == "prompt.submit" {
                            params.get("submission_id")
                        } else {
                            None
                        }
                        .and_then(Value::as_str);
                        let key = id.map(|id| {
                            (
                                profile.clone(),
                                method.to_owned(),
                                if method == "prompt.submit" {
                                    params["session_id"].as_str().unwrap_or_default().to_owned()
                                } else {
                                    String::new()
                                },
                                id.to_owned(),
                            )
                        });
                        if let Some(key) = &key
                            && let Some((previous, response)) = cache.get(key)
                        {
                            let answer = if *previous == params {
                                Ok(response.clone())
                            } else {
                                Err(WireError::Refused {
                                    code: 4001,
                                    message: "conflicting request".into(),
                                    data: if method == "session.create" {
                                        json!({"reason":"invalid_params","field":"request_id","detail":"a resumed request_id must carry the same frozen request"})
                                    } else {
                                        json!({"reason":"admission_conflict"})
                                    },
                                })
                            };
                            let _ = reply.send(answer);
                            continue;
                        }
                        let offset = state
                            .steps
                            .front()
                            .and_then(|step| step["barrier"].as_str())
                            .and_then(|barrier| {
                                state
                                    .steps
                                    .iter()
                                    .take_while(|step| step["barrier"] == barrier)
                                    .position(|step| matches_step(step, method, &params))
                            })
                            .unwrap_or(0);
                        let Some(step) = state.steps.remove(offset) else {
                            let _ = reply.send(Err(WireError::Other(format!(
                                "script has no step for {method}"
                            ))));
                            continue;
                        };
                        if step["expect"] != method
                            || !step["match"]
                                .as_object()
                                .is_none_or(|m| m.iter().all(|(k, v)| params.get(k) == Some(v)))
                        {
                            let _ = reply.send(Err(WireError::Other(format!(
                                "script expected {}, received {method}",
                                step["expect"]
                            ))));
                            continue;
                        }
                        let response = read_value(&root, &step["reply"])
                            .and_then(|v| substitute(v, &params))
                            .and_then(|v| {
                                if let Some(e) = v.get("error") {
                                    Err(WireError::Refused {
                                        code: e["code"].as_i64().unwrap_or(4001),
                                        message: e["message"].as_str().unwrap_or("refused").into(),
                                        data: e.get("data").cloned().unwrap_or(Value::Null),
                                    })
                                } else {
                                    Ok(v)
                                }
                            });
                        if let (Ok(value), Some(key)) = (&response, key) {
                            cache.insert(key, (params, value.clone()));
                        }
                        if step["close_before_reply"] == true {
                            state.reconnect = true;
                            drop(reply);
                            continue;
                        }
                        state.reconnect = step["drop_after"] == true;
                        state.gaps = step["reconnect_gap"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect();
                        if let Some(barrier) = step["barrier"].as_str() {
                            state.barrier.push((reply, response));
                            if state.steps.front().and_then(|s| s["barrier"].as_str())
                                != Some(barrier)
                            {
                                for (reply, response) in state.barrier.drain(..) {
                                    let _ = reply.send(response);
                                }
                            }
                        } else {
                            let _ = reply.send(response);
                        }
                        for event in step["events"].as_array().into_iter().flatten() {
                            if let Ok(value) = read_value(&root, event) {
                                let params = value.get("params").unwrap_or(&value);
                                if let Ok(frame) = EventFrame::from_params(params)
                                    && let Some(session_id) = frame.session_id.clone()
                                {
                                    let _ = state
                                        .events
                                        .send(WireEvent::Event { session_id, frame })
                                        .await;
                                }
                            }
                        }
                    }
                }
            }
        });
        Ok(Self { tx, history })
    }
    pub fn outbound(&self) -> Vec<OutboundFrame> {
        self.history
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .outbound
            .clone()
    }
    pub fn log(&self) -> Vec<OutboundFrame> {
        self.outbound()
    }
    pub fn tracks(&self) -> Vec<TrackRecord> {
        self.history
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .tracks
            .clone()
    }
    pub fn emit(&self, profile: &str, event: WireEvent) {
        let _ = self.tx.send(Command::Emit(profile.into(), Box::new(event)));
    }
}
fn matches_step(step: &Value, method: &str, params: &Map<String, Value>) -> bool {
    step["expect"] == method
        && step["match"].as_object().is_none_or(|expected| {
            expected
                .iter()
                .all(|(key, value)| params.get(key) == Some(value))
        })
}
fn substitute(value: Value, params: &Map<String, Value>) -> Result<Value, WireError> {
    match value {
        Value::String(s) => {
            if let Some(key) = s.strip_prefix("$param:") {
                params
                    .get(key)
                    .cloned()
                    .ok_or_else(|| WireError::Other("reply references a missing parameter".into()))
            } else {
                Ok(Value::String(s))
            }
        }
        Value::Array(items) => items
            .into_iter()
            .map(|item| substitute(item, params))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(items) => items
            .into_iter()
            .map(|(key, value)| Ok((key, substitute(value, params)?)))
            .collect::<Result<Map<_, _>, WireError>>()
            .map(Value::Object),
        other => Ok(other),
    }
}
fn read_value(root: &Path, input: &Value) -> Result<Value, WireError> {
    if let Some(name) = input.as_str() {
        let path = PathBuf::from(name);
        if path.is_absolute()
            || path
                .components()
                .any(|p| matches!(p, std::path::Component::ParentDir))
        {
            return Err(WireError::Other("fixture path must be relative".into()));
        }
        let path = if root.join(&path).is_file() {
            root.join(path)
        } else {
            root.join("wire").join(path)
        };
        let bytes =
            std::fs::read(path).map_err(|_| WireError::Other("fixture is unreadable".into()))?;
        serde_json::from_slice(&bytes).map_err(|_| WireError::Other("fixture is not JSON".into()))
    } else {
        Ok(input.clone())
    }
}
impl WireFactory for ScriptedWire {
    fn open(&self, profile: &str) -> BoxFuture<'static, Result<Arc<dyn Wire>, WireError>> {
        let tx = self.tx.clone();
        let profile = profile.to_owned();
        Box::pin(async move {
            let (reply, rx) = oneshot::channel();
            tx.send(Command::Open(profile, reply))
                .map_err(|_| WireError::Closed)?;
            rx.await.map_err(|_| WireError::Closed)?
        })
    }
}
impl Wire for ProfileWire {
    fn call(
        &self,
        method: &'static str,
        params: Map<String, Value>,
        timeout: Duration,
    ) -> BoxFuture<'static, Result<Value, WireError>> {
        let tx = self.tx.clone();
        let profile = self.profile.clone();
        let history = self.history.clone();
        Box::pin(async move {
            history
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .outbound
                .push(OutboundFrame {
                    profile: profile.clone(),
                    method: method.into(),
                    params: params.clone(),
                });
            let (reply, rx) = oneshot::channel();
            tx.send(Command::Call {
                profile,
                method,
                params,
                reply,
            })
            .map_err(|_| WireError::Closed)?;
            tokio::time::timeout(timeout, rx)
                .await
                .map_err(|_| WireError::Timeout)?
                .map_err(|_| WireError::Closed)?
        })
    }
    fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
    fn track(&self, session_id: &str, cursor: Cursor) {
        self.history
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .tracks
            .push(TrackRecord {
                profile: self.profile.clone(),
                session_id: session_id.into(),
                cursor: cursor.clone(),
                resync: false,
            });
        let _ = self.tx.send(Command::Track(
            self.profile.clone(),
            session_id.into(),
            cursor,
        ));
    }
    fn resync(&self, session_id: &str, cursor: Cursor) {
        self.history
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .tracks
            .push(TrackRecord {
                profile: self.profile.clone(),
                session_id: session_id.into(),
                cursor: cursor.clone(),
                resync: true,
            });
        let _ = self.tx.send(Command::Track(
            self.profile.clone(),
            session_id.into(),
            cursor,
        ));
    }
    fn take_events(&self) -> Option<mpsc::Receiver<WireEvent>> {
        self.events.lock().ok()?.take()
    }
}
