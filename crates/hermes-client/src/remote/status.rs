//! Gated-listener status probing.

use super::{GateStatus, RemoteError};
use url::Url;

pub async fn probe_status(_base: &Url) -> Result<GateStatus, RemoteError> {
    todo!("add remote gateway implementation")
}
#[cfg(feature = "test-support")]
pub trait RequestSpy: Send + Sync {
    fn request(&self, url: &Url);
}
#[cfg(feature = "test-support")]
pub async fn probe_with_spy(_base: &Url, _spy: &dyn RequestSpy) -> Result<GateStatus, RemoteError> {
    todo!("add remote gateway implementation")
}
