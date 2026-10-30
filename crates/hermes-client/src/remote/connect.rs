//! Verified TLS and query-ticket WebSocket upgrades.
use super::{RemoteConfig, WsTicket};
use crate::{ConnectError, Connection};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

pub(crate) fn dial_url(cfg: &RemoteConfig, ticket: &WsTicket) -> url::Url {
    let mut url = super::endpoint(&cfg.base_url, "/api/ws");
    let scheme = if cfg.base_url.scheme() == "https" {
        "wss"
    } else {
        "ws"
    };
    let _ = url.set_scheme(scheme);
    url.query_pairs_mut().append_pair("ticket", &ticket.0.0);
    url
}
pub async fn connect_remote(
    cfg: &RemoteConfig,
    ticket: WsTicket,
) -> Result<Connection, ConnectError> {
    connect_inner(cfg, ticket, None).await
}
#[cfg(feature = "test-support")]
pub async fn connect_with_root(
    cfg: &RemoteConfig,
    ticket: WsTicket,
    root_der: &[u8],
) -> Result<Connection, ConnectError> {
    connect_inner(cfg, ticket, Some(root_der)).await
}
pub(crate) async fn connect_inner(
    cfg: &RemoteConfig,
    ticket: WsTicket,
    root: Option<&[u8]>,
) -> Result<Connection, ConnectError> {
    connect_observed(cfg, ticket, root)
        .await
        .map(|(connection, _)| connection)
}
pub(crate) async fn connect_observed(
    cfg: &RemoteConfig,
    ticket: WsTicket,
    root: Option<&[u8]>,
) -> Result<(Connection, Option<String>), ConnectError> {
    super::validate_base(&cfg.base_url)
        .map_err(|_| ConnectError::Io(std::io::ErrorKind::InvalidInput))?;
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        let url = dial_url(cfg, &ticket);
        let stream = super::http::socket(&url, root)
            .await
            .map_err(|_| ConnectError::Io(std::io::ErrorKind::ConnectionRefused))?;
        let mut request = url
            .as_str()
            .into_client_request()
            .map_err(|_| ConnectError::Io(std::io::ErrorKind::InvalidInput))?;
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            http::HeaderValue::from_static("hermes-gateway-v1"),
        );
        let key = request
            .headers()
            .get("Sec-WebSocket-Key")
            .ok_or(ConnectError::SubprotocolMismatch)?
            .as_bytes()
            .to_vec();
        let expected_accept = tokio_tungstenite::tungstenite::handshake::derive_accept_key(&key);
        let (mut sender, connection) =
            hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream))
                .await
                .map_err(|_| ConnectError::Io(std::io::ErrorKind::ConnectionAborted))?;
        // Recover the same TLS stream and any coalesced WebSocket bytes after the upgrade.
        let task = tokio::spawn(connection.without_shutdown());
        struct Driver(
            tokio::task::JoinHandle<
                Result<
                    hyper::client::conn::http1::Parts<
                        hyper_util::rt::TokioIo<
                            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                        >,
                    >,
                    hyper::Error,
                >,
            >,
        );
        impl Drop for Driver {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let mut driver = Driver(task);
        *request.uri_mut() = url[url::Position::BeforePath..url::Position::AfterQuery]
            .parse()
            .map_err(|_| ConnectError::Io(std::io::ErrorKind::InvalidInput))?;
        let request = request.map(|_| http_body_util::Empty::<bytes::Bytes>::new());
        let response = sender
            .send_request(request)
            .await
            .map_err(|_| ConnectError::Io(std::io::ErrorKind::ConnectionAborted))?;
        if response.status() != http::StatusCode::SWITCHING_PROTOCOLS {
            return Err(ConnectError::UpgradeRejected {
                status: Some(response.status().as_u16()),
            });
        }
        let headers = response.headers();
        let protocol_echo = headers
            .get("Sec-WebSocket-Protocol")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        if headers.get_all("Sec-WebSocket-Protocol").iter().count() > 1
            || headers
                .get("Sec-WebSocket-Protocol")
                .is_some_and(|v| v != "hermes-gateway-v1")
        {
            return Err(ConnectError::SubprotocolMismatch);
        }
        let has_token = |key: &str, token: &str| {
            headers
                .get(key)
                .and_then(|h| h.to_str().ok())
                .is_some_and(|h| {
                    h.split(',')
                        .any(|part| part.trim().eq_ignore_ascii_case(token))
                })
        };
        if !has_token("Upgrade", "websocket")
            || !has_token("Connection", "upgrade")
            || headers.get_all("Sec-WebSocket-Accept").iter().count() != 1
            || headers
                .get("Sec-WebSocket-Accept")
                .is_none_or(|h| h != expected_accept.as_str())
            || headers.contains_key("Sec-WebSocket-Extensions")
        {
            return Err(ConnectError::Io(std::io::ErrorKind::InvalidData));
        }
        let parts = (&mut driver.0)
            .await
            .map_err(|_| ConnectError::Io(std::io::ErrorKind::ConnectionAborted))?
            .map_err(|_| ConnectError::Io(std::io::ErrorKind::ConnectionAborted))?;
        let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(Some(8 * 1024 * 1024))
            .max_frame_size(Some(8 * 1024 * 1024));
        let socket = tokio_tungstenite::WebSocketStream::from_partially_read(
            parts.io.into_inner(),
            parts.read_buf.to_vec(),
            tokio_tungstenite::tungstenite::protocol::Role::Client,
            Some(config),
        )
        .await;
        Ok((
            Connection::from_websocket(socket, crate::connect::ExpectedIdentity::Unknown),
            protocol_echo,
        ))
    })
    .await
    .map_err(|_| ConnectError::Timeout)?
}
pub fn upgrade_error(base: &url::Url, status: u16) -> String {
    match status {
        403 => "the gateway refused the WebSocket upgrade (expired ticket, host not accepted by dashboard.public_url, or chat disabled)".into(),
        404 => format!("no /api/ws at {base}"),
        502..=504 => format!("the tunnel or gateway is unavailable (HTTP {status})"),
        _ => format!("the gateway refused the WebSocket upgrade (HTTP {status})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn malformed_upgrades_are_refused() {
        for fault in [
            "accept",
            "extension",
            "upgrade",
            "connection",
            "duplicate_protocol",
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let peer = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                while !bytes.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).await.unwrap();
                    bytes.push(byte[0]);
                }
                let request = String::from_utf8(bytes).unwrap();
                let key = request
                    .lines()
                    .find_map(|line| {
                        line.split_once(':')
                            .filter(|(key, _)| key.eq_ignore_ascii_case("Sec-WebSocket-Key"))
                            .map(|(_, value)| value.trim())
                    })
                    .unwrap();
                let accept = if fault == "accept" {
                    "invalid".into()
                } else {
                    tokio_tungstenite::tungstenite::handshake::derive_accept_key(key.as_bytes())
                };
                let upgrade = if fault == "upgrade" {
                    "other"
                } else {
                    "websocket"
                };
                let connection = if fault == "connection" {
                    "close"
                } else {
                    "Upgrade"
                };
                let extra = match fault {
                    "extension" => "Sec-WebSocket-Extensions: permessage-deflate\r\n",
                    "duplicate_protocol" => {
                        "Sec-WebSocket-Protocol: hermes-gateway-v1\r\nSec-WebSocket-Protocol: other\r\n"
                    }
                    _ => "",
                };
                let response = format!(
                    "HTTP/1.1 101 Switching Protocols\r\nUpgrade: {upgrade}\r\nConnection: {connection}\r\nSec-WebSocket-Accept: {accept}\r\n{extra}\r\n"
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            });
            let cfg = RemoteConfig {
                name: super::super::RemoteName::parse("work").unwrap(),
                base_url: url::Url::parse(&format!("http://{address}")).unwrap(),
                provider: None,
            };
            assert!(
                connect_remote(&cfg, WsTicket::for_test("test-ticket"))
                    .await
                    .is_err(),
                "accepted malformed {fault} upgrade"
            );
            peer.await.unwrap();
        }
    }
}
