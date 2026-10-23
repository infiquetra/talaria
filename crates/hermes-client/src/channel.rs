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

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

use crate::connect::{ConnectionParts, WsSink};
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
        let _ = (self, formatter);
        todo!("P1-02")
    }
}

impl std::fmt::Display for InboundEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = (self, formatter);
        todo!("P1-02")
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
/// `label` is at most 64 characters of an event type. `Debug` and `Display` show the generation,
/// the kind, the length, and that label.
#[derive(Clone)]
pub struct MalformedFrame {
    /// The connection that carried the frame.
    pub generation: Generation,
    /// Which rule rejected the frame.
    pub kind: MalformedKind,
    /// Byte length of the payload.
    pub length: usize,
    /// An event type, when one could be read. Never frame text.
    pub label: Option<String>,
}

impl std::fmt::Debug for MalformedFrame {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = (self, formatter);
        todo!("P1-02")
    }
}

impl std::fmt::Display for MalformedFrame {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = (self, formatter);
        todo!("P1-02")
    }
}

/// Something the gateway sent that is not a reply to a call.
///
/// Not `Clone`: a [`ServerRequest`] owns a [`Responder`], and a responder is not cloned.
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
        let _ = (self, formatter);
        todo!("P1-02")
    }
}

impl std::fmt::Display for ServerRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = (self, formatter);
        todo!("P1-02")
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
    _not_constructed: Arc<()>,
}

impl Responder {
    /// Write a `result` for this request and wait until the write finishes. There is no timer.
    pub async fn respond<T: Serialize>(&self, result: &T) -> Result<(), RespondError> {
        let _ = (self, result);
        todo!("P1-02")
    }

    /// Write an `error` for this request and wait until the write finishes. There is no timer.
    ///
    /// For `sudo` and `secret`, the agent reads any error as a skip.
    pub async fn fail(&self, code: i64, message: &str) -> Result<(), RespondError> {
        let _ = (self, code, message);
        todo!("P1-02")
    }

    /// Leave the request for another viewer. Nothing is written.
    pub fn ignore(self) {
        let _ = self;
        todo!("P1-02")
    }

    /// A responder for replay. `respond` and `fail` succeed, write nothing, and add one to `swallowed`.
    #[doc(hidden)]
    pub fn detached(swallowed: Arc<AtomicU64>) -> Self {
        let _ = swallowed;
        todo!("P1-02")
    }
}

impl std::fmt::Debug for Responder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = (self, formatter);
        todo!("P1-02")
    }
}

impl std::fmt::Display for Responder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = (self, formatter);
        todo!("P1-02")
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
        let _ = (self, formatter);
        todo!("P1-02")
    }
}

impl std::fmt::Display for RespondError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = (self, formatter);
        todo!("P1-02")
    }
}

/// What [`decode_frame`] made of one payload.
///
/// Not `Clone`: an inbound server request owns a [`Responder`].
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
        let _ = (self, formatter);
        todo!("P1-02")
    }
}

/// The typed channel. Cheap to clone. One actor serves every clone.
#[derive(Clone)]
pub struct Client {
    _not_constructed: Arc<()>,
}

/// The inbound queue. [`InboundStream::next`] is `None` only after the final
/// [`CloseReason::ClientClosed`], or when every [`Client`] clone is dropped.
pub struct InboundStream {
    _not_constructed: Arc<()>,
}

impl Client {
    /// Start the actor on `parts`. The generation is 1. The id counter starts at `parts.next_request_id`.
    pub fn start(parts: ConnectionParts) -> (Self, InboundStream) {
        let _ = parts;
        todo!("P1-02")
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
        let _ = (self, parts);
        todo!("P1-02")
    }

    /// End the client. Pending calls become unknown. Open responders become [`RespondError::Cancelled`].
    ///
    /// One [`CloseReason::ClientClosed`] is delivered, and it is the last item.
    pub async fn close(&self) {
        let _ = self;
        todo!("P1-02")
    }

