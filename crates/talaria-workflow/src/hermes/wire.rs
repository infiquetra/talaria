//! Profile-bound transport adapters; connection recovery stays with the link.
use futures::future::BoxFuture;
use hermes_client::liveness::{ConnectHook, GatewayDial, Link, LinkConfig, LinkEvent, LinkState};
pub use hermes_client::replay::Cursor;
use hermes_client::{CallError, CallOptions, Inbound};
pub use hermes_contract::events::EventFrame;
use serde_json::{Map, Value};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::{mpsc, watch};

pub trait Wire: Send + Sync + 'static {
    fn call(
        &self,
        method: &'static str,
        params: Map<String, Value>,
        timeout: Duration,
    ) -> BoxFuture<'static, Result<Value, WireError>>;
    fn generation(&self) -> u64;
    fn track(&self, session_id: &str, cursor: Cursor);
    fn resync(&self, session_id: &str, cursor: Cursor);
    fn take_events(&self) -> Option<mpsc::Receiver<WireEvent>>;
    fn dropped_items(&self) -> u64 {
        0
    }
}
pub trait WireFactory: Send + Sync + 'static {
    fn open(&self, profile: &str) -> BoxFuture<'static, Result<Arc<dyn Wire>, WireError>>;
}
#[derive(Clone, Debug)]
// The public event shape retains the decoded contract payload.
#[allow(clippy::large_enum_variant)]
pub enum WireEvent {
    Event {
        session_id: String,
        frame: EventFrame,
    },
    Gap {
        session_id: String,
        reason: String,
    },
    Gone {
        session_id: String,
    },
    Reattached {
        session_id: String,
    },
    Reconnected {
        generation: u64,
    },
}
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum WireError {
    #[error("gateway RPC refused with code {code}")]
    Refused {
        code: i64,
        message: String,
        data: Value,
    },
    #[error("connection lost")]
    Closed,
    #[error("gateway timeout")]
    Timeout,
    #[error("profile unavailable")]
    NotServed(String),
    #[error("wire failure: {0}")]
    Other(String),
}
#[derive(Clone)]
pub struct LinkWireFactory {
    home: PathBuf,
    hook: Option<ConnectHook>,
}
impl LinkWireFactory {
    pub fn new(home: PathBuf) -> Self {
        Self { home, hook: None }
    }
    pub fn on_connect(mut self, hook: ConnectHook) -> Self {
        self.hook = Some(hook);
        self
    }
}
impl WireFactory for LinkWireFactory {
    fn open(&self, profile: &str) -> BoxFuture<'static, Result<Arc<dyn Wire>, WireError>> {
        let home = self.home.clone();
        let profile = profile.to_owned();
        let hook = self.hook.clone();
        Box::pin(async move {
            let mut dial = GatewayDial::new(home, profile);
            if let Some(hook) = hook {
                dial = dial.on_connect(hook);
            }
            let (link, events) = Link::start(dial, LinkConfig::default())
                .await
                .map_err(|e| WireError::NotServed(e.reason().into()))?;
            Ok(Arc::new(LinkWire::new(link, events)) as Arc<dyn Wire>)
        })
    }
}
enum Control {
    Track(String, Cursor),
    Resync(String, Cursor),
    Close,
}
#[derive(Clone, Debug)]
enum Connection {
    Connected,
    Waiting,
    Failed(String),
    Closed,
}
pub struct LinkWire {
    client: hermes_client::Client,
    control: mpsc::UnboundedSender<Control>,
    state: watch::Receiver<Connection>,
    generation: Arc<AtomicU64>,
    dropped: Arc<AtomicU64>,
    events: Mutex<Option<mpsc::Receiver<WireEvent>>>,
}
impl LinkWire {
    pub fn new(link: Link, mut inbound: mpsc::Receiver<LinkEvent>) -> Self {
        let client = link.client();
        let (control, mut commands) = mpsc::unbounded_channel();
        let (events, rx) = mpsc::channel(16_384);
        let (state, connection) = watch::channel(Connection::Connected);
        let generation = Arc::new(AtomicU64::new(1));
        let dropped = Arc::new(AtomicU64::new(0));
        let epoch = generation.clone();
        let bad = dropped.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    command=commands.recv()=>match command {Some(Control::Track(s,c))=>link.track(&s,c),Some(Control::Resync(s,c))=>link.resync(&s,c),Some(Control::Close)|None=>break},
                    event=inbound.recv()=>{
                        let Some(event)=event else{break;};
                        let mapped=match event {
                            LinkEvent::State(LinkState::Connected{generation})=>{epoch.store(generation,Ordering::SeqCst);state.send_replace(Connection::Connected);(generation>1).then_some(WireEvent::Reconnected{generation})},
                            LinkEvent::State(LinkState::Reconnecting{..})=>{state.send_replace(Connection::Waiting);None},
                            LinkEvent::State(LinkState::Failed{reason,..})=>{state.send_replace(Connection::Failed(reason));None},
                            LinkEvent::State(LinkState::Closed)=>{state.send_replace(Connection::Closed);None},
                            LinkEvent::Gap(gap)=>Some(WireEvent::Gap{session_id:gap.session_id,reason:gap.reason.as_str().into()}),
                            LinkEvent::SessionGone{session_id}=>Some(WireEvent::Gone{session_id}),
                            LinkEvent::Reattached{session_id,..}=>Some(WireEvent::Reattached{session_id}),
                            LinkEvent::Inbound{item:Inbound::Event(event),..}=>event.frame.session_id.clone().map(|session_id|WireEvent::Event{session_id,frame:event.frame}),
                            LinkEvent::Inbound{item:Inbound::ServerRequest(request),..}=>{bad.fetch_add(1,Ordering::Relaxed);request.responder.ignore();None},
                            LinkEvent::Inbound{..}=>{bad.fetch_add(1,Ordering::Relaxed);None},
                        };
                        if let Some(event)=mapped&&events.send(event).await.is_err(){break;}
                    }
                }
            }
            state.send_replace(Connection::Closed);
            link.close().await;
        });
        Self {
            client,
            control,
            state: connection,
            generation,
            dropped,
            events: Mutex::new(Some(rx)),
        }
    }
}
impl Drop for LinkWire {
    fn drop(&mut self) {
        let _ = self.control.send(Control::Close);
    }
}
impl Wire for LinkWire {
    fn call(
        &self,
        method: &'static str,
        params: Map<String, Value>,
        timeout: Duration,
    ) -> BoxFuture<'static, Result<Value, WireError>> {
        let client = self.client.clone();
        let mut state = self.state.clone();
        Box::pin(async move {
            tokio::time::timeout(timeout, async move {
                loop {
                    let current = state.borrow().clone();
                    match current {
                        Connection::Connected => break,
                        Connection::Failed(reason) => return Err(WireError::NotServed(reason)),
                        Connection::Closed => return Err(WireError::Closed),
                        Connection::Waiting => {
                            state.changed().await.map_err(|_| WireError::Closed)?;
                        }
                    }
                }
                client
                    .call_raw(
                        method,
                        Value::Object(params),
                        CallOptions {
                            timeout: Some(timeout),
                        },
                    )
                    .await
                    .map_err(|e| match e {
                        CallError::Rpc(e) => WireError::Refused {
                            code: e.code,
                            message: e.message,
                            data: e.data.unwrap_or(Value::Null),
                        },
                        CallError::TimedOut { .. } => WireError::Timeout,
                        CallError::NotConnected | CallError::Unknown { .. } => WireError::Closed,
                        _ => WireError::Other("request could not be encoded or decoded".into()),
                    })
            })
            .await
            .map_err(|_| WireError::Timeout)?
        })
    }
    fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
    fn track(&self, session_id: &str, cursor: Cursor) {
        let _ = self.control.send(Control::Track(session_id.into(), cursor));
    }
    fn resync(&self, session_id: &str, cursor: Cursor) {
        let _ = self
            .control
            .send(Control::Resync(session_id.into(), cursor));
    }
    fn take_events(&self) -> Option<mpsc::Receiver<WireEvent>> {
        self.events.lock().ok()?.take()
    }
    fn dropped_items(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

/// Decode a synthetic wire fixture with the generated contract types.
pub fn accept_wire_fixture(kind: &str, value: &Value) -> Result<(), String> {
    use hermes_contract::schemas::{CanonicalSnapshot, RuntimeDescribeResult};
    match kind {
        "CanonicalSnapshot" => serde_json::from_value::<CanonicalSnapshot>(value.clone())
            .map(|_| ())
            .map_err(|e| e.to_string()),
        "RuntimeDescribeResult" => serde_json::from_value::<RuntimeDescribeResult>(value.clone())
            .map(|_| ())
            .map_err(|e| e.to_string()),
        "EventFrame" => EventFrame::from_params(value)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        _ => Err(format!("unknown wire fixture type: {kind}")),
    }
}
