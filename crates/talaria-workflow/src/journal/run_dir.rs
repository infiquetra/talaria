use super::{JournalError, keys::sha, now_ms};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RunId(String);
impl RunId {
    pub fn new() -> Self {
        let mut random = [0u8; 10];
        // A failed operating-system random source is unrecoverable for the infallible constructor.
        getrandom::fill(&mut random).expect("operating-system random source unavailable");
        let mut value = (now_ms() as u128 & ((1 << 48) - 1)) << 80;
        for byte in random {
            value = (value & !((1u128 << 80) - 1))
                | (((value & ((1u128 << 80) - 1)) << 8) | u128::from(byte));
        }
        let mut out = [b'0'; 26];
        for byte in out.iter_mut().rev() {
            *byte = ALPHABET[(value & 31) as usize];
            value >>= 5;
        }
        Self(String::from_utf8_lossy(&out).into_owned())
    }
    pub fn parse(s: &str) -> Result<Self, JournalError> {
        if s.len() != 26 || !s.bytes().all(|b| ALPHABET.contains(&b)) || s.as_bytes()[0] > b'7' {
            return Err(JournalError::BadRunId(s.into()));
        }
        Ok(Self(s.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}
impl TryFrom<String> for RunId {
    type Error = JournalError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::parse(&s)
    }
}
impl From<RunId> for String {
    fn from(id: RunId) -> Self {
        id.0
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnUnknown {
    #[default]
    Wait,
    Fail,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    Fail,
    Rerun,
    Abort,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NewManifest {
    pub workflow_name: String,
    pub args: Option<Value>,
    pub started_at_ms: i64,
    pub seed: u64,
    pub profile: String,
    pub on_unknown: OnUnknown,
    pub talaria_version: String,
    pub hermes: Value,
    pub roster: Value,
    pub approval_policy: Value,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkflowManifest {
    pub name: String,
    pub script_sha256: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub v: u32,
    pub run_id: RunId,
    pub created_at_ms: i64,
    pub workflow: WorkflowManifest,
    pub args: Option<Value>,
    pub started_at_ms: i64,
    pub seed: u64,
    pub profile: String,
    pub on_unknown: OnUnknown,
    pub talaria_version: String,
    pub hermes: Value,
    pub roster: Value,
    pub approval_policy: Value,
}
#[derive(Clone, Debug)]
pub struct RunDir {
    path: PathBuf,
    manifest: Manifest,
}
impl RunDir {
    pub fn create(
        parent: &Path,
        manifest: NewManifest,
        source: &str,
    ) -> Result<Self, JournalError> {
        Self::create_with_id(parent, RunId::new(), manifest, source)
    }
    #[doc(hidden)]
    pub fn create_with_id(
        parent: &Path,
        id: RunId,
        new: NewManifest,
        source: &str,
    ) -> Result<Self, JournalError> {
        for (field, value) in [
            ("args", new.args.as_ref()),
            ("hermes", Some(&new.hermes)),
            ("roster", Some(&new.roster)),
            ("approval_policy", Some(&new.approval_policy)),
        ] {
            if let Some(v) = value {
                check_credentials(v, &format!("/{field}"))?;
            }
        }
        let manifest = Manifest {
            v: 1,
            run_id: id.clone(),
            created_at_ms: now_ms(),
            workflow: WorkflowManifest {
                name: new.workflow_name,
                script_sha256: sha(source.as_bytes()),
            },
            args: new.args,
            started_at_ms: new.started_at_ms,
            seed: new.seed,
            profile: new.profile,
            on_unknown: new.on_unknown,
            talaria_version: new.talaria_version,
            hermes: new.hermes,
            roster: new.roster,
            approval_policy: new.approval_policy,
        };
        let path = parent.join(id.as_str());
        match private_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(JournalError::Exists { run_id: id.0 });
            }
            Err(e) => return Err(e.into()),
        }
        let dir = Self { path, manifest };
        let result = (|| {
            for sub in ["scripts", "blobs", "resolutions"] {
                private_dir(&dir.path.join(sub))?;
            }
            for file in ["journal.jsonl", "run.lock"] {
                private_file(&dir.path.join(file), true)?.sync_all()?;
            }
            dir.store_script(source)?;
            let bytes = serde_json::to_vec(&dir.manifest).map_err(super::io_error)?;
            atomic_replace(&dir.path.join("manifest.json"), &bytes)?;
            sync_dir(parent)?;
            Ok::<_, JournalError>(())
        })();
        if let Err(e) = result {
            let _ = fs::remove_dir_all(&dir.path);
            return Err(e);
        }
        Ok(dir)
    }
    pub fn open(path: &Path) -> Result<Self, JournalError> {
        let manifest: Manifest = serde_json::from_slice(&fs::read(path.join("manifest.json"))?)
            .map_err(|e| JournalError::Manifest(e.to_string()))?;
        if manifest.v != 1
            || path.file_name().and_then(|s| s.to_str()) != Some(manifest.run_id.as_str())
        {
            return Err(JournalError::Manifest(
                "manifest version or run id differs from folder".into(),
            ));
        }
        Ok(Self {
            path: path.into(),
            manifest,
        })
    }
    pub fn run_id(&self) -> &RunId {
        &self.manifest.run_id
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn store_script(&self, source: &str) -> Result<String, JournalError> {
        let hash = sha(source.as_bytes());
        immutable(
            &self.path.join("scripts"),
            &format!("{hash}.js"),
            source.as_bytes(),
        )?;
        Ok(hash)
    }
    pub fn request_resolution(
        &self,
        key: &str,
        attempt: u32,
        episode: u32,
        resolution: Resolution,
    ) -> Result<(), JournalError> {
        if !valid_key(key) {
            return Err(JournalError::Manifest("invalid resolution key".into()));
        }
        let name = format!("{key}.{attempt}.{episode}.json");
        let answer = ResolutionFile {
            v: 1,
            key: key.into(),
            attempt,
            episode,
            resolution,
        };
        let bytes = serde_json::to_vec(&answer).map_err(super::io_error)?;
        let folder = self.path.join("resolutions");
        let tmp = folder.join(format!(".tmp-{key}.{attempt}.{episode}.{}", random_hex()?));
        let mut file = private_file(&tmp, true)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        let result = fs::hard_link(&tmp, folder.join(&name));
        fs::remove_file(tmp)?;
        match result {
            Ok(()) => sync_dir(&folder).map_err(Into::into),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(JournalError::ResolutionPending { file: name })
            }
            Err(e) => Err(e.into()),
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResolutionFile {
    pub v: u32,
    pub key: String,
    pub attempt: u32,
    pub episode: u32,
    pub resolution: Resolution,
}
pub(crate) fn valid_key(key: &str) -> bool {
    key.len() == 32
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn check_credentials(value: &Value, path: &str) -> Result<(), JournalError> {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let pointer = format!("{path}/{}", k.replace('~', "~0").replace('/', "~1"));
                let normalized = k.to_lowercase().replace(['-', '_'], "");
                if [
                    "token",
                    "accesstoken",
                    "refreshtoken",
                    "ticket",
                    "secret",
                    "clientsecret",
                    "password",
                    "apikey",
                    "authorization",
                    "cookie",
                ]
                .contains(&normalized.as_str())
                {
                    return Err(JournalError::Manifest(format!(
                        "manifest field {pointer} looks like a credential"
                    )));
                }
                check_credentials(v, &pointer)?;
            }
        }
        Value::Array(a) => {
            for (i, v) in a.iter().enumerate() {
                check_credentials(v, &format!("{path}/{i}"))?;
            }
        }
        Value::String(s) if s.starts_with("hermes-gateway-ticket") => {
            return Err(JournalError::Manifest(format!(
                "manifest field {path} looks like a credential"
            )));
        }
        _ => {}
    }
    Ok(())
}
pub(crate) fn private_dir(path: &Path) -> std::io::Result<()> {
    let mut b = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(path)
}
pub(crate) fn private_file(path: &Path, create_new: bool) -> std::io::Result<File> {
    let mut o = OpenOptions::new();
    o.read(true).write(true);
    if create_new {
        o.create_new(true);
    } else {
        o.create(true).truncate(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path)
}
pub(crate) fn sync_dir(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}
pub(crate) fn random_hex() -> std::io::Result<String> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(hex::encode(bytes))
}
pub(crate) fn immutable(folder: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = folder.join(format!(".tmp-{}", random_hex()?));
    let mut file = private_file(&tmp, true)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    let result = fs::hard_link(&tmp, folder.join(name));
    fs::remove_file(tmp)?;
    match result {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if fs::read(folder.join(name))? != bytes {
                return Err(std::io::Error::other("immutable content differs"));
            }
        }
        Err(e) => return Err(e),
    }
    sync_dir(folder)
}
pub(crate) fn atomic_replace(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    let mut file = private_file(&tmp, true)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(tmp, path)?;
    if let Some(folder) = path.parent() {
        sync_dir(folder)?;
    }
    Ok(())
}
