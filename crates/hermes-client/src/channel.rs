//! The connection actor: one task owns the pending calls, the open server requests, and the generation.
//!
//! A frame is stamped with the generation of the connection that read it. A frame from any other
//! generation changes nothing except [`ChannelStats::stale_generation_frames`]. Reconnect is
//! [`Client::replace_connection`] on this same client. [`Client::start`] runs once.
//!
//! Every outbound text frame goes through [`write_frame`]. Every inbound payload goes through
//! [`decode_frame`]. A later recording tap belongs on those two functions and nowhere else.
//!
//! An answer to `sudo` or `secret`, including [`Responder::fail`] and the error sent when a
//! [`Responder`] is dropped, is read by the agent as a skip. [`Responder::ignore`] is the only
//! operation that leaves the request alone. This crate does not send a 4404 decline.
//!
//! The responder's failed write stays answerable ([`RespondError::NotConnected`]). Hermes marks
//! the responder settled before the write (`apps/shared/src/json-rpc-channel.ts` L371), so a failed
//! write is final there.
//!
//! The other departures from that file, also listed on the rpc module: a numeric id with a
//! method is not a pending call (L470); an event is an event whatever its id (L470 before L500);
//! a timeout of zero sends nothing (L298); a reply with neither `result` nor `error` stays
//! pending (L493); an `id: null` error is `Malformed(GatewayError)`; an event whose `params.type`
//! is not a string is `Malformed(UnexpectedRequest)` (L500).

#![warn(missing_docs)]

use crate::rpc::{CallError, FrameClass, RpcError, classify, encode_request};
use futures_util::{SinkExt, StreamExt};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use serde::Serialize;
use serde_json::Value;

use crate::connect::{ConnectionParts, WsSink};
use crate::record::Recorder;
use hermes_contract::events::EventFrame;
use hermes_contract::events::ServerRequest as ContractServerRequest;

/// The number of the connection the client is speaking on. The first is [`Generation`]`(1)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generation(pub u64);

/// [`Client::replace_connection`] was called after [`Client::close`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientClosed;

/// Counters for one client. Gauges describe the present moment. The four totals never reset.
#[derive(Clone, Debug, Default)]
pub struct ChannelStats {
    /// The generation the client is on.
    pub generation: Generation,
    /// Calls waiting for a reply.
    pub pending_calls: usize,
    /// Server requests that are still answerable.
    pub open_server_requests: usize,
    /// Frames of the current generation. Zero when a generation begins.
    pub frames_received: u64,
    /// When the current generation last received a frame. `None` when a generation begins.
    pub last_frame_at: Option<Instant>,
    /// When this generation ended, independent of when its inbound closure is consumed.
    pub closed_at: Option<Instant>,
    /// Frames whose generation was not current. A total for the life of the client.
    pub stale_generation_frames: u64,
    /// Replies that matched no waiting integer id. A total for the life of the client.
    pub unmatched_replies: u64,
    /// Frames reported as malformed. A total for the life of the client.
    pub malformed_frames: u64,
    /// Responders dropped before anyone answered. A total for the life of the client.
    pub dropped_without_decision: u64,
    /// Inbound items queued and not yet taken. Zero once the stream is dropped.
    pub inbound_backlog: usize,
}

/// Why the connection that owned a generation ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseReason {
    /// The peer sent a close frame. Code 1011 is [`Overloaded`](Self::Overloaded), not this variant.
    RemoteClosed {
        /// The close code, when the frame carried one.
        code: Option<u16>,
    },
    /// Close code 1011. The peer could not drain, or its fan-out overflowed.
    Overloaded,
    /// The socket ended without a close frame, or the read failed.
    Error(std::io::ErrorKind),
    /// [`Client::replace_connection`] retired this generation.
    Replaced,
    /// [`Client::close`] ended the client.
    ClientClosed,
}

/// The last inbound item of one generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClosedConnection {
    /// The generation that ended.
    pub generation: Generation,
    /// Why it ended.
    pub reason: CloseReason,
}

/// One event, with the generation that carried it and the raw `params` object.
///
/// `Debug` and `Display` show the generation, the event type, `session_id`, and `seq`.
/// They do not show the payload or `raw`.
#[derive(Clone)]
pub struct InboundEvent {
    /// The connection that carried the event.
    pub generation: Generation,
    /// The decoded event, including an unknown type.
    pub frame: EventFrame,
    /// The whole `params` object.
    pub raw: Value,
}

impl std::fmt::Debug for InboundEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InboundEvent")
            .field("generation", &self.generation)
            .field("type", &self.frame.type_name())
            .field("session_id", &self.frame.session_id)
            .field("seq", &self.frame.seq)
            .finish()
    }
}

impl std::fmt::Display for InboundEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, formatter)
    }
}

/// Why a frame was not a reply, an event, or a server request the application can answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MalformedKind {
    /// Not UTF-8, or not JSON.
    NotJson,
    /// JSON that is not an object.
    NotObject,
    /// A method that is not a server request and not an event with a string type.
    UnexpectedRequest,
    /// An error frame with a null or missing id.
    GatewayError,
    /// Nothing in the classification table matched.
    Unrecognized,
    /// An event or a known server request whose payload did not decode.
    BadPayload,
    /// A server-request method this pin does not know. Reported once per method per connection.
    UnknownServerRequest,
}

/// A frame the application can see, without the frame text.
///
/// `label` is at most 64 characters of an event type or unreadable server-request kind.
/// `Debug` and `Display` show the generation,
/// the kind, the length, and that label.
#[derive(Clone)]
pub struct MalformedFrame {
    /// The connection that carried the frame.
    pub generation: Generation,
    /// Which rule rejected the frame.
    pub kind: MalformedKind,
    /// Byte length of the payload.
    pub length: usize,
    /// An event type or unreadable server-request kind. Never frame text.
    pub label: Option<String>,
}

impl std::fmt::Debug for MalformedFrame {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MalformedFrame")
            .field("generation", &self.generation)
            .field("kind", &self.kind)
            .field("length", &self.length)
            .field("label", &self.label)
            .finish()
    }
}

impl std::fmt::Display for MalformedFrame {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, formatter)
    }
}

/// Something the gateway sent that is not a reply to a call.
///
/// Not `Clone`: a [`ServerRequest`] owns a [`Responder`], and a responder is not cloned.
// Preserve the agreed public variants; the generated EventFrame determines their size.
#[allow(clippy::large_enum_variant)]
pub enum Inbound {
    /// An event notification.
    Event(InboundEvent),
    /// A question the gateway is waiting on.
    ServerRequest(ServerRequest),
    /// A frame that was counted and not otherwise delivered.
    Malformed(MalformedFrame),
    /// The last item of one generation.
    Closed(ClosedConnection),
}

/// A server request, plus the handle that answers it.
///
/// `Debug` and `Display` show the generation, the id, the kind, `session_id`, and the length of
/// the params. They do not show the params. The public `request` field is the contract type, whose
/// own `Debug` may show the command: log the kind, not `request`.
pub struct ServerRequest {
    /// The connection that carried the request.
    pub generation: Generation,
    /// The `srq-` id.
    pub id: String,
    /// `params.session_id` when it was a string.
    pub session_id: Option<String>,
    /// The decoded request. Its derived `Debug` may include the command.
    pub request: ContractServerRequest,
    /// Answers this request. The first successful answer wins.
    pub responder: Responder,
}

impl std::fmt::Debug for ServerRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = serde_json::to_value(&self.request).unwrap_or(Value::Null);
        let (kind, length) = value
            .as_object()
            .and_then(|o| o.iter().next())
            .map(|(kind, params)| (kind.as_str(), params.to_string().len()))
            .unwrap_or(("unknown", 0));
        formatter
            .debug_struct("ServerRequest")
            .field("generation", &self.generation)
            .field("id", &self.id)
            .field("kind", &kind)
            .field("session_id", &self.session_id)
            .field("params_length", &length)
            .finish()
    }
}

impl std::fmt::Display for ServerRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, formatter)
    }
}

