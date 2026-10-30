//! Callback listener for native sign-in.

use super::SignInError;
use std::time::Duration;
use tokio::net::TcpListener;
use url::Url;

pub struct CallbackListener {
    pub listener: TcpListener,
}
impl CallbackListener {
    pub async fn bind() -> std::io::Result<Self> {
        todo!("add remote gateway implementation")
    }
    pub fn redirect_uri(&self) -> Url {
        todo!("add remote gateway implementation")
    }
    pub async fn wait(
        self,
        _expected_state: &str,
        _timeout: Duration,
    ) -> Result<String, SignInError> {
        todo!("add remote gateway implementation")
    }
}
