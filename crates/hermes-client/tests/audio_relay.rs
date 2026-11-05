use hermes_client::remote::{RemoteConfig, RemoteName, TokenSet, TokenStore};
use hermes_client::{
    RestClient, RestOptions, Ticket, TicketError, TicketMinter,
    audio_api::{AUDIO_LIMIT, AudioRelay, RESPONSE_LIMIT},
};
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn audio_deadline_is_longer_than_the_json_helper_deadline() {
    let json_deadline = Duration::from_secs(15);
    assert_eq!(hermes_client::audio_api::DEADLINE, Duration::from_secs(120));
    assert!(hermes_client::audio_api::DEADLINE > json_deadline);
}

struct Minter;
impl TicketMinter for Minter {
    fn mint(&self) -> Pin<Box<dyn Future<Output = Result<Ticket, TicketError>> + Send + '_>> {
        Box::pin(async { Ticket::parse("local-secret".into()) })
    }
}

fn local(addr: std::net::SocketAddr) -> AudioRelay {
    AudioRelay::Local(Arc::new(
        RestClient::new(
            &format!("http://{addr}"),
            Arc::new(Minter),
            RestOptions {
                max_body: RESPONSE_LIMIT,
                ..RestOptions::default()
            },
        )
        .unwrap(),
    ))
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut buf = [0; 8192];
    loop {
        let n = stream.read(&mut buf).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&bytes[..end]);
            let len = header
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|v| v.parse::<usize>().ok())
                })
                .unwrap_or(0);
            if bytes.len() >= end + 4 + len {
                break;
            }
        }
    }
    String::from_utf8(bytes).unwrap()
}
async fn reply(stream: &mut tokio::net::TcpStream, status: u16, body: &str) {
    stream.write_all(format!("HTTP/1.1 {status} response\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
}

#[tokio::test]
async fn local_production_relay_uses_ticket_profile_and_data_urls() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = local(listener.local_addr().unwrap());
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_request(&mut stream).await;
        assert!(request.starts_with("POST /api/audio/transcribe?profile=work%2F%26+team "));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("x-hermes-gateway-ticket: local-secret")
        );
        assert!(request.contains("data:audio/wav;base64,YXVkaW8="));
        reply(&mut stream, 200, r#"{"ok":true,"transcript":"hello"}"#).await;
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_request(&mut stream).await;
        assert!(request.starts_with("POST /api/audio/speak?profile=work "));
        assert!(request.contains(r#""text":"hello""#));
        reply(
            &mut stream,
            200,
            r#"{"ok":true,"data_url":"data:audio/wav;base64,YXVkaW8="}"#,
        )
        .await;
    });
    assert_eq!(
        relay.transcribe("work/& team", b"audio").await.unwrap(),
        "hello"
    );
    assert_eq!(relay.speak("work", "hello").await.unwrap(), b"audio");
    peer.await.unwrap();
}

#[tokio::test]
async fn remote_authenticates_refreshes_and_redacts_errors() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let temp = tempfile::tempdir().unwrap();
    let name = RemoteName::parse("work").unwrap();
    let store = TokenStore::new(temp.path(), name.clone());
    store
        .save(&TokenSet::for_test(
            "old-secret",
            "refresh-secret",
            i64::MAX,
            1,
        ))
        .unwrap();
    let relay = AudioRelay::Remote {
        config: RemoteConfig {
            name,
            base_url: format!("http://{}/prefix", listener.local_addr().unwrap())
                .parse()
                .unwrap(),
            provider: None,
        },
        store: store.clone(),
    };
    let peer = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let req = read_request(&mut s).await;
        assert!(req.starts_with("POST /prefix/api/audio/speak?profile=p%2Fq "));
        assert!(
            req.to_ascii_lowercase()
                .contains("authorization: bearer old-secret")
        );
        reply(&mut s, 401, r#"{"detail":"old-secret"}"#).await;
        let (mut s, _) = listener.accept().await.unwrap();
        let req = read_request(&mut s).await;
        assert!(req.starts_with("POST /prefix/auth/native/refresh "));
        assert!(req.contains("refresh-secret"));
        reply(&mut s, 200, r#"{"token_type":"Bearer","access_token":"new-secret","refresh_token":"next-secret","expires_at":4102444800,"provider":"basic","user_id":"fixture"}"#).await;
        let (mut s, _) = listener.accept().await.unwrap();
        let req = read_request(&mut s).await;
        assert!(
            req.to_ascii_lowercase()
                .contains("authorization: bearer new-secret")
        );
        reply(&mut s, 403, r#"{"detail":"new-secret refresh-secret"}"#).await;
    });
    let err = relay.speak("p/q", "hello").await.unwrap_err();
    assert!(!err.contains("secret"));
    assert_eq!(
        store.load().unwrap().unwrap().access_for_test(),
        "new-secret"
    );
    peer.await.unwrap();
}

#[tokio::test]
async fn caps_inputs_and_response_without_widening_ordinary_requests() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = local(listener.local_addr().unwrap());
    assert!(
        relay
            .transcribe("p", &vec![0; AUDIO_LIMIT + 1])
            .await
            .is_err()
    );
    assert!(relay.transcribe("p", &[]).await.is_err());
    let peer = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        read_request(&mut s).await;
        s.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                RESPONSE_LIMIT + 1
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    });
    assert!(relay.speak("p", "hi").await.is_err());
    peer.await.unwrap();
    let ordinary = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", ordinary.local_addr().unwrap())
        .parse()
        .unwrap();
    let peer = tokio::spawn(async move {
        let (mut stream, _) = ordinary.accept().await.unwrap();
        read_request(&mut stream).await;
        let body = serde_json::json!({"auth_required": true, "auth_flows": ["native_pkce"], "padding": "x".repeat(1024 * 1024)}).to_string();
        let _ = stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            )
            .await;
        let _ = stream.write_all(body.as_bytes()).await;
    });
    assert!(hermes_client::remote::probe_status(&base).await.is_err());
    peer.await.unwrap();
}

#[tokio::test]
async fn local_audio_accepts_requests_larger_than_the_normal_json_limit() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = local(listener.local_addr().unwrap());
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_request(&mut stream).await;
        assert!(request.len() > 8 * 1024 * 1024);
        reply(&mut stream, 200, r#"{"transcript":"large"}"#).await;
    });
    assert_eq!(
        relay
            .transcribe("p", &vec![0; 7 * 1024 * 1024])
            .await
            .unwrap(),
        "large"
    );
    peer.await.unwrap();
}

#[tokio::test]
async fn relay_deadline_covers_a_stalled_peer() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = local(listener.local_addr().unwrap());
    let (ready, wait) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        read_request(&mut s).await;
        let _ = ready.send(());
        std::future::pending::<()>().await;
    });
    let task = tokio::spawn(async move { relay.speak("p", "hi").await });
    wait.await.unwrap();
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(121)).await;
    assert!(task.await.unwrap().is_err());
    peer.abort();
}
