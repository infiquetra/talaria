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
    /// Take the next turn that settles on the stopped agent's Hermes session as the call's
    /// result. Only a call parked because it was stopped accepts it.
    UseReply,
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
    /// Which source the Talaria build came from.
    #[serde(flatten)]
    pub build: BuildStamp,
    pub hermes: Value,
    pub roster: Value,
    pub approval_policy: Value,
}
/// The commit a Talaria build was made from and whether its tree had uncommitted changes to
/// tracked files. Both read `unknown` for a build made without git and in manifests written
/// before they were recorded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildStamp {
    /// Short commit hash, or `unknown`.
    pub talaria_commit: String,
    /// `clean`, `modified` or `unknown`.
    pub talaria_tree: String,
}
impl Default for BuildStamp {
    fn default() -> Self {
        Self {
            talaria_commit: "unknown".into(),
            talaria_tree: "unknown".into(),
        }
    }
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
    #[serde(flatten)]
    pub build: BuildStamp,
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
            build: new.build,
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
            || !super::valid_sha256(&manifest.workflow.script_sha256)
        {
            return Err(JournalError::Manifest(
                "manifest version, run id or script hash is invalid".into(),
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
    /// Leave a note that the workflow view is about to stop `key`'s running turn, so the runner
    /// can say who stopped it when the turn settles. `restart` marks a stop sent to restart the
    /// agent. The note names the turn running now, read from the journal; a newer note replaces
    /// an older one.
    pub fn note_agent_stop(
        &self,
        key: &str,
        restart: bool,
        at_ms: i64,
    ) -> Result<(), JournalError> {
        let source = if restart {
            super::StopSource::WorkflowViewRestart
        } else {
            super::StopSource::WorkflowView
        };
        // A journal that cannot be read names no turn; the note then matches nothing.
        let records = super::JournalReader::read_from(&self.path.join("journal.jsonl"), 0)
            .map(|(records, _)| records)
            .unwrap_or_default();
        let turn = running_turn(&records, key);
        self.note_stop(key, source, turn.as_ref(), at_ms)
    }
    /// Leave a durable note that `source` is about to stop `turn` of `key`, before the stop is
    /// sent. A settlement read later, even by a later execution, is then attributed to it. A
    /// note that names no turn matches no settlement.
    pub fn note_stop(
        &self,
        key: &str,
        source: super::StopSource,
        turn: Option<&StopTurn>,
        at_ms: i64,
    ) -> Result<(), JournalError> {
        if !valid_key(key) {
            return Err(JournalError::Manifest("invalid agent key".into()));
        }
        let folder = self.path.join("control");
        fs::create_dir_all(&folder)?;
        let note = AgentStopNote {
            v: 1,
            kind: note_kind(source).into(),
            key: key.into(),
            at_ms,
            attempt: turn.map(|t| t.attempt),
            turn: turn.map(|t| t.turn),
            admission_id: turn.and_then(|t| t.admission_id.clone()),
        };
        let bytes = serde_json::to_vec(&note).map_err(super::io_error)?;
        let tmp = folder.join(format!(".tmp-agent-stop-{key}.{}", random_hex()?));
        let mut file = private_file(&tmp, true)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, folder.join(agent_stop_name(key)))?;
        sync_dir(&folder).map_err(Into::into)
    }
    /// Write the note unless one already names exactly this turn, and say whether it was
    /// written. A note that already names the turn records an earlier stop that did go out (a
    /// budget stop, the view's stop, or a run stop whose settlement was never read). Writing
    /// over it, and then clearing it when a second stop finds the turn already ended, would
    /// leave that earlier stop without a source, and the turn would park on resume.
    pub fn note_stop_if_new(
        &self,
        key: &str,
        source: super::StopSource,
        turn: &StopTurn,
        at_ms: i64,
    ) -> Result<bool, JournalError> {
        if let Some(admission_id) = &turn.admission_id
            && self
                .agent_stop_for(key, turn.attempt, turn.turn, admission_id)
                .is_some()
        {
            return Ok(false);
        }
        self.note_stop(key, source, Some(turn), at_ms)?;
        Ok(true)
    }
    fn read_stop_note(&self, key: &str) -> Option<(AgentStopNote, super::StopSource)> {
        let bytes = fs::read(self.path.join("control").join(agent_stop_name(key))).ok()?;
        let note: AgentStopNote = serde_json::from_slice(&bytes).ok()?;
        if note.v != 1 || note.key != key {
            return None;
        }
        let source = note_source(&note.kind)?;
        Some((note, source))
    }
    /// Who the readable stop note for `key` says sent a stop, whatever turn it names.
    pub fn agent_stop(&self, key: &str) -> Option<super::StopSource> {
        self.read_stop_note(key).map(|(_, source)| source)
    }
    /// Who stopped the turn now settling, when `key`'s note names exactly that turn: the same
    /// attempt and turn, and the same admission when the note knows it. A note about another
    /// turn, or one that names none, says nothing about this settlement.
    pub fn agent_stop_for(
        &self,
        key: &str,
        attempt: u32,
        turn: u32,
        admission_id: &str,
    ) -> Option<super::StopSource> {
        let (note, source) = self.read_stop_note(key)?;
        note.names(attempt, turn, Some(admission_id))
            .then_some(source)
    }
    /// Remove `key`'s stop note once the turn it named has settled.
    pub fn clear_agent_stop(&self, key: &str) -> std::io::Result<()> {
        match fs::remove_file(self.path.join("control").join(agent_stop_name(key))) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
    /// Remove every stop note that no longer names a turn still running, read from the
    /// journal: its turn settled, a later attempt replaced it, or it names no turn. A note for
    /// a turn still waiting to settle is kept, so its settlement keeps its source.
    pub fn prune_agent_stops(&self) -> Result<(), JournalError> {
        let folder = self.path.join("control");
        let entries = match fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let records = super::JournalReader::read_from(&self.path.join("journal.jsonl"), 0)?.0;
        for entry in entries {
            let name = entry?.file_name().to_string_lossy().into_owned();
            let Some(key) = name
                .strip_prefix("agent-stop-")
                .and_then(|rest| rest.strip_suffix(".json"))
            else {
                continue;
            };
            let live = valid_key(key)
                && self.read_stop_note(key).is_some_and(|(note, _)| {
                    running_turn(&records, key).is_some_and(|running| {
                        note.names(
                            running.attempt,
                            running.turn,
                            running.admission_id.as_deref(),
                        )
                    })
                });
            if !live {
                fs::remove_file(folder.join(&name))?;
            }
        }
        sync_dir(&folder).map_err(Into::into)
    }
}
/// The turn a stop note names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopTurn {
    pub attempt: u32,
    pub turn: u32,
    /// The turn's admission, when it was admitted before the note was written.
    pub admission_id: Option<String>,
}
/// The turn of `key` that has not settled yet, from the journal: its latest record is the
/// turn's intent or its admission.
pub(crate) fn running_turn(records: &[super::RecordLine], key: &str) -> Option<StopTurn> {
    use super::Record;
    let latest = records
        .iter()
        .rev()
        .find(|line| line.record.identity().is_some_and(|(k, _)| k == key))?;
    match &latest.record {
        Record::TurnIntent { attempt, turn, .. } => Some(StopTurn {
            attempt: *attempt,
            turn: *turn,
            admission_id: None,
        }),
        Record::TurnAdmitted {
            attempt,
            turn,
            admission_id,
            ..
        } => Some(StopTurn {
            attempt: *attempt,
            turn: *turn,
            admission_id: Some(admission_id.clone()),
        }),
        _ => None,
    }
}
fn note_kind(source: super::StopSource) -> &'static str {
    match source {
        super::StopSource::WorkflowView => "agent_stop",
        super::StopSource::WorkflowViewRestart => "agent_restart",
        super::StopSource::RunStop => "run_stop",
        super::StopSource::Budget => "budget_stop",
    }
}
fn note_source(kind: &str) -> Option<super::StopSource> {
    match kind {
        "agent_stop" => Some(super::StopSource::WorkflowView),
        "agent_restart" => Some(super::StopSource::WorkflowViewRestart),
        "run_stop" => Some(super::StopSource::RunStop),
        "budget_stop" => Some(super::StopSource::Budget),
        _ => None,
    }
}
/// The kinds a stop note can have, which the control-file reader accepts.
pub const STOP_NOTE_KINDS: [&str; 4] = ["agent_stop", "agent_restart", "run_stop", "budget_stop"];
/// The control file that holds the note about a stop sent to `key`'s running turn.
pub fn agent_stop_name(key: &str) -> String {
    format!("agent-stop-{key}.json")
}
#[derive(Serialize, Deserialize)]
struct AgentStopNote {
    v: u32,
    kind: String,
    key: String,
    at_ms: i64,
    /// The turn the stop was sent to. A note written before notes named their turn has none,
    /// and matches nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attempt: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    turn: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    admission_id: Option<String>,
}
impl AgentStopNote {
    /// Whether the note names this turn. An admission is compared only when both sides know it.
    fn names(&self, attempt: u32, turn: u32, admission_id: Option<&str>) -> bool {
        self.attempt == Some(attempt)
            && self.turn == Some(turn)
            && match (self.admission_id.as_deref(), admission_id) {
                (Some(noted), Some(settling)) => noted == settling,
                _ => true,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn old_manifest() -> Value {
        serde_json::json!({"v":1,"run_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","created_at_ms":1,
            "workflow":{"name":"fixture","script_sha256":"b".repeat(64)},"args":null,
            "started_at_ms":1,"seed":7,"profile":"fixture","on_unknown":"wait",
            "talaria_version":"0.9.0","hermes":{},"roster":{},
            "approval_policy":{"mode":"finite-only","source":"an older free-text note"}})
    }

    #[test]
    fn a_manifest_written_before_the_build_was_recorded_still_reads() {
        let manifest: Manifest = serde_json::from_value(old_manifest()).unwrap();
        assert_eq!(manifest.build, BuildStamp::default());
        assert_eq!(manifest.build.talaria_commit, "unknown");
        assert_eq!(manifest.approval_policy["mode"], "finite-only");
    }

    #[test]
    fn opening_a_run_rejects_an_invalid_script_hash() {
        let parent = tempfile::tempdir().unwrap();
        let folder = parent.path().join("01ARZ3NDEKTSV4RRFFQ69G5FAV");
        fs::create_dir(&folder).unwrap();
        let mut manifest = old_manifest();
        manifest["workflow"]["script_sha256"] = serde_json::json!("../../outside");
        fs::write(
            folder.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            RunDir::open(&folder),
            Err(JournalError::Manifest(_))
        ));
    }

    #[test]
    fn an_earlier_stop_note_for_the_same_turn_is_kept() {
        let parent = tempfile::tempdir().unwrap();
        let folder = parent.path().join("01ARZ3NDEKTSV4RRFFQ69G5FAV");
        fs::create_dir_all(&folder).unwrap();
        fs::write(
            folder.join("manifest.json"),
            serde_json::to_vec(&old_manifest()).unwrap(),
        )
        .unwrap();
        let dir = RunDir::open(&folder).unwrap();
        let key = "0123456789abcdef0123456789abcdef";
        let turn = |turn: u32, admission: &str| StopTurn {
            attempt: 0,
            turn,
            admission_id: Some(admission.into()),
        };
        // The budget stop goes out first and leaves its note.
        assert!(
            dir.note_stop_if_new(key, crate::journal::StopSource::Budget, &turn(0, "a0"), 1)
                .unwrap()
        );
        // A run stop of the same turn must not take the note over: it would then clear it when
        // it finds the turn already ended, and the budget's stop would park on resume.
        assert!(
            !dir.note_stop_if_new(key, crate::journal::StopSource::RunStop, &turn(0, "a0"), 2)
                .unwrap()
        );
        assert_eq!(
            dir.agent_stop_for(key, 0, 0, "a0"),
            Some(crate::journal::StopSource::Budget)
        );
        // A note about another turn is stale and is replaced.
        assert!(
            dir.note_stop_if_new(key, crate::journal::StopSource::RunStop, &turn(1, "a1"), 3)
                .unwrap()
        );
        assert_eq!(
            dir.agent_stop_for(key, 0, 1, "a1"),
            Some(crate::journal::StopSource::RunStop)
        );
    }

    #[test]
    fn the_build_is_written_beside_the_version() {
        let mut value = old_manifest();
        value["talaria_commit"] = "0123456789ab".into();
        value["talaria_tree"] = "modified".into();
        let manifest: Manifest = serde_json::from_value(value).unwrap();
        let written = serde_json::to_value(&manifest).unwrap();
        assert_eq!(written["talaria_version"], "0.9.0");
        assert_eq!(written["talaria_commit"], "0123456789ab");
        assert_eq!(written["talaria_tree"], "modified");
        assert!(written.get("build").is_none(), "{written}");
    }
}
