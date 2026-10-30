//! Remote WebSocket upgrades.
use super::{RemoteConfig, WsTicket};
use crate::{ConnectError, Connection};
pub async fn connect_remote(
    _cfg: &RemoteConfig,
    _ticket: WsTicket,
) -> Result<Connection, ConnectError> {
    todo!("add remote gateway implementation")
}
#[cfg(feature = "test-support")]
pub async fn connect_with_root(
    _cfg: &RemoteConfig,
    _ticket: WsTicket,
    _root_der: &[u8],
) -> Result<Connection, ConnectError> {
    todo!("add remote gateway implementation")
}
pub fn upgrade_error(_base: &url::Url, _status: u16) -> String {
    todo!("add remote gateway implementation")
}
