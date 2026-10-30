//! Native sign-in proof keys.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

#[derive(Clone, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct PkcePair {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}
impl std::fmt::Debug for PkcePair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PkcePair(<redacted>)")
    }
}
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}
fn random() -> Result<String, super::SignInError> {
    let mut bytes = zeroize::Zeroizing::new([0u8; 32]);
    getrandom::fill(&mut *bytes).map_err(|_| super::SignInError::Io(String::new()))?;
    Ok(URL_SAFE_NO_PAD.encode(*bytes))
}
/// Generate independent proof and state values, failing closed if entropy is unavailable.
pub fn try_generate() -> Result<PkcePair, super::SignInError> {
    let verifier = random()?;
    Ok(PkcePair {
        challenge: challenge(&verifier),
        verifier,
        state: random()?,
    })
}
/// Infallible convenience for fixtures running with the test-support feature.
#[cfg(feature = "test-support")]
pub fn generate() -> PkcePair {
    try_generate().expect("test operating system random source")
}
pub fn authorize_url(
    base: &url::Url,
    challenge: &str,
    redirect: &url::Url,
    state: &str,
    provider: Option<&str>,
) -> url::Url {
    let mut url = super::endpoint(base, "/auth/native/authorize");
    url.query_pairs_mut()
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("redirect_uri", redirect.as_str())
        .append_pair("state", state);
    if let Some(provider) = provider {
        url.query_pairs_mut().append_pair("provider", provider);
    }
    url
}
