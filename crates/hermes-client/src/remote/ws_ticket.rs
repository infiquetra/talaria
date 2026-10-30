//! Single-use tickets: at most one refresh and one retry after refusal.
use super::{RemoteConfig, RemoteError, Secret, TokenStore, WsTicket};
use super::{http, native_auth};
pub async fn mint_ws_ticket(
    cfg: &RemoteConfig,
    store: &TokenStore,
) -> Result<WsTicket, RemoteError> {
    mint_with_root(cfg, store, None).await
}
pub(crate) async fn mint_with_root(
    cfg: &RemoteConfig,
    store: &TokenStore,
    root: Option<&[u8]>,
) -> Result<WsTicket, RemoteError> {
    let mut tokens = native_auth::fresh(cfg, store, root).await?;
    for attempt in 0..2 {
        let (status, body) = http::request(
            &cfg.base_url,
            "/api/auth/ws-ticket",
            "POST",
            Some(&tokens.access),
            None,
            root,
        )
        .await?;
        if status == 200 {
            return body["ticket"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(|s| WsTicket(Secret(s.to_owned())))
                .ok_or_else(|| RemoteError::Other(String::new()));
        }
        if status != 401 {
            return Err(RemoteError::Http(status));
        }
        if attempt == 0 {
            tokens = native_auth::refresh_cached(cfg, store, &tokens, root)
                .await
                .map_err(RemoteError::Refresh)?;
        }
    }
    Err(RemoteError::RefusedAfterRefresh {
        name: cfg.name.as_str().to_owned(),
    })
}
