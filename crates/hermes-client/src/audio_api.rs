//! Profile-scoped audio relay.
use url::Url;

pub async fn transcribe(_base: &Url, _profile: &str, _audio: &[u8]) -> Result<String, String> {
    Err("not implemented".into())
}

pub async fn speak(_base: &Url, _profile: &str, _text: &str) -> Result<Vec<u8>, String> {
    Err("not implemented".into())
}
