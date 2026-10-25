//! Submitted prompt history: file store with in-memory views.
//!
//! Behavior contract: issue 228.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use talaria_core::prompt_history::HistoryEntry;

/// Select the local repository directory without starting Git before the first frame.
pub fn history_workspace(folder: &Path) -> PathBuf {
    let canonical = crate::launch::workspace_key(folder, None);
    let repository = canonical
        .ancestors()
        .find(|path| path.join(".git").exists());
    crate::launch::workspace_key(&canonical, repository)
}

/// The history store for one run.
#[derive(Debug, Clone)]
pub struct PromptHistory {
    file_lock: Arc<std::sync::Mutex<()>>,
    dir: PathBuf,
    path: PathBuf,
    save: bool,
    workspace: String,
    entries: Vec<HistoryEntry>,
}

impl PromptHistory {
    /// Open the store, loading the newest entries for this workspace.
    pub fn open(dir: &Path, save: bool, workspace_key: &str) -> (Self, Option<String>) {
        Self::open_file(&dir.join("history.jsonl"), save, workspace_key)
    }

    /// Open an explicitly selected history file, for an isolated terminal surface.
    pub fn open_file(path: &Path, save: bool, workspace_key: &str) -> (Self, Option<String>) {
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut store = Self {
            file_lock: Arc::new(std::sync::Mutex::new(())),
            dir: dir.to_owned(),
            path: path.to_owned(),
            save,
            workspace: workspace_key.to_owned(),
            entries: Vec::new(),
        };
        if save && let Ok(mut file) = File::open(path) {
            let loaded = (|| -> std::io::Result<Vec<u8>> {
                let length = file.metadata()?.len();
                file.seek(SeekFrom::Start(length.saturating_sub(1024 * 1024)))?;
                let mut bytes = Vec::new();
                file.take(1024 * 1024).read_to_end(&mut bytes)?;
                Ok(bytes)
            })();
            if let Ok(bytes) = loaded {
                store.entries = bytes
                    .split(|byte| *byte == b'\n')
                    .filter_map(|line| decode(line, workspace_key))
                    .collect();
            }
        }
        (store, None)
    }

    /// Every entry known to this run, oldest first.
    pub fn snapshot(&self) -> Arc<[HistoryEntry]> {
        self.entries.clone().into()
    }

    /// This workspace's newest entries for seeding the composer, oldest first.
    pub fn composer_seed(&self) -> Vec<String> {
        let mut texts: Vec<_> = self
            .entries
            .iter()
            .rev()
            .filter(|entry| entry.this_folder)
            .take(1000)
            .map(|entry| entry.text.clone())
            .collect();
        texts.reverse();
        texts
    }

    /// Record one submit, on disk and in memory.
    pub fn append(&mut self, text: &str) -> Option<String> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| {
                i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
            });
        self.entries.push(HistoryEntry {
            text: text.to_owned(),
            ts,
            this_folder: true,
        });
        if !self.save || text.starts_with(' ') || text.chars().count() > 100_000 {
            return None;
        }
        let result = (|| -> std::io::Result<()> {
            let _lock = self
                .file_lock
                .lock()
                .map_err(|_| std::io::Error::other("history lock failed"))?;
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&self.dir)?;
            let mode = std::fs::metadata(&self.dir)?.permissions().mode() & 0o700;
            std::fs::set_permissions(&self.dir, std::fs::Permissions::from_mode(mode))?;
            let mut file = OpenOptions::new()
                .append(true)
                .create(true)
                .mode(0o600)
                .open(&self.path)?;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            let mut line =
                serde_json::json!({"v": 1, "ts": ts, "folder": self.workspace, "text": text})
                    .to_string();
            line.push('\n');
            if file.write(line.as_bytes())? != line.len() {
                return Err(std::io::ErrorKind::WriteZero.into());
            }
            Ok(())
        })();
        result.err().map(|error| {
            self.save = false;
            format!(
                "Prompt history could not be saved ({}); history works for this run only.",
                super::editor::io_reason(&error)
            )
        })
    }

    /// Shrink an oversized file to its newest lines. True when it compacted.
    pub fn compact(&self) -> std::io::Result<bool> {
        let _lock = self
            .file_lock
            .lock()
            .map_err(|_| std::io::Error::other("history lock failed"))?;
        if !self.save {
            return Ok(false);
        }
        let path = &self.path;
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        if file.metadata()?.len() <= 4 * 1024 * 1024 {
            return Ok(false);
        }
        let mut lines = VecDeque::new();
        let mut bytes = 0;
        for line in BufReader::new(file).split(b'\n') {
            let mut line = line?;
            if decode(&line, &self.workspace).is_none() {
                continue;
            }
            line.push(b'\n');
            bytes += line.len();
            lines.push_back(line);
            while lines.len() > 5000 || bytes > 1024 * 1024 {
                if let Some(line) = lines.pop_front() {
                    bytes -= line.len();
                } else {
                    break;
                }
            }
        }
        let mut temporary = tempfile::NamedTempFile::new_in(&self.dir)?;
        for line in lines {
            temporary.write_all(&line)?;
        }
        temporary.flush()?;
        temporary.persist(path).map_err(|error| error.error)?;
        Ok(true)
    }
}

/// The notice for a failed compaction. The caller shows it once per run.
pub fn compact_failed_notice(reason: &str) -> String {
    format!("Prompt history could not be compacted ({reason}); it will be tried again next start.")
}

fn decode(line: &[u8], workspace: &str) -> Option<HistoryEntry> {
    let record: serde_json::Value = serde_json::from_slice(line).ok()?;
    if record.get("v")?.as_u64()? != 1 {
        return None;
    }
    Some(HistoryEntry {
        text: record.get("text")?.as_str()?.to_owned(),
        ts: record.get("ts")?.as_i64()?,
        this_folder: record.get("folder")?.as_str()? == workspace,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composer_seed_caps_submissions_before_duplicate_removal() {
        let folder = tempfile::tempdir().unwrap();
        let (mut history, _) = PromptHistory::open(folder.path(), false, "fixture");
        history.append("older than the window");
        for _ in 0..1000 {
            history.append("same prompt");
        }
        let seed = history.composer_seed();
        assert_eq!(seed.len(), 1000);
        assert!(seed.iter().all(|text| text == "same prompt"));
    }

    #[test]
    fn history_workspace_handles_repository_and_worktree_markers() {
        for file in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let nested = root.path().join("nested");
            std::fs::create_dir(&nested).unwrap();
            let marker = root.path().join(".git");
            if file {
                std::fs::write(&marker, "gitdir: elsewhere\n").unwrap();
            } else {
                std::fs::create_dir(&marker).unwrap();
            }
            assert_eq!(
                history_workspace(&nested),
                root.path().canonicalize().unwrap()
            );
        }
    }

    #[test]
    fn existing_history_directory_loses_group_and_other_permissions() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let (mut history, _) = PromptHistory::open(root.path(), true, "fixture");
        assert_eq!(history.append("private prompt"), None);
        assert_eq!(
            std::fs::metadata(root.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}
