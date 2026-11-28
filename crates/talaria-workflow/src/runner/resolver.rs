use std::{
    fs,
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

use crate::engine::{ResolveError, WorkflowRef, WorkflowResolver};

#[derive(Clone, Debug)]
pub struct FilesystemResolver {
    workflows_dir: PathBuf,
    root_script_dir: PathBuf,
    cwd: PathBuf,
}
impl FilesystemResolver {
    pub fn new(workflows_dir: PathBuf, root_script_dir: PathBuf) -> Self {
        Self {
            workflows_dir,
            root_script_dir,
            cwd: std::env::current_dir().unwrap_or_default(),
        }
    }
    pub fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.cwd = cwd;
        self
    }
}
const MAX_SCRIPT_BYTES: u64 = 1024 * 1024;

fn unreadable(error: io::Error) -> ResolveError {
    if error.kind() == io::ErrorKind::NotFound {
        ResolveError::NotFound
    } else {
        ResolveError::Unreadable(error.to_string())
    }
}

pub(super) fn read_script(path: &Path) -> Result<String, ResolveError> {
    let before = fs::metadata(path).map_err(unreadable)?;
    if !before.is_file() || before.len() > MAX_SCRIPT_BYTES {
        return Err(ResolveError::Unreadable(
            "workflow script must be a regular file no larger than 1 MiB".into(),
        ));
    }
    let file = crate::untrusted_file::open_read_only(path).map_err(unreadable)?;
    let opened = file.metadata().map_err(unreadable)?;
    if !opened.is_file() || opened.len() > MAX_SCRIPT_BYTES {
        return Err(ResolveError::Unreadable(
            "workflow script must be a regular file no larger than 1 MiB".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if opened.dev() != before.dev() || opened.ino() != before.ino() {
            return Err(ResolveError::Unreadable(
                "workflow script changed while it was opened".into(),
            ));
        }
    }
    let mut bytes = Vec::new();
    file.take(MAX_SCRIPT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(unreadable)?;
    if bytes.len() as u64 > MAX_SCRIPT_BYTES {
        return Err(ResolveError::Unreadable(
            "workflow script exceeds 1 MiB".into(),
        ));
    }
    String::from_utf8(bytes)
        .map_err(|error| ResolveError::Unreadable(format!("workflow script is not UTF-8: {error}")))
}

fn confined(root: &Path, path: &Path) -> Result<PathBuf, ResolveError> {
    let root = root.canonicalize().map_err(unreadable)?;
    let resolved = root.join(path).canonicalize().map_err(unreadable)?;
    if !resolved.starts_with(&root) {
        return Err(ResolveError::NotFound);
    }
    Ok(resolved)
}

fn valid_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn read_from(root: &Path, path: &Path) -> Result<String, ResolveError> {
    let path = confined(root, path)?;
    read_script(&path)
}

fn canonical_named(root: &Path, name: &str) -> Result<PathBuf, ResolveError> {
    let root = root.canonicalize().map_err(unreadable)?;
    let resolved = root
        .join(format!("{name}.js"))
        .canonicalize()
        .map_err(unreadable)?;
    if !resolved.starts_with(&root) {
        return Err(ResolveError::NotFound);
    }
    Ok(resolved)
}
impl WorkflowResolver for FilesystemResolver {
    fn resolve(&self, reference: &WorkflowRef) -> Result<String, ResolveError> {
        match reference {
            WorkflowRef::Name(name) => {
                if name.is_empty()
                    || name.contains(['/', '\\'])
                    || Path::new(name)
                        .components()
                        .any(|c| !matches!(c, Component::Normal(_)))
                {
                    return Err(ResolveError::NotFound);
                }
                read_script(&canonical_named(&self.workflows_dir, name)?)
            }
            WorkflowRef::ScriptPath(path) if valid_relative(Path::new(path)) => {
                let path = Path::new(path);
                match read_from(&self.root_script_dir, path) {
                    Err(ResolveError::NotFound) => read_from(&self.cwd, path),
                    result => result,
                }
            }
            WorkflowRef::ScriptPath(_) => Err(ResolveError::NotFound),
        }
    }
}
