//! Attach to the local Hermes gateway: control socket -> ticket -> WebSocket.
use anyhow::{anyhow, bail, Context, Result};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpStream, UnixStream};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;
pub type WsSink = SplitSink<Ws, Message>;
pub type WsRead = SplitStream<Ws>;

pub struct Attached {
    pub sink: WsSink,
    pub read: WsRead,
    pub instance_id: String,
    pub profile_id: String,
}

async fn control(sock: &str, req: Value) -> Result<Value> {
    let mut s = UnixStream::connect(sock).await.with_context(|| format!("connect {sock}"))?;
    s.write_all(format!("{req}\n").as_bytes()).await?;
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).await?;
    if line.is_empty() {
        bail!("control socket closed without a reply");
    }
    Ok(serde_json::from_str(&line)?)
}

pub fn default_home() -> String {
    format!("{}/.hermes", std::env::var("HOME").unwrap_or_default())
}

/// Returns the open socket (split) after the server accepted the upgrade. The first
/// `gateway.ready` frame is left unread on `read` so the caller can record it.
pub async fn attach(home: &str) -> Result<Attached> {
    let sock = format!("{home}/gateway.sock");
    let ident = control(&sock, json!({"verb":"identify","id":1,"protocol":1})).await?;
    let res = ident.get("result").unwrap_or(&ident);
    let instance_id = res["instance_id"].as_str().ok_or_else(|| anyhow!("identify: no instance_id"))?.to_owned();
    let origin = res["api_origin"].as_str().ok_or_else(|| anyhow!("identify: no api_origin"))?.to_owned();
    let profile_id = res["served_profiles"]
        .as_array()
        .and_then(|a| a.iter().find_map(|p| p["profile_id"].as_str()))
        .unwrap_or(home)
        .to_owned();
    let grant = control(
        &sock,
        json!({"protocol":1,"id":2,"verb":"session-ticket","params":{
            "profile_id": profile_id, "instance_id": instance_id,
            "purpose":"interactive","scope":"profile"}}),
    )
    .await?;
    if grant.get("ok").and_then(Value::as_bool) != Some(true) {
        bail!("session-ticket refused: {}", grant.get("error").map(|e| e.to_string()).unwrap_or_default());
    }
    let ticket = grant["result"]["ticket"].as_str().ok_or_else(|| anyhow!("no ticket in grant"))?;
    let url = format!("{}/api/ws", origin.replacen("http", "ws", 1));
    let mut req = url.as_str().into_client_request()?;
    req.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        format!("hermes-gateway-v1, hermes-gateway-ticket.{ticket}").parse()?,
    );
    let (ws, resp) = tokio_tungstenite::connect_async(req).await.map_err(|e| anyhow!("websocket upgrade failed: {}", redact(&e.to_string())))?;
    let echoed = resp.headers().get("sec-websocket-protocol").and_then(|v| v.to_str().ok()).unwrap_or("");
    if echoed != "hermes-gateway-v1" {
        bail!("server echoed unexpected subprotocol {echoed:?}");
    }
    let (sink, read) = ws.split();
    Ok(Attached { sink, read, instance_id, profile_id })
}

fn redact(s: &str) -> String {
    match s.find("hermes-gateway-ticket.") {
        Some(i) => format!("{}hermes-gateway-ticket.<redacted>", &s[..i]),
        None => s.to_owned(),
    }
}