    /// The generation the client is on.
    pub fn generation(&self) -> Generation {
        let _ = self;
        todo!("P1-02")
    }

    /// A snapshot. A `Forget` from a dropped caller is applied before this returns.
    pub async fn stats(&self) -> ChannelStats {
        let _ = self;
        todo!("P1-02")
    }
}

impl InboundStream {
    /// The next inbound item.
    pub async fn next(&mut self) -> Option<Inbound> {
        let _ = self;
        todo!("P1-02")
    }
}

/// Write one text frame. This is the only function that sends text on the socket.
#[doc(hidden)]
pub async fn write_frame(
    sink: &mut WsSink,
    generation: Generation,
    text: &str,
) -> Result<(), std::io::Error> {
    let _ = (sink, generation, text);
    todo!("P1-02")
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
    let _ = (generation, raw, responders);
    todo!("P1-02")
}

/// What the actor should do after one state change. No sockets and no clocks.
///
/// Not `Clone`: a [`Effect::Deliver`] owns an [`Inbound`], and a responder is not cloned.
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
    _not_constructed: (),
}

impl ChannelState {
    pub(crate) fn new(next_request_id: u64) -> Self {
        let _ = next_request_id;
        todo!("P1-02")
    }

    /// Enter `generation`. Fail the previous generation's calls and emit [`CloseReason::Replaced`]
    /// when that generation had not already closed. `frames_received` and `last_frame_at` restart.
    pub(crate) fn begin_generation(&mut self, generation: Generation) -> Vec<Effect> {
        let _ = (self, generation);
        todo!("P1-02")
    }

    /// Take the next id and remember `method` on the current generation.
    pub(crate) fn register_call(&mut self, method: &str) -> u64 {
        let _ = (self, method);
        todo!("P1-02")
    }

    /// Drop a pending call. A later reply is unmatched.
    pub(crate) fn forget_call(&mut self, id: u64) {
        let _ = (self, id);
        todo!("P1-02")
    }

    /// Apply one frame that claims `generation`. `now` is the caller's clock, not read here.
    pub(crate) fn on_frame(
        &mut self,
        generation: Generation,
        raw: &[u8],
        now: Instant,
    ) -> Vec<Effect> {
        let _ = (self, generation, raw, now);
        todo!("P1-02")
    }

    /// End `generation` for `reason`, when it is the current one and it has not already closed.
    pub(crate) fn on_closed(&mut self, generation: Generation, reason: CloseReason) -> Vec<Effect> {
        let _ = (self, generation, reason);
        todo!("P1-02")
    }

    /// The first successful answer wins. A second call is [`RespondError::AlreadyAnswered`].
    pub(crate) fn answer(
        &mut self,
        request_id: &str,
        answer: Answer,
    ) -> Result<Effect, RespondError> {
        let _ = (self, request_id, answer);
        todo!("P1-02")
    }

    /// `ignore` writes nothing and closes the request.
    pub(crate) fn ignore(&mut self, request_id: &str) {
        let _ = (self, request_id);
        todo!("P1-02")
    }

    /// The responder was dropped. Counts [`ChannelStats::dropped_without_decision`].
    ///
    /// With a current connection this emits the `-32603` answer. With none, it writes nothing.
    pub(crate) fn drop_undecided(&mut self, request_id: &str) -> Vec<Effect> {
        let _ = (self, request_id);
        todo!("P1-02")
    }

    /// `request.cancel`, or the same transition without a frame.
    pub(crate) fn cancel(&mut self, request_id: &str) {
        let _ = (self, request_id);
        todo!("P1-02")
    }

    /// [`Client::close`]: pending calls unknown, open responders cancelled, one client-closed item.
    pub(crate) fn close_client(&mut self) -> Vec<Effect> {
        let _ = self;
        todo!("P1-02")
    }

    pub(crate) fn stats(&self, inbound_backlog: usize) -> ChannelStats {
        let _ = (self, inbound_backlog);
        todo!("P1-02")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

#[allow(dead_code)]
fn _duration_is_available() -> Duration {
    Duration::from_secs(0)
}
