#![allow(dead_code)]

use serde_json::Value;

pub fn deep_merge(_base: &mut Value, _patch: &Value) {
    todo!("settings stub")
}

pub fn effective_config(_defaults: &Value, _raw: &Value) -> Value {
    todo!("settings stub")
}

pub struct MockRest;

impl MockRest {
    pub async fn start() -> Self {
        todo!("settings stub")
    }

    pub fn with_route<F>(self, _method: &str, _path: &str, _handler: F) -> Self
    where
        F: Send + Sync + 'static,
    {
        todo!("settings stub")
    }
}

#[derive(Clone, Debug)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl MockRest {
    pub fn origin(&self) -> String {
        todo!("settings stub")
    }
    pub async fn requests(&self) -> Vec<RecordedRequest> {
        todo!("settings stub")
    }
    pub async fn fail_status(
        &self,
        _method: &str,
        _path: &str,
        _status: u16,
        _detail: Option<&str>,
    ) {
        todo!("settings stub")
    }
    pub async fn first_401(&self, _method: &str, _path: &str) {
        todo!("settings stub")
    }
    pub async fn drop_after_request(&self, _method: &str, _path: &str) {
        todo!("settings stub")
    }
    pub async fn delay(&self, _method: &str, _path: &str, _duration: std::time::Duration) {
        todo!("settings stub")
    }
    pub async fn response_body(&self, _method: &str, _path: &str, _body: Vec<u8>) {
        todo!("settings stub")
    }
}

#[derive(Default)]
pub struct CountingMinter(std::sync::atomic::AtomicUsize);
impl CountingMinter {
    pub fn count(&self) -> usize {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}
impl hermes_client::rest::TicketMinter for CountingMinter {
    fn mint(
        &self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<hermes_client::Ticket, hermes_client::TicketError>,
                > + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            let n = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            hermes_client::Ticket::parse(format!("TICKET-SENTINEL-{n}"))
        })
    }
}
