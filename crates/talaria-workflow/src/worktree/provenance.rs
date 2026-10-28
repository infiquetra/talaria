use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Sidecar {
    pub call_digest: String,
    pub occurrence: u32,
    pub branch: String,
    pub base_commit: String,
    pub repo_root: PathBuf,
    pub base_dirty: bool,
}

impl Sidecar {
    pub fn call(&self) -> String {
        format!("{}-{}", self.call_digest, self.occurrence)
    }

    pub fn read(path: &Path) -> Result<Self, String> {
        let data = fs::read(path).map_err(|e| e.to_string())?;
        serde_json::from_slice(&data).map_err(|e| e.to_string())
    }

    pub fn publish(&self, path: &Path) -> Result<(), String> {
        let data = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        let parent = path.parent().ok_or("provenance has no parent directory")?;
        let name = path.file_name().ok_or("provenance has no filename")?;
        // create_new protects temporary files left by a crash or another writer.
        let (temp, mut file) = (0_u64..)
            .find_map(|index| {
                let mut temporary = OsString::from(".");
                temporary.push(name);
                temporary.push(format!(".{index}.tmp"));
                let temp = parent.join(temporary);
                match OpenOptions::new().write(true).create_new(true).open(&temp) {
                    Ok(file) => Some(Ok((temp, file))),
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => None,
                    Err(e) => Some(Err(e.to_string())),
                }
            })
            .ok_or("no temporary provenance filename available")??;
        let result = (|| {
            file.write_all(&data).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            drop(file);
            fs::rename(&temp, path).map_err(|e| e.to_string())?;
            File::open(parent)
                .and_then(|dir| dir.sync_all())
                .map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}

pub(super) fn valid_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(super) fn valid_commit(commit: &str) -> bool {
    matches!(commit.len(), 40 | 64) && commit.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Older Git writes raw paths into line porcelain, so newline paths are ambiguous.
pub(super) fn legacy_paths(output: &[u8], target: &Path) -> Result<Vec<Vec<u8>>, String> {
    if target.as_os_str().as_encoded_bytes().contains(&b'\n') {
        return Err(
            "this Git version needs NUL worktree inventory support for a newline in the path"
                .into(),
        );
    }
    Ok(output
        .split(|&b| b == b'\n')
        .filter_map(|entry| entry.strip_prefix(b"worktree "))
        .map(<[u8]>::to_vec)
        .collect())
}

/// Resolve the existing prefix, including symlinks, without requiring the leaf to exist.
pub(super) fn canonical(path: &Path) -> Result<PathBuf, String> {
    let absolute = std::path::absolute(path).map_err(|e| e.to_string())?;
    let mut prefix = absolute.as_path();
    let mut suffix = Vec::new();
    let mut resolved = loop {
        match fs::canonicalize(prefix) {
            Ok(path) => break path,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let last = prefix
                    .components()
                    .next_back()
                    .ok_or_else(|| e.to_string())?;
                suffix.push(last.as_os_str().to_owned());
                prefix = prefix.parent().ok_or_else(|| e.to_string())?;
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    for part in suffix.iter().rev() {
        match Path::new(part).components().next() {
            Some(Component::ParentDir) => {
                resolved.pop();
            }
            Some(Component::Normal(name)) => resolved.push(name),
            _ => {}
        }
    }
    Ok(resolved)
}
