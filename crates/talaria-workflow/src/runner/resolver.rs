use std::{
    fs, io,
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
fn read(path: &Path) -> Result<String, ResolveError> {
    fs::read_to_string(path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            ResolveError::NotFound
        } else {
            ResolveError::Unreadable(e.to_string())
        }
    })
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
                let root = self
                    .workflows_dir
                    .canonicalize()
                    .map_err(|_| ResolveError::NotFound)?;
                let path = root.join(format!("{name}.js"));
                let resolved = path.canonicalize().map_err(|e| {
                    if e.kind() == io::ErrorKind::NotFound {
                        ResolveError::NotFound
                    } else {
                        ResolveError::Unreadable(e.to_string())
                    }
                })?;
                if !resolved.starts_with(&root) {
                    return Err(ResolveError::NotFound);
                }
                read(&resolved)
            }
            WorkflowRef::ScriptPath(path) => match read(&self.root_script_dir.join(path)) {
                Err(ResolveError::NotFound) => read(&self.cwd.join(path)),
                result => result,
            },
        }
    }
}
