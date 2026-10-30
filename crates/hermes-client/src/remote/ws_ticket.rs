//! One-use remote WebSocket tickets.
use super::{RemoteConfig, RemoteError, TokenStore, WsTicket};
pub async fn mint_ws_ticket(
    _cfg: &RemoteConfig,
    _store: &TokenStore,
) -> Result<WsTicket, RemoteError> {
    todo!("add remote gateway implementation")
}
