//! Worker-level checks using fake helpers and a loopback HTTP peer.
use hermes_client::{
    RestClient, RestOptions, Ticket, TicketError, TicketMinter, audio_api::AudioRelay,
};
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};
use talaria_app::{app::AppMessage, voice::relay::ClientAudio};
use talaria_core::voice::client::Input;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Minter;
impl TicketMinter for Minter {
    fn mint(&self) -> Pin<Box<dyn Future<Output = Result<Ticket, TicketError>> + Send + '_>> {
        Box::pin(async { Ticket::parse("fixture-ticket".into()) })
    }
}

fn relay(address: std::net::SocketAddr) -> AudioRelay {
    let client = RestClient::new(
        &format!("http://{address}"),
        Arc::new(Minter),
        RestOptions {
            get_deadline: Duration::from_secs(120),
            write_deadline: Duration::from_secs(120),
            max_body: 40 * 1024 * 1024,
        },
    )
    .unwrap();
    AudioRelay::Local(Arc::new(client))
}

#[tokio::test]
async fn remote_worker_refuses_before_starting_helper_or_contacting_relay() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("started");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let worker = ClientAudio::new(
        dir.path().into(),
        Some(relay(listener.local_addr().unwrap())),
        true,
        false,
        tx,
    );
    worker.capture(
        vec!["/usr/bin/touch".into(), marker.to_string_lossy().into()],
        "work".into(),
        "one".into(),
        1,
        1,
    );
    assert!(matches!(
        rx.recv().await,
        Some(AppMessage::ClientVoice {
            input: Input::Failed { epoch: 1 },
            ..
        })
    ));
    assert!(!marker.exists());
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
    worker.close().await;
}

#[tokio::test]
async fn closing_a_queued_capture_does_not_start_the_helper() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("started");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let worker = ClientAudio::new(dir.path().into(), None, false, false, tx);
    worker.capture(
        vec!["/usr/bin/touch".into(), marker.to_string_lossy().into()],
        "work".into(),
        "one".into(),
        1,
        1,
    );
    worker.close().await;
    assert!(!marker.exists());
}

#[tokio::test]
async fn opted_in_worker_transcribes_capture_and_reports_to_application() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut received = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let n = stream.read(&mut buffer).await.unwrap();
            if n == 0 {
                break;
            }
            received.extend_from_slice(&buffer[..n]);
            if let Some(end) = received.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&received[..end]);
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|n| n.trim().parse().ok())
                    })
                    .unwrap_or(0);
                if received.len() >= end + 4 + length {
                    break;
                }
            }
        }
        let request = String::from_utf8_lossy(&received);
        assert!(request.starts_with("POST /api/audio/transcribe?profile=work "));
        assert!(request.contains("data:audio/wav;base64,"));
        let body = r#"{"ok":true,"transcript":"hello"}"#;
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let worker = ClientAudio::new(dir.path().into(), Some(relay(address)), true, true, tx);
    worker.capture(
        vec![
            "/bin/sh".into(),
            "-c".into(),
            "read line; printf RIFFWAVE".into(),
        ],
        "work".into(),
        "one".into(),
        2,
        3,
    );
    worker.finish_capture();
    let text = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match rx.recv().await {
                Some(AppMessage::ClientVoice {
                    input: Input::Text { epoch: 3, text },
                    ..
                }) => break text,
                Some(AppMessage::ClientVoice {
                    input: Input::Failed { .. },
                    notice,
                    ..
                }) => panic!("worker failed: {notice:?}"),
                Some(_) => {}
                None => panic!("worker closed"),
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(text, "hello");
    peer.await.unwrap();
    worker.close().await;
}

