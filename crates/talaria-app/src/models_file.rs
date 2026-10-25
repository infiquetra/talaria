//! Persistence for saved model choices. Issue 230.

use std::path::Path;
use talaria_core::models::SavedModels;

/// Filesystem operations kept outside the pure model state.
pub trait SavedModelsFile: Sized {
    fn read(path: &Path) -> Result<Self, String>;
    fn write(&self, path: &Path) -> Result<(), String>;
}

impl SavedModelsFile for SavedModels {
    fn read(path: &Path) -> Result<Self, String> {
        let _ = path;
        todo!()
    }

    fn write(&self, path: &Path) -> Result<(), String> {
        let _ = path;
        todo!()
    }
}