/// The handle for one server request. It is not cloned.
///
/// `respond` and `fail` borrow it, so a failed write leaves the request answerable.
/// `ignore` consumes it and writes nothing. Dropping it unanswered writes
/// `-32603` `request dropped by client` when a connection is current, and writes nothing
/// when none is. Either drop counts as [`ChannelStats::dropped_without_decision`].
///
/// `Debug` and `Display` show the id and the state. They do not show an answer.
pub struct Responder {
    state: Arc<AtomicU8>,
    id: String,
    sender: Option<mpsc::WeakUnboundedSender<Command>>,
    swallowed: Option<Arc<AtomicU64>>,
    ignored: bool,
}

impl Responder {
    /// Write a `result` for this request and wait until the write finishes. There is no timer.
    pub async fn respond<T: Serialize>(&self, result: &T) -> Result<(), RespondError> {
        let result = serde_json::to_value(result).map_err(|_| RespondError::Encode)?;
        self.send_answer(Answer::Result(result)).await
    }

    /// Write an `error` for this request and wait until the write finishes. There is no timer.
    ///
    /// For `sudo` and `secret`, the agent reads any error as a skip.
    pub async fn fail(&self, code: i64, message: &str) -> Result<(), RespondError> {
        self.send_answer(Answer::Error {
            code,
            message: message.to_owned(),
        })
        .await
    }

    /// Leave the request for another viewer. Nothing is written.
    pub fn ignore(mut self) {
        self.ignored = true;
        if let Some(sender) = self
            .sender
            .as_ref()
            .and_then(mpsc::WeakUnboundedSender::upgrade)
        {
            let _ = sender.send(Command::Ignore(self.id.clone()));
        }
        self.state
            .compare_exchange(OPEN, ANSWERED, Ordering::SeqCst, Ordering::SeqCst)
            .ok();
    }

    /// A responder for replay. `respond` and `fail` succeed, write nothing, and add one to `swallowed`.
    #[doc(hidden)]
    pub fn detached(swallowed: Arc<AtomicU64>) -> Self {
        Self {
            state: Arc::new(AtomicU8::new(OPEN)),
            id: String::new(),
            sender: None,
            swallowed: Some(swallowed),
            ignored: false,
        }
    }
}

impl std::fmt::Debug for Responder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Responder")
            .field("id", &self.id)
            .field("state", &self.state.load(Ordering::SeqCst))
            .finish()
    }
}

impl std::fmt::Display for Responder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, formatter)
    }
}

/// Why an answer was not written.
///
/// `Debug` and `Display` name the variant. They do not show the answer.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RespondError {
    /// No connection is current, or the write failed. The request stays answerable.
    NotConnected,
    /// This request already has a successful answer.
    AlreadyAnswered,
    /// `request.cancel` or [`Client::close`] ended the request. Nothing is written.
    Cancelled,
    /// The result value could not be encoded. The request stays answerable.
    Encode,
}

impl std::fmt::Debug for RespondError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NotConnected => "NotConnected",
            Self::AlreadyAnswered => "AlreadyAnswered",
            Self::Cancelled => "Cancelled",
            Self::Encode => "Encode",
        })
    }
}

impl std::fmt::Display for RespondError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, formatter)
    }
}

/// What [`decode_frame`] made of one payload.
///
/// Not `Clone`: an inbound server request owns a [`Responder`].
// The recording seam returns the same public Inbound value without changing its shape.
#[allow(clippy::large_enum_variant)]
pub enum Decoded {
    /// An event, a server request, or a malformed frame.
    Inbound(Inbound),
    /// A reply. The actor matches it. It is not delivered on the inbound stream.
    Reply(ReplyFrame),
}

/// A reply, split into its id and its body. `Ok` is `result`. `Err` is `error`.
///
/// An object that has both is `Err`.
#[derive(Clone)]
pub struct ReplyFrame {
    /// The `id` value, unchanged.
    pub id: Value,
    /// `Ok(result)` or `Err(error)`.
    pub body: Result<Value, Value>,
}

impl std::fmt::Debug for ReplyFrame {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReplyFrame")
            .field("id", &self.id)
            .field("is_error", &self.body.is_err())
            .finish()
    }
}

/// The typed channel. Cheap to clone. One actor serves every clone.
#[derive(Clone)]
pub struct Client {
    handle: Arc<ClientHandle>,
}

/// The inbound queue. [`InboundStream::next`] is `None` only after the final
/// [`CloseReason::ClientClosed`], or when every [`Client`] clone is dropped.
pub struct InboundStream {
    receiver: mpsc::UnboundedReceiver<Inbound>,
    sender: mpsc::WeakUnboundedSender<Command>,
    ended: bool,
}

impl Client {
    /// Install a parameter injector for outbound calls.
    pub fn set_param_injector(&self, _injector: Option<crate::remote::ParamInjector>) {
        todo!("add remote gateway implementation")
    }
    /// Start the actor on `parts`. The generation is 1. The id counter starts at `parts.next_request_id`.
    pub fn start(parts: ConnectionParts) -> (Self, InboundStream) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let (inbound, inbound_rx) = mpsc::unbounded_channel();
        let generation = Arc::new(AtomicU64::new(1));
        let handle = Arc::new(ClientHandle {
            sender,
            generation: generation.clone(),
            token: AtomicU64::new(1),
        });
        tokio::spawn(Actor::run(
            parts,
            receiver,
            inbound,
            generation,
            handle.sender.downgrade(),
        ));
        let stream = InboundStream {
            receiver: inbound_rx,
            sender: handle.sender.downgrade(),
            ended: false,
        };
        (Self { handle }, stream)
    }

    /// Swap the socket. Pending calls of the old generation become [`crate::CallError::Unknown`].
    ///
    /// Order: bump the generation, fail those calls, deliver [`CloseReason::Replaced`] if the old
    /// generation had not already closed, stop the old tasks, start the new ones, then deliver
    /// `parts.buffered` before any frame read from the new socket.
    pub async fn replace_connection(
        &self,
        parts: ConnectionParts,
    ) -> Result<Generation, ClientClosed> {
        let (sender, receiver) = oneshot::channel();
        self.handle
            .sender
            .send(Command::Replace(Box::new(parts), sender))
            .map_err(|_| ClientClosed)?;
        receiver.await.unwrap_or(Err(ClientClosed))
    }

    /// End the client. Pending calls become unknown. Open responders become [`RespondError::Cancelled`].
    ///
    /// One [`CloseReason::ClientClosed`] is delivered, and it is the last item.
    pub async fn close(&self) {
        let (sender, receiver) = oneshot::channel();
        if self
            .handle
            .sender
            .send(Command::Close(Some(sender)))
            .is_ok()
        {
            let _ = receiver.await;
        }
    }

    /// The generation the client is on.
    pub fn generation(&self) -> Generation {
        Generation(self.handle.generation.load(Ordering::SeqCst))
    }

    /// A snapshot. A `Forget` from a dropped caller is applied before this returns.
    pub async fn stats(&self) -> ChannelStats {
        let (sender, receiver) = oneshot::channel();
        if self.handle.sender.send(Command::Stats(sender)).is_err() {
            return ChannelStats::default();
        }
        receiver.await.unwrap_or_default()
    }
}

impl InboundStream {
    /// The next inbound item.
    pub async fn next(&mut self) -> Option<Inbound> {
        if self.ended {
            return None;
        }
        let item = self.receiver.recv().await?;
        if matches!(
            item,
            Inbound::Closed(ClosedConnection {
                reason: CloseReason::ClientClosed,
                ..
            })
        ) {
            self.ended = true;
        }
        if let Some(sender) = self.sender.upgrade() {
            let _ = sender.send(Command::Taken);
        }
        Some(item)
    }
}

/// Write one text frame. This is the only function that sends text on the socket.
#[doc(hidden)]
pub async fn write_frame(
    sink: &mut WsSink,
    generation: Generation,
    text: &str,
) -> Result<(), std::io::Error> {
    let _ = generation;
    sink.send(Message::Text(text.to_owned().into()))
        .await
        .map_err(|failure| match failure {
            tokio_tungstenite::tungstenite::Error::Io(error) => std::io::Error::from(error.kind()),
            _ => std::io::Error::from(std::io::ErrorKind::BrokenPipe),
        })
}

