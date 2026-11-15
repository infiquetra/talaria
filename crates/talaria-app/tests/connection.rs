use std::path::PathBuf;

use futures_util::{SinkExt, StreamExt};
use hermes_client::ConnectionParts;
use hermes_client::liveness::{Dial, DialFailure, Dialed};
use serde_json::{Value, json};
use talaria_app::connection::ConnectionService;
use talaria_app::launch::{Launch, StartMode};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{MaybeTlsStream, accept_async, client_async, tungstenite::Message};

struct TestDial {
    parts: Option<ConnectionParts>,
}

impl Dial for TestDial {
    async fn dial(&mut self, _generation: u64) -> Result<Dialed, DialFailure> {
        let parts = self.parts.take().expect("parts consumed once");
        Ok(Dialed {
            parts,
            process_epoch: "0123abcd".into(),
            heartbeat: false,
            capabilities: Vec::new(),
        })
    }
}

#[tokio::test]
async fn open_with_dial_skips_a_null_skin_colour() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");

    let client_task = tokio::spawn(async move {
        let stream = TcpStream::connect(addr).await.expect("connect");
        stream.set_nodelay(true).expect("client nodelay");
        let (ws, _) = client_async("ws://127.0.0.1/", MaybeTlsStream::Plain(stream))
            .await
            .expect("client handshake");
        ws
    });

    let (server_stream, _) = listener.accept().await.expect("accept");
    server_stream.set_nodelay(true).expect("server nodelay");
    let mut server_ws = accept_async(server_stream).await.expect("server handshake");
    let client_ws = client_task.await.expect("client task");
    let (sink, stream) = client_ws.split();
    let parts = ConnectionParts::for_test(sink, stream, 3, Vec::new());

    let skin = json!({
        "name": "nullcolour",
        "colors": {"banner": null, "prompt": "#FFD700"},
        "light_colors": {"ui_ok": null},
        "dark_colors": {"ui_warn": null},
    });

    let server_task = tokio::spawn(async move {
        let frame = loop {
            match server_ws
                .next()
                .await
                .expect("server frame")
                .expect("ws ok")
            {
                Message::Text(text) => break text,
                Message::Ping(payload) => {
                    let _ = server_ws.send(Message::Pong(payload)).await;
                }
                _ => {}
            }
        };
        let request: Value = serde_json::from_str(&frame).expect("request json");
        assert_eq!(request["method"], "runtime.describe");
        let reply = json!({
            "jsonrpc": "2.0",
            "id": request["id"],
            "result": {
                "instance_id": "0123456789abcdef0123456789abcdef",
                "profile_id": "fixture",
                "authority_epoch": 3,
                "capabilities": ["session-authority-v1", "durable-admission-v1", "event-replay-v1"],
                "session_create": {"sources": ["local"], "parameters": ["text"]},
                "skin": skin,
            }
        });
        server_ws
            .send(Message::Text(reply.to_string().into()))
            .await
            .expect("send reply");
    });

    let launch = Launch {
        mode: StartMode::New,
        model: None,
        provider: None,
        effort: None,
        profile: "fixture".into(),
        folder: PathBuf::new(),
        record: None,
        bench_replay: None,
        no_ensure: true,
    };

    let open = ConnectionService::open_with_dial(&launch, TestDial { parts: Some(parts) })
        .await
        .expect("open_with_dial should skip null skin colour");
    server_task.await.expect("server task");

    let describe_skin = open.describe.skin.expect("describe skin");
    assert_eq!(
        describe_skin.colors.get("prompt").map(String::as_str),
        Some("#FFD700")
    );
    assert!(!describe_skin.colors.contains_key("banner"));
    assert!(!describe_skin.light_colors.contains_key("ui_ok"));
    assert!(!describe_skin.dark_colors.contains_key("ui_warn"));
    open.link.close().await;
}
