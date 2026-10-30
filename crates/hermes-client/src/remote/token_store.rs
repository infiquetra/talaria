//! Owner-only token files, atomically replaced beside their destination.
use super::{RemoteError, RemoteName, Secret, TokenSet};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

#[derive(Clone)]
pub struct TokenStore {
    folder: PathBuf,
    name: RemoteName,
    pub(crate) refresh_lock: Arc<tokio::sync::Mutex<()>>,
    #[cfg(feature = "test-support")]
    fail_rename: Arc<std::sync::atomic::AtomicBool>,
}
impl TokenStore {
    pub fn new(folder: &Path, name: RemoteName) -> Self {
        type Locks = Mutex<HashMap<PathBuf, Weak<tokio::sync::Mutex<()>>>>;
        static LOCKS: OnceLock<Locks> = OnceLock::new();
        let folder = folder
            .canonicalize()
            .unwrap_or_else(|_| folder.to_path_buf());
        let key = folder.join("remote-tokens").join(name.as_str());
        let mut locks = LOCKS
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        locks.retain(|_, lock| lock.strong_count() > 0);
        let refresh_lock = locks.get(&key).and_then(Weak::upgrade).unwrap_or_else(|| {
            let lock = Arc::new(tokio::sync::Mutex::new(()));
            locks.insert(key, Arc::downgrade(&lock));
            lock
        });
        Self {
            folder,
            name,
            refresh_lock,
            #[cfg(feature = "test-support")]
            fail_rename: Arc::default(),
        }
    }
    pub fn path(&self) -> PathBuf {
        self.folder
            .join("remote-tokens")
            .join(format!("{}.json", self.name.as_str()))
    }
    pub fn load(&self) -> Result<Option<TokenSet>, RemoteError> {
        let mut file = match OpenOptions::new()
            .read(true)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(self.path())
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(self.io()),
        };
        let meta = file.metadata().map_err(|_| self.io())?;
        if !meta.is_file() || meta.uid() != rustix::process::geteuid().as_raw() {
            return Err(self.io());
        }
        if meta.mode() & 0o077 != 0 {
            return Err(RemoteError::TokenPermissions {
                name: self.name.as_str().to_owned(),
            });
        }
        if meta.len() > 1024 * 1024 {
            return Err(self.io());
        }
        let mut bytes = zeroize::Zeroizing::new(Vec::new());
        file.read_to_end(&mut bytes).map_err(|_| self.io())?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| self.io())?;
        if value["version"] != 1 {
            return Ok(None);
        }
        decode(&value).map(Some).ok_or_else(|| self.io())
    }
    pub fn save(&self, tokens: &TokenSet) -> Result<(), RemoteError> {
        let directory = self.folder.join("remote-tokens");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&directory)
            .map_err(|_| self.io())?;
        let meta = fs::symlink_metadata(&directory).map_err(|_| self.io())?;
        if !meta.is_dir() || meta.uid() != rustix::process::geteuid().as_raw() {
            return Err(self.io());
        }
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .map_err(|_| self.io())?;
        let mut temp = tempfile::NamedTempFile::new_in(&directory).map_err(|_| self.io())?;
        temp.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| self.io())?;
        let bytes = zeroize::Zeroizing::new(json!({"version":1,"access_token":tokens.access.0,"refresh_token":tokens.refresh.0,"expires_at":tokens.expires_at,"provider":tokens.provider,"user_id":tokens.user_id,"saved_at":tokens.saved_at}).to_string());
        temp.write_all(bytes.as_bytes()).map_err(|_| self.io())?;
        temp.as_file().sync_all().map_err(|_| self.io())?;
        #[cfg(feature = "test-support")]
        if self.fail_rename.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(self.io());
        }
        temp.persist(self.path()).map_err(|_| self.io())?;
        fs::File::open(directory)
            .and_then(|dir| dir.sync_all())
            .map_err(|_| self.io())?;
        Ok(())
    }
    pub fn clear(&self) -> Result<(), RemoteError> {
        match fs::remove_file(self.path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(self.io()),
        }
    }
    fn io(&self) -> RemoteError {
        RemoteError::Other(String::new())
    }
    #[cfg(feature = "test-support")]
    pub fn fail_before_rename(&self, on: bool) {
        self.fail_rename
            .store(on, std::sync::atomic::Ordering::SeqCst);
    }
}
pub(crate) fn decode(value: &Value) -> Option<TokenSet> {
    let text = |key: &str| {
        value[key]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    Some(TokenSet {
        access: Secret(text("access_token")?),
        refresh: Secret(text("refresh_token")?),
        expires_at: value["expires_at"].as_i64()?,
        provider: text("provider")?,
        user_id: text("user_id")?,
        saved_at: value["saved_at"].as_i64().unwrap_or_else(super::now),
    })
}

impl std::fmt::Debug for TokenStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TokenStore(<private>)")
    }
}
