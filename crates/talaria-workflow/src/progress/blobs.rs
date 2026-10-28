//! Read the selected prompt without trusting a journal blob reference as a path.
use crate::journal::RunDir;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlobRef {
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    #[error("prompt unavailable — journal blob missing")]
    Unavailable,
}
pub fn load_text(dir: &RunDir, reference: &BlobRef) -> Result<String, BlobError> {
    if reference.sha256.len() != 64
        || !reference
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(BlobError::Unavailable);
    }
    let bytes = std::fs::read(dir.path().join("blobs").join(&reference.sha256))
        .map_err(|_| BlobError::Unavailable)?;
    let hash: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if bytes.len() as u64 != reference.bytes || hash != reference.sha256 {
        return Err(BlobError::Unavailable);
    }
    String::from_utf8(bytes).map_err(|_| BlobError::Unavailable)
}