// Keep the stamped envelope and original payload even when a stream event's
// typed payload cannot represent it. Other malformed frames remain errors.
pub(crate) fn decode_event_params(
    params: &Value,
) -> Result<EventFrame, hermes_contract::events::DecodeError> {
    match EventFrame::from_params(params) {
        Ok(frame) => Ok(frame),
        Err(error) => {
            let kind = params.get("type").and_then(Value::as_str);
            if !matches!(kind, Some("message.delta" | "tool.start" | "tool.complete"))
                || params.get("session_id").and_then(Value::as_str).is_none()
                || params.get("admission_id").and_then(Value::as_str).is_none()
                || params.get("seq").and_then(Value::as_u64).is_none()
                || params.get("replay_epoch").and_then(Value::as_str).is_none()
            {
                return Err(error);
            }
            let mut envelope = params.clone();
            if let Some(object) = envelope.as_object_mut() {
                object.insert("type".into(), Value::String("raw.stream.event".into()));
            }
            let mut frame = EventFrame::from_params(&envelope)?;
            frame.notification = hermes_contract::events::Notification::Unknown(params.clone());
            Ok(frame)
        }
    }
}

/// Classify and decode one payload. This is the only function that interprets inbound text.
///
/// `responders` is called once for a server request the application may answer, with the request id.
#[doc(hidden)]
pub fn decode_frame(
    generation: Generation,
    raw: &[u8],
    responders: &mut dyn FnMut(&str) -> Responder,
) -> Decoded {
    let class = classify(raw);
    let value: Value = serde_json::from_slice(raw).unwrap_or(Value::Null);
    let malformed = |kind| {
        Decoded::Inbound(Inbound::Malformed(MalformedFrame {
            generation,
            kind,
            length: raw.len(),
            label: value
                .pointer("/params/type")
                .or_else(|| (class == FrameClass::ServerRequest).then(|| &value["method"]))
                .and_then(Value::as_str)
                .map(|label| label.chars().take(64).collect()),
        }))
    };
    match class {
        FrameClass::Malformed(kind) => malformed(kind),
        FrameClass::Reply => Decoded::Reply(ReplyFrame {
            id: value["id"].clone(),
            body: if let Some(error) = value.get("error") {
                Err(error.clone())
            } else {
                Ok(value["result"].clone())
            },
        }),
        FrameClass::Event => match decode_event_params(&value["params"]) {
            Ok(frame) => Decoded::Inbound(Inbound::Event(InboundEvent {
                generation,
                frame,
                raw: value["params"].clone(),
            })),
            Err(_) => malformed(MalformedKind::BadPayload),
        },
        FrameClass::ServerRequest => {
            match hermes_contract::events::ServerRequestFrame::from_request(&value) {
                Ok(frame) => {
                    // Classification already requires a string identifier.
                    let id = value["id"].as_str().unwrap_or_default();
                    Decoded::Inbound(Inbound::ServerRequest(ServerRequest {
                        generation,
                        id: id.to_owned(),
                        session_id: value
                            .pointer("/params/session_id")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        request: frame.request,
                        responder: responders(id),
                    }))
                }
                Err(_) => malformed(MalformedKind::BadPayload),
            }
        }
    }
}

/// What the actor should do after one state change. No sockets and no clocks.
///
/// Not `Clone`: a [`Effect::Deliver`] owns an [`Inbound`], and a responder is not cloned.
// Keep the socket-free effects directly matchable, as at the public decoding seam.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Effect {
    Deliver(Inbound),
    Write {
        generation: Generation,
        text: String,
    },
    Complete {
        id: u64,
        outcome: Result<Value, crate::CallError>,
    },
}

/// A server-request answer, before it is encoded.
#[derive(Clone)]
pub(crate) enum Answer {
    Result(Value),
    Error { code: i64, message: String },
}

/// The actor's data, with no socket, task, or clock.
pub(crate) struct ChannelState {
    stats: ChannelStats,
    next_id: u64,
    connected: bool,
    closed: bool,
    generation_closed: bool,
    calls: HashMap<u64, String>,
    requests: HashMap<String, Arc<AtomicU8>>,
    unknown_methods: HashSet<String>,
}

impl ChannelState {
    pub(crate) fn new(next_request_id: u64) -> Self {
        Self {
            stats: ChannelStats::default(),
            next_id: next_request_id,
            connected: false,
            closed: false,
            generation_closed: true,
            calls: HashMap::new(),
            requests: HashMap::new(),
            unknown_methods: HashSet::new(),
        }
    }

    /// Enter `generation`. Fail the previous generation's calls and emit [`CloseReason::Replaced`]
    /// when that generation had not already closed. `frames_received` and `last_frame_at` restart.
    pub(crate) fn begin_generation(&mut self, generation: Generation) -> Vec<Effect> {
        let effects = self.on_closed(self.stats.generation, CloseReason::Replaced);
        self.stats.generation = generation;
        self.stats.frames_received = 0;
        self.stats.last_frame_at = None;
        self.stats.closed_at = None;
        self.connected = true;
        self.generation_closed = false;
        self.unknown_methods.clear();
        effects
    }

    /// Take the next id and remember `method` on the current generation.
    pub(crate) fn register_call(&mut self, method: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.calls.insert(id, method.to_owned());
        id
    }

    /// Drop a pending call. A later reply is unmatched.
    pub(crate) fn forget_call(&mut self, id: u64) {
        self.calls.remove(&id);
    }

    /// Apply one frame that claims `generation`. `now` is the caller's clock, not read here.
    pub(crate) fn on_frame(
        &mut self,
        generation: Generation,
        raw: &[u8],
        now: Instant,
    ) -> Vec<Effect> {
        if generation != self.stats.generation {
            self.stats.stale_generation_frames += 1;
            return Vec::new();
        }
        if self.generation_closed {
            return Vec::new();
        }
        self.stats.frames_received += 1;
        self.stats.last_frame_at = Some(now);
        let decoded = decode_frame(generation, raw, &mut |id| {
            let mut responder = Responder::detached(Arc::new(AtomicU64::new(0)));
            responder.id = id.to_owned();
            responder
        });
        match decoded {
            Decoded::Reply(reply) => {
                if let Some(id) = reply.id.as_u64().filter(|id| self.calls.contains_key(id)) {
                    self.calls.remove(&id);
                    vec![Effect::Complete {
                        id,
                        outcome: reply
                            .body
                            .map_err(|error| CallError::Rpc(RpcError::from_value(&error))),
                    }]
                } else {
                    self.stats.unmatched_replies += 1;
                    Vec::new()
                }
            }
            Decoded::Inbound(Inbound::ServerRequest(request)) => {
                if matches!(request.request, ContractServerRequest::Unknown(_)) {
                    let value: Value = serde_json::from_slice(raw).unwrap_or(Value::Null);
                    let method = value["method"].as_str().unwrap_or_default();
                    let mut effects = vec![Effect::Write {
                        generation,
                        text: answer_text(
                            &request.id,
                            Answer::Error {
                                code: -32601,
                                message: format!("no handler for server request: {method}"),
                            },
                        ),
                    }];
                    self.stats.malformed_frames += 1;
                    if self.unknown_methods.insert(method.to_owned()) {
                        effects.push(Effect::Deliver(Inbound::Malformed(MalformedFrame {
                            generation,
                            kind: MalformedKind::UnknownServerRequest,
                            length: raw.len(),
                            label: None,
                        })));
                    }
                    effects
                } else {
                    self.requests
                        .insert(request.id.clone(), request.responder.state.clone());
                    vec![Effect::Deliver(Inbound::ServerRequest(request))]
                }
            }
            Decoded::Inbound(Inbound::Event(event)) => {
                if event.frame.type_name() == Some("request.cancel")
                    && let Some(id) = event.raw.pointer("/payload/id").and_then(Value::as_str)
                {
                    self.cancel(id);
                }
                vec![Effect::Deliver(Inbound::Event(event))]
            }
            Decoded::Inbound(Inbound::Malformed(frame)) => {
                self.stats.malformed_frames += 1;
                let mut effects = Vec::new();
                if classify(raw) == FrameClass::ServerRequest {
                    let value: Value = serde_json::from_slice(raw).unwrap_or(Value::Null);
                    let id = value["id"].as_str().unwrap_or_default();
                    let method = value["method"].as_str().unwrap_or_default();
                    effects.push(Effect::Write {
                        generation,
                        text: answer_text(
                            id,
                            Answer::Error {
                                code: -32603,
                                message: format!(
                                    "server request params could not be read: {method}"
                                ),
                            },
                        ),
                    });
                }
                effects.push(Effect::Deliver(Inbound::Malformed(frame)));
                effects
            }
            Decoded::Inbound(Inbound::Closed(_)) => Vec::new(),
        }
    }

