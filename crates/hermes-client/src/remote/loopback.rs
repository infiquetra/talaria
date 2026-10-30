//! One-shot callback listener for native sign-in.
use super::SignInError;
use std::time::Duration;
use subtle::ConstantTimeEq;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

pub struct CallbackListener {
    pub listener: TcpListener,
}
impl CallbackListener {
    pub async fn bind() -> std::io::Result<Self> {
        Ok(Self {
            listener: TcpListener::bind("127.0.0.1:0").await?,
        })
    }
    pub fn redirect_uri(&self) -> Url {
        // The listener is bound by this type to an IP literal, so its address is a canonical URL.
        let addr = self
            .listener
            .local_addr()
            .expect("bound callback listener has a local address");
        Url::parse(&format!("http://{addr}/callback")).expect("loopback address is a URL")
    }
    pub async fn wait(
        self,
        expected_state: &str,
        timeout: Duration,
    ) -> Result<String, SignInError> {
        tokio::time::timeout(timeout, async {
            loop {
                let (mut stream, _) = self.listener.accept().await.map_err(|_| SignInError::Io(String::new()))?;
                let mut bytes = Vec::new();
                loop {
                    let mut byte = [0];
                    if stream.read(&mut byte).await.map_err(|_| SignInError::Io(String::new()))? == 0 { break; }
                    bytes.push(byte[0]);
                    if bytes.ends_with(b"\r\n\r\n") || bytes.len() >= 16384 { break; }
                }
                let target = std::str::from_utf8(&bytes).ok().and_then(|r| r.lines().next()).and_then(|line| line.strip_prefix("GET ")).and_then(|s| s.split_whitespace().next());
                let url = target.and_then(|target| Url::parse(&format!("http://127.0.0.1{target}")).ok());
                let callback = url.as_ref().is_some_and(|url| url.path() == "/callback");
                let response = if callback { "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: 41\r\nConnection: close\r\n\r\nSign-in received. You may close this page." } else { "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n" };
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
                if callback { return parse_callback(url.as_ref().ok_or_else(|| SignInError::Denied("invalid_request".into()))?, expected_state); }
            }
        }).await.map_err(|_| SignInError::TimedOut)?
    }
}
pub(crate) fn parse_callback(url: &Url, expected: &str) -> Result<String, SignInError> {
    let pairs: Vec<_> = url.query_pairs().collect();
    if let Some((_, error)) = pairs.iter().find(|(key, _)| key == "error") {
        return Err(SignInError::Denied(
            if matches!(
                error.as_ref(),
                "access_denied" | "login_required" | "temporarily_unavailable" | "invalid_request"
            ) {
                error.to_string()
            } else {
                "denied".into()
            },
        ));
    }
    let states: Vec<_> = pairs.iter().filter(|(key, _)| key == "state").collect();
    if states.len() != 1 || !bool::from(states[0].1.as_bytes().ct_eq(expected.as_bytes())) {
        return Err(SignInError::StateMismatch);
    }
    let codes: Vec<_> = pairs.iter().filter(|(key, _)| key == "code").collect();
    if codes.len() != 1 || codes[0].1.is_empty() {
        return Err(SignInError::Denied("invalid_request".into()));
    }
    Ok(codes[0].1.to_string())
}
