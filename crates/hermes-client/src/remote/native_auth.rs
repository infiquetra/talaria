//! Native sign-in and token refresh.

use super::{RefreshError, RemoteConfig, SignInError, SignInOptions, TokenSet, TokenStore};
#[cfg(feature = "test-support")]
use std::io::{Read, Write};
#[cfg(feature = "test-support")]
use url::Url;

pub async fn sign_in(_cfg: &RemoteConfig, _opts: SignInOptions) -> Result<TokenSet, SignInError> {
    todo!("add remote gateway implementation")
}
pub async fn exchange(
    _cfg: &RemoteConfig,
    _code: &str,
    _verifier: &str,
) -> Result<TokenSet, SignInError> {
    todo!("add remote gateway implementation")
}
pub async fn refresh(_cfg: &RemoteConfig, _store: &TokenStore) -> Result<TokenSet, RefreshError> {
    todo!("add remote gateway implementation")
}
#[cfg(feature = "test-support")]
pub async fn refresh_held(
    _cfg: &RemoteConfig,
    _store: &TokenStore,
    _cached: &TokenSet,
) -> Result<TokenSet, RefreshError> {
    todo!("add remote gateway implementation")
}
#[cfg(feature = "test-support")]
pub fn read_paste<R: Read + std::os::fd::AsFd, W: Write>(
    _input: &mut R,
    _prompt: &mut W,
    _state: &str,
) -> Result<Url, SignInError> {
    todo!("add remote gateway implementation")
}

#[cfg(feature = "test-support")]
pub struct SignInIo {
    pub stdout: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    pub stderr: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    pub browser: std::sync::Arc<dyn Fn(&Url) + Send + Sync>,
    pub authorize: tokio::sync::mpsc::UnboundedSender<Url>,
}
#[cfg(feature = "test-support")]
pub async fn sign_in_with_io(
    _cfg: &RemoteConfig,
    _opts: SignInOptions,
    _io: SignInIo,
) -> Result<TokenSet, SignInError> {
    todo!("add remote gateway implementation")
}
