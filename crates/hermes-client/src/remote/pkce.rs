//! Native sign-in proof keys.

#[derive(Debug, Clone)]
pub struct PkcePair {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}
pub fn challenge(_verifier: &str) -> String {
    todo!("add remote gateway implementation")
}
pub fn generate() -> PkcePair {
    todo!("add remote gateway implementation")
}
pub fn authorize_url(
    _base: &url::Url,
    _challenge: &str,
    _redirect: &url::Url,
    _state: &str,
    _provider: Option<&str>,
) -> url::Url {
    todo!("add remote gateway implementation")
}
