use std::path::PathBuf;

use crate::engine::{ResolveError, WorkflowRef, WorkflowResolver};

pub struct FilesystemResolver;

impl FilesystemResolver {
    pub fn new(_workflows_dir: PathBuf, _root_script_dir: PathBuf) -> Self {
        unimplemented!()
    }
}

impl WorkflowResolver for FilesystemResolver {
    fn resolve(&self, _reference: &WorkflowRef) -> Result<String, ResolveError> {
        unimplemented!()
    }
}