    /// End `generation` for `reason`, when it is the current one and it has not already closed.
    pub(crate) fn on_closed(&mut self, generation: Generation, reason: CloseReason) -> Vec<Effect> {
        if generation != self.stats.generation {
            self.stats.stale_generation_frames += 1;
            return Vec::new();
        }
        if self.generation_closed {
            return Vec::new();
        }
        self.connected = false;
        self.generation_closed = true;
        self.stats.closed_at = Some(Instant::now());
        let mut effects: Vec<_> = self
            .calls
            .drain()
            .map(|(id, method)| Effect::Complete {
                id,
                outcome: Err(CallError::Unknown { method }),
            })
            .collect();
        effects.push(Effect::Deliver(Inbound::Closed(ClosedConnection {
            generation,
            reason,
        })));
        effects
    }

    /// The first successful answer wins. A second call is [`RespondError::AlreadyAnswered`].
    pub(crate) fn answer(
        &mut self,
        request_id: &str,
        answer: Answer,
    ) -> Result<Effect, RespondError> {
        let state = self
            .requests
            .get(request_id)
            .ok_or(RespondError::Cancelled)?;
        match state.load(Ordering::SeqCst) {
            ANSWERED => return Err(RespondError::AlreadyAnswered),
            CANCELLED | ANSWERED_CANCELLED => return Err(RespondError::Cancelled),
            _ => {}
        }
        if self.closed {
            return Err(RespondError::Cancelled);
        }
        if !self.connected {
            return Err(RespondError::NotConnected);
        }
        state.store(ANSWERED, Ordering::SeqCst);
        Ok(Effect::Write {
            generation: self.stats.generation,
            text: answer_text(request_id, answer),
        })
    }

    /// `ignore` writes nothing and closes the request.
    pub(crate) fn ignore(&mut self, request_id: &str) {
        if let Some(state) = self.requests.remove(request_id) {
            state.store(ANSWERED, Ordering::SeqCst);
        }
    }

    /// The responder was dropped. Counts [`ChannelStats::dropped_without_decision`].
    ///
    /// With a current connection this emits the `-32603` answer. With none, it writes nothing.
    pub(crate) fn drop_undecided(&mut self, request_id: &str) -> Vec<Effect> {
        let Some(state) = self.requests.remove(request_id) else {
            return Vec::new();
        };
        if state.load(Ordering::SeqCst) != OPEN {
            return Vec::new();
        }
        state.store(ANSWERED, Ordering::SeqCst);
        self.stats.dropped_without_decision += 1;
        if self.connected {
            vec![Effect::Write {
                generation: self.stats.generation,
                text: answer_text(
                    request_id,
                    Answer::Error {
                        code: -32603,
                        message: "request dropped by client".to_owned(),
                    },
                ),
            }]
        } else {
            Vec::new()
        }
    }

    /// `request.cancel`, or the same transition without a frame.
    pub(crate) fn cancel(&mut self, request_id: &str) {
        if let Some(state) = self.requests.get(request_id) {
            cancel_state(state);
        }
    }

    /// [`Client::close`]: pending calls unknown, open responders cancelled, one client-closed item.
    pub(crate) fn close_client(&mut self) -> Vec<Effect> {
        if self.closed {
            return Vec::new();
        }
        self.closed = true;
        for state in self.requests.values() {
            cancel_state(state);
        }
        let mut effects = self.on_closed(self.stats.generation, CloseReason::ClientClosed);
        if effects.is_empty() {
            effects.push(Effect::Deliver(Inbound::Closed(ClosedConnection {
                generation: self.stats.generation,
                reason: CloseReason::ClientClosed,
            })));
        }
        effects
    }

    pub(crate) fn stats(&self, inbound_backlog: usize) -> ChannelStats {
        let mut stats = self.stats.clone();
        stats.pending_calls = self.calls.len();
        stats.open_server_requests = self
            .requests
            .values()
            .filter(|state| matches!(state.load(Ordering::SeqCst), OPEN | WRITING))
            .count();
        stats.inbound_backlog = inbound_backlog;
        stats
    }
}

const OPEN: u8 = 0;
const ANSWERED: u8 = 1;
const CANCELLED: u8 = 2;
const WRITING: u8 = 3;
const ANSWERED_CANCELLED: u8 = 4;

fn cancel_state(state: &AtomicU8) {
    let _ = state.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
        Some(if matches!(value, ANSWERED | ANSWERED_CANCELLED) {
            ANSWERED_CANCELLED
        } else {
            CANCELLED
        })
    });
}

fn mark_written(state: &AtomicU8) {
    // Cancellation may race a write already in progress. Preserve both facts.
    let _ = state.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
        Some(if matches!(value, CANCELLED | ANSWERED_CANCELLED) {
            ANSWERED_CANCELLED
        } else {
            ANSWERED
        })
    });
}

fn answer_text(id: &str, answer: Answer) -> String {
    match answer {
        Answer::Result(value) => {
            serde_json::json!({"jsonrpc":"2.0", "id":id, "result":value}).to_string()
        }
        Answer::Error { code, message } => {
            serde_json::json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})
                .to_string()
        }
    }
}

impl Responder {
    async fn send_answer(&self, answer: Answer) -> Result<(), RespondError> {
        match self.state.load(Ordering::SeqCst) {
            CANCELLED | ANSWERED_CANCELLED => return Err(RespondError::Cancelled),
            ANSWERED => return Err(RespondError::AlreadyAnswered),
            _ => {}
        }
        if let Some(counter) = &self.swallowed {
            self.state
                .compare_exchange(OPEN, ANSWERED, Ordering::SeqCst, Ordering::SeqCst)
                .map_err(|_| RespondError::AlreadyAnswered)?;
            counter.fetch_add(1, Ordering::SeqCst);
            return Ok(());
        }
        let sender = self
            .sender
            .as_ref()
            .and_then(mpsc::WeakUnboundedSender::upgrade)
            .ok_or(RespondError::Cancelled)?;
        let (tx, rx) = oneshot::channel();
        sender
            .send(Command::Answer(self.id.clone(), answer, tx))
            .map_err(|_| RespondError::Cancelled)?;
        rx.await.unwrap_or(Err(RespondError::Cancelled))
    }
}

impl Drop for Responder {
    fn drop(&mut self) {
        if !self.ignored
            && let Some(sender) = self
                .sender
                .as_ref()
                .and_then(mpsc::WeakUnboundedSender::upgrade)
        {
            let _ = sender.send(Command::DropResponder(self.id.clone()));
        }
    }
}

impl std::error::Error for RespondError {}
impl std::fmt::Display for ClientClosed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("client closed")
    }
}
impl std::error::Error for ClientClosed {}

struct ClientHandle {
    sender: mpsc::UnboundedSender<Command>,
    generation: Arc<AtomicU64>,
    token: AtomicU64,
}

impl Drop for ClientHandle {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Gone);
    }
}

impl Drop for InboundStream {
    fn drop(&mut self) {
        self.receiver.close();
        if let Some(sender) = self.sender.upgrade() {
            let _ = sender.send(Command::StreamDropped);
        }
    }
}

