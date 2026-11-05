use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn transcribe_uses_profile_and_hermes_data_url() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = vec![0; 8192];
        let n = stream.read(&mut request).await.unwrap();
        let request = String::from_utf8_lossy(&request[..n]);
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
    assert_eq!(
        hermes_client::audio_api::transcribe(&url, "work", b"audio")
            .await
            .unwrap(),
        "hello"
    );
    peer.await.unwrap();
}

#[tokio::test]
async fn speak_decodes_hermes_data_url() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = vec![0; 8192];
        let n = stream.read(&mut request).await.unwrap();
        let request = String::from_utf8_lossy(&request[..n]);
        assert!(request.starts_with("POST /api/audio/speak?profile=work "));
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
    assert_eq!(
        hermes_client::audio_api::speak(&url, "work", "hello")
            .await
            .unwrap(),
        b"audio"
    );
    peer.await.unwrap();
}
