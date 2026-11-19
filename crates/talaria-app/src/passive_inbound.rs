//! Readers that are not the session's prompt owner.
//!
//! Dropping one ignores server requests still sitting in its queue. A dropped responder would
//! otherwise write `-32603` and settle the prompt for every viewer.

use hermes_client::liveness::LinkEvent;
use hermes_client::{Inbound, InboundStream};
use tokio::sync::mpsc;

/// Leave a server request for another viewer. Other inbound items are discarded.
pub(crate) fn ignore_request(item: Inbound) {
    if let Inbound::ServerRequest(request) = item {
        request.responder.ignore();
    }
}

/// Leave a server request carried by a link event for another viewer.
pub(crate) fn ignore_link_event(event: LinkEvent) {
    if let LinkEvent::Inbound { item, .. } = event {
        ignore_request(item);
    }
}

/// An inbound stream that ignores queued server requests when it is dropped.
pub(crate) struct PassiveInbound {
    inbound: InboundStream,
}

impl PassiveInbound {
    pub(crate) fn new(inbound: InboundStream) -> Self {
        Self { inbound }
    }

    pub(crate) async fn next(&mut self) -> Option<Inbound> {
        self.inbound.next().await
    }
}

impl Drop for PassiveInbound {
    fn drop(&mut self) {
        self.inbound.ignore_queued_requests();
    }
}

/// A link-event queue that ignores queued server requests when it is dropped.
pub(crate) struct PassiveEvents {
    events: mpsc::Receiver<LinkEvent>,
}

impl PassiveEvents {
    pub(crate) fn new(events: mpsc::Receiver<LinkEvent>) -> Self {
        Self { events }
    }

    pub(crate) async fn recv(&mut self) -> Option<LinkEvent> {
        self.events.recv().await
    }
}

impl Drop for PassiveEvents {
    fn drop(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            ignore_link_event(event);
        }
    }
}