enum Command {
    Call {
        token: u64,
        method: String,
        params: Value,
        result: oneshot::Sender<Result<Value, CallError>>,
    },
    Forget(u64),
    Replace(
        Box<ConnectionParts>,
        oneshot::Sender<Result<Generation, ClientClosed>>,
    ),
    Close(Option<oneshot::Sender<()>>),
    Gone,
    Stats(oneshot::Sender<ChannelStats>),
    Answer(String, Answer, oneshot::Sender<Result<(), RespondError>>),
    Ignore(String),
    DropResponder(String),
    Taken,
    StreamDropped,
}

struct ForgetOnDrop {
    sender: mpsc::UnboundedSender<Command>,
    token: u64,
}
impl Drop for ForgetOnDrop {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Forget(self.token));
    }
}

impl Client {
    pub(crate) async fn execute_call(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        started: tokio::time::Instant,
    ) -> Result<Value, CallError> {
        let deadline = started.checked_add(timeout);
        let token = self.handle.token.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.handle
            .sender
            .send(Command::Call {
                token,
                method: method.to_owned(),
                params,
                result: tx,
            })
            .map_err(|_| CallError::NotConnected)?;
        let _guard = ForgetOnDrop {
            sender: self.handle.sender.clone(),
            token,
        };
        tokio::select! {
            biased;
            _ = async { match deadline { Some(deadline) => tokio::time::sleep_until(deadline).await, None => std::future::pending::<()>().await } } => Err(CallError::TimedOut { method: method.to_owned(), after: timeout }),
            result = rx => result.unwrap_or_else(|_| Err(CallError::Unknown { method: method.to_owned() })),
        }
    }
}

enum SocketEvent {
    Frame(Generation, Vec<u8>),
    Control(Generation),
    CloseFrame(Generation, CloseReason),
    Closed(Generation, CloseReason),
    Written(Generation, String, Result<(), RespondError>),
}

struct WriteJob {
    text: String,
    request: Option<String>,
    state: Option<Arc<AtomicU8>>,
}
struct Transport {
    writer: mpsc::UnboundedSender<WriteJob>,
    reader_task: JoinHandle<()>,
    writer_task: JoinHandle<()>,
}
impl Drop for Transport {
    fn drop(&mut self) {
        self.reader_task.abort();
        self.writer_task.abort();
    }
}

impl Transport {
    async fn stop(mut self) {
        self.reader_task.abort();
        self.writer_task.abort();
        let _ = (&mut self.reader_task).await;
        let _ = (&mut self.writer_task).await;
    }

    fn start(
        parts: ConnectionParts,
        generation: Generation,
        events: mpsc::UnboundedSender<SocketEvent>,
        recorder: Option<Recorder>,
    ) -> (Self, Vec<String>) {
        // Frames in `buffered` were recorded by the connection before hand-over; only frames
        // that cross the socket from here on are recorded below.
        let read_recorder = recorder.clone();
        let ConnectionParts {
            mut sink,
            mut stream,
            buffered,
            ..
        } = parts;
        let (writer, mut jobs) = mpsc::unbounded_channel::<WriteJob>();
        let read_events = events.clone();
        let reader_task = tokio::spawn(async move {
            let reason = loop {
                match stream.next().await {
                    Some(Ok(Message::Text(text))) => { if let Some(recorder) = &read_recorder { recorder.record_inbound(generation.0, &text); }
                        if read_events.send(SocketEvent::Frame(generation, text.as_bytes().to_vec())).is_err() { return; } },
                    Some(Ok(Message::Binary(bytes))) => { if read_events.send(SocketEvent::Frame(generation, bytes.to_vec())).is_err() { return; } },
                    Some(Ok(Message::Close(frame))) => { let reason = match frame {
                        Some(frame) if u16::from(frame.code) == 1011 => CloseReason::Overloaded,
                        frame => CloseReason::RemoteClosed { code: frame.map(|frame| u16::from(frame.code)) },
                    }; let _ = read_events.send(SocketEvent::CloseFrame(generation, reason)); return; },
                    Some(Ok(_)) => { let _ = read_events.send(SocketEvent::Control(generation)); },
                    Some(Err(tokio_tungstenite::tungstenite::Error::Io(error))) => break CloseReason::Error(error.kind()),
                    Some(Err(tokio_tungstenite::tungstenite::Error::Protocol(tokio_tungstenite::tungstenite::error::ProtocolError::ResetWithoutClosingHandshake))) | None => break CloseReason::Error(std::io::ErrorKind::UnexpectedEof),
                    Some(Err(_)) => break CloseReason::Error(std::io::ErrorKind::InvalidData),
                }
            };
            let _ = read_events.send(SocketEvent::Closed(generation, reason));
        });
        let writer_task = tokio::spawn(async move {
            while let Some(WriteJob {
                text,
                request,
                state,
            }) = jobs.recv().await
            {
                if state.as_ref().is_some_and(|state| {
                    matches!(state.load(Ordering::SeqCst), CANCELLED | ANSWERED_CANCELLED)
                }) {
                    if let Some(request) = request {
                        let _ = events.send(SocketEvent::Written(
                            generation,
                            request,
                            Err(RespondError::Cancelled),
                        ));
                    }
                    continue;
                }
                if let Some(recorder) = &recorder {
                    recorder.record_outbound(generation.0, &text);
                }
                let result = write_frame(&mut sink, generation, &text).await;
                drop(text);
                if result.is_ok()
                    && let Some(state) = &state
                {
                    mark_written(state);
                }
                if let Some(request) = request {
                    let _ = events.send(SocketEvent::Written(
                        generation,
                        request,
                        if result.is_ok() {
                            Ok(())
                        } else {
                            Err(RespondError::NotConnected)
                        },
                    ));
                }
                if let Err(error) = result {
                    let _ = events.send(SocketEvent::Closed(
                        generation,
                        CloseReason::Error(error.kind()),
                    ));
                    return;
                }
            }
        });
        (
            Self {
                writer,
                reader_task,
                writer_task,
            },
            buffered,
        )
    }
}

struct PendingCall {
    token: u64,
    result: oneshot::Sender<Result<Value, CallError>>,
}
type AnswerResult = oneshot::Sender<Result<(), RespondError>>;
type QueuedAnswers = std::collections::VecDeque<(Answer, AnswerResult)>;
struct Actor {
    state: ChannelState,
    pending: HashMap<u64, PendingCall>,
    tokens: HashMap<u64, u64>,
    answers: HashMap<String, oneshot::Sender<Result<(), RespondError>>>,
    answer_queue: HashMap<String, QueuedAnswers>,
    transport: Option<Transport>,
    inbound: mpsc::UnboundedSender<Inbound>,
    stream_alive: bool,
    backlog: usize,
    generation: Arc<AtomicU64>,
    commands: mpsc::WeakUnboundedSender<Command>,
    recorder: Option<Recorder>,
}

