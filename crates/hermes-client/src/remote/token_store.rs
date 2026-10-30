//! Owner-only token files.

use super::{RemoteError, RemoteName, TokenSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct TokenStore {
    folder: PathBuf,
    name: RemoteName,
}
impl TokenStore {
    pub fn new(folder: &Path, name: RemoteName) -> Self {
        Self {
            folder: folder.to_path_buf(),
            name,
        }
    }
    pub fn path(&self) -> PathBuf {
        self.folder
            .join("remote-tokens")
            .join(format!("{}.json", self.name.as_str()))
    }
    pub fn load(&self) -> Result<Option<TokenSet>, RemoteError> {
        todo!("add remote gateway implementation")
    }
    pub fn save(&self, _tokens: &TokenSet) -> Result<(), RemoteError> {
        todo!("add remote gateway implementation")
    }
    pub fn clear(&self) -> Result<(), RemoteError> {
        todo!("add remote gateway implementation")
    }
    #[cfg(feature = "test-support")]
    pub fn fail_before_rename(&self, _on: bool) {
        todo!("add remote gateway implementation")
    }
}
