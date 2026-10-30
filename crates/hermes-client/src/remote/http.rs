//! Bounded HTTP requests without redirects, automatic retries, or response-body diagnostics.
use super::{RemoteError, Secret, endpoint, validate_base};
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper_util::rt::TokioIo;
use serde_json::Value;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_tungstenite::MaybeTlsStream;
use url::Url;

pub(crate) fn tls_config(root: Option<&[u8]>) -> Result<Arc<rustls::ClientConfig>, RemoteError> {
    let mut roots = rustls::RootCertStore::empty();
    roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    if let Some(root) = root {
        roots
            .add(rustls::pki_types::CertificateDer::from(root.to_vec()))
            .map_err(|_| RemoteError::Certificate)?;
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| RemoteError::Certificate)?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

pub(crate) async fn socket(
    url: &Url,
    root: Option<&[u8]>,
) -> Result<MaybeTlsStream<TcpStream>, RemoteError> {
    let host = match url.host() {
        Some(url::Host::Ipv6(ip)) => ip.to_string(),
        Some(host) => host.to_string(),
        None => return Err(RemoteError::InvalidUrl),
    };
    let port = url.port_or_known_default().ok_or(RemoteError::InvalidUrl)?;
    let tcp = TcpStream::connect((host.as_str(), port))
        .await
        .map_err(|_| RemoteError::Other(String::new()))?;
    if matches!(url.scheme(), "https" | "wss") {
        let server =
            rustls::pki_types::ServerName::try_from(host).map_err(|_| RemoteError::Certificate)?;
        let socket = tokio_rustls::TlsConnector::from(tls_config(root)?)
            .connect(server, tcp)
            .await
            .map_err(|_| RemoteError::Certificate)?;
        Ok(MaybeTlsStream::Rustls(socket))
    } else {
        Ok(MaybeTlsStream::Plain(tcp))
    }
}

pub(crate) async fn request(
    base: &Url,
    path: &str,
    method: &str,
    bearer: Option<&Secret>,
    body: Option<Value>,
    root: Option<&[u8]>,
) -> Result<(u16, Value), RemoteError> {
    validate_base(base)?;
    #[cfg(feature = "test-support")]
    let _network_clock = NetworkClock::hold().await;
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        let url = endpoint(base, path);
        let stream = socket(&url, root).await?;
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .map_err(|_| RemoteError::Other(String::new()))?;
        let mut builder = http::Request::builder()
            .method(method)
            .uri(&url[url::Position::BeforePath..url::Position::AfterQuery])
            .header(
                "Host",
                &url[url::Position::BeforeHost..url::Position::AfterPort],
            )
            .header("Connection", "close");
        if let Some(bearer) = bearer {
            let mut value = http::HeaderValue::from_str(&format!("Bearer {}", bearer.0))
                .map_err(|_| RemoteError::Other(String::new()))?;
            value.set_sensitive(true);
            builder = builder.header("Authorization", value);
        }
        let body = match body {
            Some(value) => {
                builder = builder.header("Content-Type", "application/json");
                value.to_string()
            }
            None => String::new(),
        };
        let request = builder
            .body(Full::new(Bytes::from(body)))
            .map_err(|_| RemoteError::Other(String::new()))?;
        let operation = async {
            let response = sender
                .send_request(request)
                .await
                .map_err(|_| RemoteError::Other("response headers".into()))?;
            let status = response.status().as_u16();
            let bytes = Limited::new(response.into_body(), 1024 * 1024)
                .collect()
                .await
                .map_err(|_| RemoteError::Other("response body".into()))?
                .to_bytes();
            Ok::<_, RemoteError>((status, bytes))
        };
        tokio::pin!(operation);
        tokio::pin!(connection);
        let (status, bytes) = tokio::select! {
            biased;
            result = &mut operation => result?,
            _ = &mut connection => operation.await?,
        };
        // Errors are classified by status. Untrusted error bodies are never printed.
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Ok((status, value))
    })
    .await
    .map_err(|_| RemoteError::Other("timeout".into()))?
}

// Real socket readiness must arrive before a paused test clock auto-advances the deadline.
// Explicit advance() still exercises the deadline, including unknown exchange outcomes.
#[cfg(feature = "test-support")]
struct NetworkClock(std::sync::mpsc::Sender<()>);
#[cfg(feature = "test-support")]
impl NetworkClock {
    async fn hold() -> Self {
        let (stop, receiver) = std::sync::mpsc::channel();
        let (started, ready) = tokio::sync::oneshot::channel();
        tokio::task::spawn_blocking(move || {
            let _ = started.send(());
            let _ = receiver.recv_timeout(std::time::Duration::from_secs(15));
        });
        let _ = ready.await;
        Self(stop)
    }
}
#[cfg(feature = "test-support")]
impl Drop for NetworkClock {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}