#[tokio::test]
async fn closing_worker_reaps_capture_without_sending_audio() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let pid_path = dir.path().join("pid");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let worker = ClientAudio::new(
        dir.path().into(),
        Some(relay(listener.local_addr().unwrap())),
        true,
        true,
        tx,
    );
    worker.capture(
        vec![
            "/bin/sh".into(),
            "-c".into(),
            "echo $$ > \"$1\"; exec /bin/sleep 30".into(),
            "helper".into(),
            pid_path.to_string_lossy().into(),
        ],
        "work".into(),
        "one".into(),
        1,
        1,
    );
    let pid = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(pid) = std::fs::read_to_string(&pid_path) {
                break pid.trim().to_owned();
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    worker.finish_capture();
    tokio::task::yield_now().await;
    let started = std::time::Instant::now();
    worker.close().await;
    assert!(started.elapsed() < Duration::from_millis(500));
    while let Ok(message) = rx.try_recv() {
        assert!(
            !matches!(
                message,
                AppMessage::ClientVoice {
                    notice: Some(_),
                    ..
                } | AppMessage::ClientVoiceHealth { ok: false, .. }
            ),
            "intentional cancellation reported a failure"
        );
    }
    assert!(
        !std::process::Command::new("/bin/ps")
            .args(["-p", &pid])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn closing_worker_reaps_active_playback() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        let _ = stream.read(&mut request).await.unwrap();
        let body = r#"{"ok":true,"data_url":"data:audio/wav;base64,YXVkaW8="}"#;
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let pid_path = dir.path().join("player-pid");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let worker = ClientAudio::new(dir.path().into(), Some(relay(address)), true, true, tx);
    worker.speak(
        vec![
            "/bin/sh".into(),
            "-c".into(),
            "echo $$ > \"$1\"; exec /bin/sleep 30".into(),
            "helper".into(),
            pid_path.to_string_lossy().into(),
        ],
        "work".into(),
        "one".into(),
        1,
        "hello".into(),
    );
    let pid = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(pid) = std::fs::read_to_string(&pid_path) {
                break pid.trim().to_owned();
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let started = std::time::Instant::now();
    worker.close().await;
    assert!(started.elapsed() < Duration::from_millis(500));
    assert!(
        !std::process::Command::new("/bin/ps")
            .args(["-p", &pid])
            .output()
            .unwrap()
            .status
            .success()
    );
    peer.await.unwrap();
}

#[tokio::test]
async fn restarting_capture_waits_for_the_previous_helper_to_be_reaped() {
    let dir = tempfile::tempdir().unwrap();
    let pid_path = dir.path().join("old-pid");
    let next_path = dir.path().join("new-started");
    let overlap_path = dir.path().join("overlap");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let worker = ClientAudio::new(dir.path().into(), None, false, false, tx);
    worker.capture(
        vec![
            "/bin/sh".into(),
            "-c".into(),
            "echo $$ > \"$1\"; exec /bin/sleep 30".into(),
            "helper".into(),
            pid_path.to_string_lossy().into(),
        ],
        "work".into(),
        "one".into(),
        1,
        1,
    );
    let old_pid = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(pid) = std::fs::read_to_string(&pid_path) {
                break pid.trim().to_owned();
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let closing = worker.close();
    worker.capture(vec!["/bin/sh".into(), "-c".into(), "if /bin/ps -p \"$1\" >/dev/null; then /usr/bin/touch \"$2\"; fi; /usr/bin/touch \"$3\"; read line; printf audio".into(), "helper".into(), old_pid, overlap_path.to_string_lossy().into(), next_path.to_string_lossy().into()], "work".into(), "one".into(), 1, 2);
    tokio::time::timeout(Duration::from_secs(5), async {
        while !next_path.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        !overlap_path.exists(),
        "new helper started before old helper was reaped"
    );
    closing.await;
    worker.close().await;
}

#[tokio::test]
async fn closing_a_stalled_relay_does_not_block_future_capture() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buffer = [0; 4096];
        let _ = stream.read(&mut buffer).await.unwrap();
        let _ = entered.send(());
        tokio::time::sleep(Duration::from_secs(3)).await;
    });
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("new-started");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let worker = ClientAudio::new(dir.path().into(), Some(relay(address)), true, true, tx);
    worker.capture(
        vec![
            "/bin/sh".into(),
            "-c".into(),
            "read line; printf audio".into(),
        ],
        "work".into(),
        "one".into(),
        1,
        1,
    );
    worker.finish_capture();
    tokio::time::timeout(Duration::from_secs(5), waiting)
        .await
        .unwrap()
        .unwrap();
    worker.close().await;
    assert!(
        !worker.capture_blocked(),
        "a stalled relay was mistaken for an unreaped helper"
    );
    worker.capture(
        vec![
            "/bin/sh".into(),
            "-c".into(),
            "echo started > \"$1\"; read line".into(),
            "helper".into(),
            marker.to_string_lossy().into(),
        ],
        "work".into(),
        "one".into(),
        1,
        2,
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while !marker.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    worker.close().await;
    peer.abort();
}

#[tokio::test]
async fn closing_stalled_synthesis_does_not_block_future_playback() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut first, _) = listener.accept().await.unwrap();
        let mut buffer = [0; 4096];
        let _ = first.read(&mut buffer).await.unwrap();
        let _ = entered.send(());
        let _held = first;
        let (mut second, _) = listener.accept().await.unwrap();
        let _ = second.read(&mut buffer).await.unwrap();
        let body = r#"{"ok":true,"data_url":"data:audio/wav;base64,YXVkaW8="}"#;
        second
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("player-started");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let worker = ClientAudio::new(dir.path().into(), Some(relay(address)), true, true, tx);
    worker.speak(
        vec!["/bin/cat".into()],
        "work".into(),
        "one".into(),
        1,
        "first".into(),
    );
    tokio::time::timeout(Duration::from_secs(5), waiting)
        .await
        .unwrap()
        .unwrap();
    worker.close().await;
    assert!(!worker.playback_blocked());
    worker.speak(
        vec![
            "/bin/sh".into(),
            "-c".into(),
            "echo started > \"$1\"; exec /bin/cat".into(),
            "helper".into(),
            marker.to_string_lossy().into(),
        ],
        "work".into(),
        "one".into(),
        1,
        "second".into(),
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while !marker.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    worker.close().await;
    peer.await.unwrap();
}
