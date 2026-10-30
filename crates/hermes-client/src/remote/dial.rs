//! Remote reconnect dial.
use super::{RemoteConfig, TokenStore};
use crate::liveness::{Dial, DialFailure, Dialed};

#[allow(dead_code)]
pub struct RemoteDial {
    cfg: RemoteConfig,
    store: TokenStore,
    profile: String,
}
impl RemoteDial {
    pub fn new(cfg: RemoteConfig, store: TokenStore, profile: String) -> Self {
        Self {
            cfg,
            store,
            profile,
        }
    }
    pub fn with_recorder(self, _recorder: crate::Recorder) -> Self {
        todo!("add remote gateway implementation")
    }
    #[cfg(feature = "test-support")]
    pub fn with_root_certificate(self, _root_der: &[u8]) -> Self {
        todo!("add remote gateway implementation")
    }
}
impl Dial for RemoteDial {
    async fn dial(&mut self, _generation: u64) -> Result<Dialed, DialFailure> {
        todo!("add remote gateway implementation")
    }
}