impl Actor {
    async fn run(
        parts: ConnectionParts,
        mut commands: mpsc::UnboundedReceiver<Command>,
        inbound: mpsc::UnboundedSender<Inbound>,
        generation: Arc<AtomicU64>,
        sender: mpsc::WeakUnboundedSender<Command>,
    ) {
        let (events, mut event_rx) = mpsc::unbounded_channel();
        let mut actor = Self {
            state: ChannelState::new(parts.next_request_id),
            pending: HashMap::new(),
            tokens: HashMap::new(),
            answers: HashMap::new(),
            answer_queue: HashMap::new(),
            transport: None,
            inbound,
            stream_alive: true,
            backlog: 0,
            generation,
            commands: sender,
            recorder: None,
        };
        actor.install(parts, &events).await;
        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    Some(Command::Gone) | None => { actor.shutdown().await; break; },
                    Some(command) => actor.command(command, &events).await,
                },
                Some(event) = event_rx.recv() => actor.socket_event(event).await,
            }
        }
    }

    async fn install(
        &mut self,
        mut parts: ConnectionParts,
        events: &mpsc::UnboundedSender<SocketEvent>,
    ) {
        // The client keeps its recorder across a replaced connection.
        if let Some(recorder) = parts.recorder.take() {
            self.recorder = Some(recorder);
        }
        let generation = Generation(self.generation.load(Ordering::SeqCst));
        let effects = self.state.begin_generation(generation);
        self.effects(effects);
        self.disconnect_writer().await;
        let (transport, buffered) =
            Transport::start(parts, generation, events.clone(), self.recorder.clone());
        self.transport = Some(transport);
        for text in buffered {
            self.frame(generation, text.as_bytes());
        }
    }

    async fn disconnect_writer(&mut self) {
        if let Some(transport) = self.transport.take() {
            transport.stop().await;
        }
        self.finish_answers();
    }

    fn finish_answers(&mut self) {
        for (id, result) in self.answers.drain() {
            if let Some(state) = self.state.requests.get(&id) {
                let _ = state.compare_exchange(WRITING, OPEN, Ordering::SeqCst, Ordering::SeqCst);
            }
            let cancelled = self.state.requests.get(&id).is_some_and(|state| {
                matches!(state.load(Ordering::SeqCst), CANCELLED | ANSWERED_CANCELLED)
            });
            let written = self.state.requests.get(&id).is_some_and(|state| {
                matches!(state.load(Ordering::SeqCst), ANSWERED | ANSWERED_CANCELLED)
            });
            let _ = result.send(if written {
                Ok(())
            } else {
                Err(if cancelled {
                    RespondError::Cancelled
                } else {
                    RespondError::NotConnected
                })
            });
        }
        for (id, answers) in std::mem::take(&mut self.answer_queue) {
            let outcome = match self
                .state
                .requests
                .get(&id)
                .map(|state| state.load(Ordering::SeqCst))
            {
                Some(CANCELLED | ANSWERED_CANCELLED) => RespondError::Cancelled,
                Some(ANSWERED) => RespondError::AlreadyAnswered,
                _ if self.state.closed => RespondError::Cancelled,
                _ => RespondError::NotConnected,
            };
            for (_, result) in answers {
                let _ = result.send(Err(outcome));
            }
        }
    }

    async fn shutdown(&mut self) {
        let effects = self.state.close_client();
        self.effects(effects);
        self.disconnect_writer().await;
    }

    async fn command(&mut self, command: Command, events: &mpsc::UnboundedSender<SocketEvent>) {
        match command {
            Command::Call {
                token,
                method,
                params,
                result,
            } => {
                if !self.state.connected || self.state.closed {
                    let _ = result.send(Err(CallError::NotConnected));
                    return;
                }
                let id = self.state.register_call(&method);
                match encode_request(id, &method, &params) {
                    Ok(text) => {
                        self.pending.insert(id, PendingCall { token, result });
                        self.tokens.insert(token, id);
                        self.write(text, None);
                    }
                    Err(error) => {
                        self.state.forget_call(id);
                        let _ = result.send(Err(error));
                    }
                }
            }
            Command::Forget(token) => {
                if let Some(id) = self.tokens.remove(&token) {
                    self.pending.remove(&id);
                    self.state.forget_call(id);
                }
            }
            Command::Replace(parts, result) => {
                if self.state.closed {
                    let _ = result.send(Err(ClientClosed));
                    return;
                }
                self.generation.fetch_add(1, Ordering::SeqCst);
                self.install(*parts, events).await;
                let _ = result.send(Ok(self.state.stats.generation));
            }
            Command::Close(result) => {
                self.shutdown().await;
                if let Some(result) = result {
                    let _ = result.send(());
                }
            }
            Command::Stats(result) => {
                let _ = result.send(self.state.stats(self.backlog));
            }
            Command::Answer(id, answer, result) => self.answer(id, answer, result),
            Command::Ignore(id) => self.state.ignore(&id),
            Command::DropResponder(id) => {
                let effects = self.state.drop_undecided(&id);
                self.effects(effects);
            }
            Command::Taken => self.backlog = self.backlog.saturating_sub(1),
            Command::StreamDropped => {
                self.stream_alive = false;
                self.backlog = 0;
            }
            Command::Gone => {}
        }
    }

    async fn socket_event(&mut self, event: SocketEvent) {
        match event {
            SocketEvent::Frame(generation, raw) => self.frame(generation, &raw),
            SocketEvent::Control(generation) => self.control_frame(generation),
            SocketEvent::CloseFrame(generation, reason) => {
                if generation == self.state.stats.generation {
                    self.control_frame(generation);
                }
                self.end_connection(generation, reason).await;
            }
            SocketEvent::Closed(generation, reason) => {
                self.end_connection(generation, reason).await;
            }
            SocketEvent::Written(generation, id, outcome) => {
                if generation != self.state.stats.generation {
                    return;
                }
                if let Some(result) = self.answers.remove(&id) {
                    let cancelled = self
                        .state
                        .requests
                        .get(&id)
                        .is_some_and(|state| state.load(Ordering::SeqCst) == CANCELLED);
                    if let Some(state) = self.state.requests.get(&id) {
                        let _ = state.compare_exchange(
                            WRITING,
                            if outcome.is_ok() { ANSWERED } else { OPEN },
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        );
                    }
                    let _ = result.send(if cancelled {
                        Err(RespondError::Cancelled)
                    } else {
                        outcome
                    });
                    if let Some(mut queue) = self.answer_queue.remove(&id) {
                        while let Some((answer, result)) = queue.pop_front() {
                            self.answer(id.clone(), answer, result);
                        }
                    }
                }
            }
        }
    }

    async fn end_connection(&mut self, generation: Generation, reason: CloseReason) {
        let effects = self.state.on_closed(generation, reason);
        self.effects(effects);
        if generation == self.state.stats.generation {
            self.disconnect_writer().await;
        }
    }

    fn control_frame(&mut self, generation: Generation) {
        if generation != self.state.stats.generation {
            self.state.stats.stale_generation_frames += 1;
        } else if !self.state.generation_closed {
            self.state.stats.frames_received += 1;
            self.state.stats.last_frame_at = Some(Instant::now());
        }
    }

    fn frame(&mut self, generation: Generation, raw: &[u8]) {
        let effects = self.state.on_frame(generation, raw, Instant::now());
        self.effects(effects);
    }

    fn effects(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Complete { id, outcome } => {
                    if let Some(call) = self.pending.remove(&id) {
                        self.tokens.remove(&call.token);
                        let _ = call.result.send(outcome);
                    }
                }
                Effect::Write { generation, text } => {
                    if generation == self.state.stats.generation {
                        self.write(text, None);
                    }
                }
                Effect::Deliver(mut item) => {
                    if let Inbound::ServerRequest(request) = &mut item {
                        request.responder.swallowed = None;
                        request.responder.sender = Some(self.commands.clone());
                        if !self.stream_alive || self.inbound.is_closed() {
                            let value =
                                serde_json::to_value(&request.request).unwrap_or(Value::Null);
                            let kind = request_kind(&value);
                            self.state.ignore(&request.id);
                            request.responder.ignored = true;
                            self.write(
                                answer_text(
                                    &request.id,
                                    Answer::Error {
                                        code: -32601,
                                        message: format!("no handler for server request: {kind}"),
                                    },
                                ),
                                None,
                            );
                            continue;
                        }
                    }
                    if self.stream_alive && self.inbound.send(item).is_ok() {
                        self.backlog += 1;
                    } else {
                        self.stream_alive = false;
                        self.backlog = 0;
                    }
                }
            }
        }
    }

    fn write(&mut self, text: String, request: Option<String>) {
        let state = request
            .as_ref()
            .and_then(|id| self.state.requests.get(id))
            .cloned();
        let sent = self.transport.as_ref().is_some_and(|transport| {
            transport
                .writer
                .send(WriteJob {
                    text,
                    request,
                    state,
                })
                .is_ok()
        });
        if !sent && self.state.connected {
            let effects = self.state.on_closed(
                self.state.stats.generation,
                CloseReason::Error(std::io::ErrorKind::BrokenPipe),
            );
            self.effects(effects);
            self.transport.take();
            self.finish_answers();
        }
    }
    fn answer(
        &mut self,
        id: String,
        answer: Answer,
        result: oneshot::Sender<Result<(), RespondError>>,
    ) {
        if self.answers.contains_key(&id) {
            self.answer_queue
                .entry(id)
                .or_default()
                .push_back((answer, result));
            return;
        }
        match self.state.answer(&id, answer) {
            Ok(Effect::Write { text, .. }) => {
                if let Some(state) = self.state.requests.get(&id) {
                    state.store(WRITING, Ordering::SeqCst);
                }
                self.answers.insert(id.clone(), result);
                self.write(text, Some(id));
            }
            Ok(_) => unreachable!("answer produces a write"),
            Err(error) => {
                let _ = result.send(Err(error));
            }
        }
    }
}

fn request_kind(value: &Value) -> &str {
    match value
        .as_object()
        .and_then(|object| object.keys().next())
        .map(String::as_str)
    {
        Some("Sudo") => "sudo",
        Some("Secret") => "secret",
        Some("Clarify") => "clarify",
        Some("Approval") => "approval",
        Some("DisplayInstallSudo") => "display.install.sudo",
        Some("PreviewAct") => "preview.act",
        Some("PreviewRead") => "preview.read",
        Some("SetupBeat") => "setup_beat",
        Some("TerminalRead") => "terminal.read",
        Some("ThemeApply") => "theme.apply",
        Some("ThemePick") => "theme.pick",
        Some("Tour") => "tour",
        Some("VaultCode") => "vault.code",
        Some("VaultSaveLogin") => "vault.save_login",
        Some("VaultUnlockPrompt") => "vault.unlock_prompt",
        Some("WindowRead") => "window.read",
        Some(other) => other,
        None => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn closure_time_is_recorded_once_and_cleared_for_the_next_generation() {
        let mut state = ChannelState::new(1);
        let _ = state.begin_generation(Generation(1));
        assert!(state.stats(0).closed_at.is_none());
        let _ = state.on_closed(Generation(1), CloseReason::RemoteClosed { code: None });
        let closed_at = state.stats(0).closed_at.expect("closure time");
        let _ = state.on_closed(
            Generation(1),
            CloseReason::Error(std::io::ErrorKind::BrokenPipe),
        );
        let _ = state.on_closed(Generation(0), CloseReason::RemoteClosed { code: None });
        assert_eq!(state.stats(0).closed_at, Some(closed_at));
        let _ = state.begin_generation(Generation(2));
        assert!(state.stats(0).closed_at.is_none());
    }

    fn pending_answer() -> (Actor, String, oneshot::Receiver<Result<(), RespondError>>) {
        let mut state = ChannelState::new(3);
        let _ = state.begin_generation(Generation(1));
        let id = "srq-000000000005".to_owned();
        let _ = state.on_frame(Generation(1), &sudo_frame(&id), Instant::now());
        state.requests[&id].store(WRITING, Ordering::SeqCst);
        let (sender, _) = mpsc::unbounded_channel();
        let (inbound, _) = mpsc::unbounded_channel();
        let (result, receiver) = oneshot::channel();
        let actor = Actor {
            state,
            pending: HashMap::new(),
            tokens: HashMap::new(),
            answers: HashMap::from([(id.clone(), result)]),
            answer_queue: HashMap::new(),
            transport: None,
            inbound,
            stream_alive: true,
            backlog: 0,
            generation: Arc::new(AtomicU64::new(1)),
            commands: sender.downgrade(),
            recorder: None,
        };
        (actor, id, receiver)
    }

    #[tokio::test]
    async fn cancelled_answer_does_not_report_a_successful_write() {
        let (mut actor, id, result) = pending_answer();
        actor.state.cancel(&id);
        actor
            .socket_event(SocketEvent::Written(Generation(1), id, Ok(())))
            .await;
        assert_eq!(
            result.await.expect("answer completion"),
            Err(RespondError::Cancelled)
        );
        assert_eq!(actor.state.stats(0).open_server_requests, 0);
    }

    #[tokio::test]
    async fn failed_answer_write_remains_answerable() {
        let (mut actor, id, result) = pending_answer();
        actor
            .socket_event(SocketEvent::Written(
                Generation(1),
                id.clone(),
                Err(RespondError::NotConnected),
            ))
            .await;
        assert_eq!(
            result.await.expect("answer completion"),
            Err(RespondError::NotConnected)
        );
        assert_eq!(actor.state.stats(0).open_server_requests, 1);
        assert!(
            actor
                .state
                .answer(&id, Answer::Result(json!({"value":""})))
                .is_ok()
        );
    }

    #[tokio::test]
    async fn closing_client_cancels_an_answer_waiting_for_its_write() {
        let (mut actor, _, result) = pending_answer();
        actor.shutdown().await;
        assert_eq!(
            result.await.expect("answer completion"),
            Err(RespondError::Cancelled)
        );
        assert_eq!(actor.state.stats(0).open_server_requests, 0);
    }

    #[tokio::test]
    async fn completed_answer_is_not_reopened_when_replacing_its_writer() {
        let (mut actor, id, result) = pending_answer();
        actor.state.requests[&id].store(ANSWERED, Ordering::SeqCst);
        actor.disconnect_writer().await;
        assert_eq!(result.await.expect("answer completion"), Ok(()));
        assert!(matches!(
            actor.state.answer(&id, Answer::Result(json!({"value":""}))),
            Err(RespondError::AlreadyAnswered)
        ));
    }

    #[tokio::test]
    async fn completed_answer_stays_successful_when_close_precedes_its_acknowledgement() {
        for cancel_first in [false, true] {
            let (mut actor, id, result) = pending_answer();
            if cancel_first {
                actor.state.cancel(&id);
            }
            mark_written(&actor.state.requests[&id]);
            actor.shutdown().await;
            assert_eq!(result.await.expect("answer completion"), Ok(()));
            assert!(matches!(
                actor.state.answer(&id, Answer::Result(json!({"value":""}))),
                Err(RespondError::Cancelled)
            ));
        }
    }

    #[tokio::test]
    async fn control_frames_count_as_traffic_and_stale_closes_count_once() {
        let (mut actor, _, _) = pending_answer();
        let before = actor.state.stats(0);
        actor
            .socket_event(SocketEvent::Control(Generation(1)))
            .await;
        assert_eq!(
            actor.state.stats(0).frames_received,
            before.frames_received + 1
        );
        actor
            .socket_event(SocketEvent::Control(Generation(0)))
            .await;
        assert_eq!(
            actor.state.stats(0).frames_received,
            before.frames_received + 1
        );
        assert_eq!(actor.state.stats(0).stale_generation_frames, 1);
        actor
            .socket_event(SocketEvent::CloseFrame(
                Generation(1),
                CloseReason::RemoteClosed { code: None },
            ))
            .await;
        assert_eq!(
            actor.state.stats(0).frames_received,
            before.frames_received + 2
        );
        actor
            .socket_event(SocketEvent::CloseFrame(
                Generation(0),
                CloseReason::RemoteClosed { code: None },
            ))
            .await;
        assert_eq!(actor.state.stats(0).stale_generation_frames, 2);
        assert_eq!(
            actor.state.stats(0).frames_received,
            before.frames_received + 2
        );
    }

    fn sudo_frame(id: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "sudo",
            "params": {"session_id": "s1", "command": "apt list"}
        }))
        .expect("sudo frame")
    }

    #[test]
    fn stale_generation_frame_is_discarded_and_counted() {
        let mut state = ChannelState::new(3);
        let _ = state.begin_generation(Generation(1));
        let entered = state.begin_generation(Generation(2));
        assert!(entered.iter().any(|effect| matches!(
            effect,
            Effect::Deliver(Inbound::Closed(ClosedConnection {
                generation: Generation(1),
                reason: CloseReason::Replaced,
            }))
        )));
        let id = state.register_call("runtime.describe");
        let now = Instant::now();
        let reply = format!(r#"{{"jsonrpc":"2.0","id":{id},"result":{{}}}}"#);
        let event = br#"{"jsonrpc":"2.0","method":"event","params":{"type":"message.delta","payload":{"text":"x"}}}"#;
        let request = sudo_frame("srq-0123456789ab");
        for frame in [reply.as_bytes(), event.as_slice(), request.as_slice()] {
            assert!(state.on_frame(Generation(1), frame, now).is_empty());
        }
        assert!(
            state
                .on_closed(
                    Generation(1),
                    CloseReason::RemoteClosed { code: Some(1000) }
                )
                .is_empty()
        );
        let stats = state.stats(0);
        assert_eq!(stats.stale_generation_frames, 4);
        assert_eq!(stats.generation, Generation(2));
        assert_eq!(stats.pending_calls, 1);
        assert!(stats.frames_received == 0 || stats.generation == Generation(2));
    }

    #[tokio::test]
    async fn responder_first_answer_wins() {
        let mut state = ChannelState::new(3);
        let _ = state.begin_generation(Generation(1));
        let id = "srq-0123456789ab";
        let _ = state.on_frame(Generation(1), &sudo_frame(id), Instant::now());
        let first = state
            .answer(id, Answer::Result(json!({"value": "one"})))
            .expect("first answer");
        let text = match first {
            Effect::Write { text, .. } => text,
            Effect::Deliver(_) | Effect::Complete { .. } => panic!("the first answer is one write"),
        };
        let parsed: Value = serde_json::from_str(&text).expect("answer json");
        assert_eq!(parsed.get("id").and_then(Value::as_str), Some(id));
        assert!(parsed.get("method").is_none());
        assert_eq!(
            parsed.pointer("/result/value").and_then(Value::as_str),
            Some("one")
        );
        let second = state.answer(id, Answer::Result(json!({"value": "two"})));
        assert!(matches!(second, Err(RespondError::AlreadyAnswered)));

        let event = decode_frame(
            Generation(1),
            br#"{"method":"event","params":{"type":"message.delta","session_id":"s1","payload":{"text":"hi"}}}"#,
            &mut |_| panic!("an event does not build a responder"),
        );
        assert!(matches!(event, Decoded::Inbound(Inbound::Event(_))));
        let request = decode_frame(Generation(1), &sudo_frame(id), &mut |seen| {
            assert_eq!(seen, id);
            Responder::detached(Arc::new(AtomicU64::new(0)))
        });
        assert!(matches!(
            request,
            Decoded::Inbound(Inbound::ServerRequest(_))
        ));
        let reply = decode_frame(
            Generation(1),
            br#"{"id":3,"result":{"ok":true}}"#,
            &mut |_| panic!("a reply does not build a responder"),
        );
        assert!(matches!(reply, Decoded::Reply(_)));
        let garbage = decode_frame(Generation(1), b"not json", &mut |_| {
            panic!("garbage builds nothing")
        });
        assert!(matches!(
            garbage,
            Decoded::Inbound(Inbound::Malformed(ref frame)) if frame.kind == MalformedKind::NotJson
        ));

        let counter = Arc::new(AtomicU64::new(0));
        let detached = Responder::detached(Arc::clone(&counter));
        detached
            .respond(&json!({"value": "hidden"}))
            .await
            .expect("detached swallows");
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn stats_counters_follow_their_rules() {
        let mut state = ChannelState::new(3);
        let _ = state.begin_generation(Generation(1));
        let now = Instant::now();
        let before = state.stats(0);
        assert_eq!(before.frames_received, 0);
        assert_eq!(before.last_frame_at, None);
        assert_eq!(before.generation, Generation(1));

        let _ = state.on_frame(Generation(1), br#"{"id":99,"result":{}}"#, now);
        let _ = state.on_frame(Generation(1), b"not json", now);
        let _ = state.on_frame(Generation(1), &sudo_frame("srq-000000000001"), now);
        let open = state.stats(0);
        assert_eq!(open.frames_received, 3);
        assert_eq!(open.last_frame_at, Some(now));
        assert_eq!(open.unmatched_replies, 1);
        assert_eq!(open.malformed_frames, 1);
        assert_eq!(open.open_server_requests, 1);

        let _ = state.answer("srq-000000000001", Answer::Result(json!({"value": ""})));
        assert_eq!(state.stats(0).open_server_requests, 0);

        let _ = state.on_frame(Generation(1), &sudo_frame("srq-000000000002"), now);
        state.ignore("srq-000000000002");
        assert_eq!(state.stats(0).open_server_requests, 0);

        let _ = state.on_frame(Generation(1), &sudo_frame("srq-000000000003"), now);
        let dropped = state.drop_undecided("srq-000000000003");
        assert!(
            dropped
                .iter()
                .any(|effect| matches!(effect, Effect::Write { .. }))
        );
        let after_drop = state.stats(0);
        assert_eq!(after_drop.open_server_requests, 0);
        assert_eq!(after_drop.dropped_without_decision, 1);

        let _ = state.on_frame(Generation(1), &sudo_frame("srq-000000000004"), now);
        state.cancel("srq-000000000004");
        assert_eq!(state.stats(0).open_server_requests, 0);

        let stale_before = state.stats(0).stale_generation_frames;
        let totals = state.stats(0);
        let _ = state.on_frame(Generation(0), br#"{"id":1,"result":{}}"#, now);
        let after_stale = state.stats(0);
        assert_eq!(after_stale.stale_generation_frames, stale_before + 1);
        assert_eq!(after_stale.frames_received, totals.frames_received);
        assert_eq!(after_stale.unmatched_replies, totals.unmatched_replies);
        assert_eq!(after_stale.malformed_frames, totals.malformed_frames);
        assert_eq!(
            after_stale.dropped_without_decision,
            totals.dropped_without_decision
        );

        let kept_unmatched = after_stale.unmatched_replies;
        let kept_malformed = after_stale.malformed_frames;
        let kept_dropped = after_stale.dropped_without_decision;
        let kept_stale = after_stale.stale_generation_frames;
        let _ = state.begin_generation(Generation(2));
        let restarted = state.stats(2);
        assert_eq!(restarted.generation, Generation(2));
        assert_eq!(restarted.frames_received, 0);
        assert_eq!(restarted.last_frame_at, None);
        assert_eq!(restarted.inbound_backlog, 2);
        assert_eq!(restarted.unmatched_replies, kept_unmatched);
        assert_eq!(restarted.malformed_frames, kept_malformed);
        assert_eq!(restarted.dropped_without_decision, kept_dropped);
        assert_eq!(restarted.stale_generation_frames, kept_stale);

        let _ = state.on_frame(Generation(2), &sudo_frame("srq-000000000005"), now);
        let _ = state.close_client();
        let closed = state.stats(0);
        assert_eq!(closed.open_server_requests, 0);
        assert_eq!(closed.pending_calls, 0);
        assert_eq!(closed.dropped_without_decision, kept_dropped);
    }
}

#[cfg(test)]
mod raw_stream_event_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preserves_stamped_untyped_stream_payloads() {
        for payload in [
            json!({"tool_call_id":"call-1", "tool_name":"terminal"}),
            json!(null),
        ] {
            let params = json!({"type":"tool.start", "session_id":"s", "admission_id":"a", "seq":1, "replay_epoch":"e", "payload":payload});
            let frame = decode_event_params(&params).expect("stamped event is retained");
            assert_eq!(frame.session_id.as_deref(), Some("s"));
            assert_eq!(frame.seq, Some(1));
            assert_eq!(frame.type_name(), Some("tool.start"));
            assert!(
                matches!(frame.notification, hermes_contract::events::Notification::Unknown(raw) if raw == params)
            );
        }
        assert!(decode_event_params(&json!({"type":"message.delta", "payload":1})).is_err());
        assert!(decode_event_params(&json!({"type":"tool.start", "session_id":"s", "admission_id":"a", "seq":"bad", "replay_epoch":"e", "payload":{}})).is_err());
    }
}
