//! Private, atomic persistence for saved model choices.

use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

use talaria_core::models::SavedModels;

/// Storage operations for the domain's saved model choices.
pub trait SavedModelsFile {
    fn read(path: &Path) -> Result<Self, String>
    where
        Self: Sized;

    fn write(&self, path: &Path) -> Result<(), String>;
}

impl SavedModelsFile for SavedModels {
    fn read(path: &Path) -> Result<Self, String> {
        match fs::read_to_string(path) {
            Ok(text) => Self::from_json(&text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::empty()),
            Err(error) => Err(file_error("read the file", &error)),
        }
    }

    fn write(&self, path: &Path) -> Result<(), String> {
        // A corrupt file must remain available for its owner to repair.
        Self::read(path)?;
        let text = self
            .to_json()
            .map_err(|_| notice("could not encode the saved choices"))?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut directory = DirBuilder::new();
        directory.recursive(true);
        #[cfg(unix)]
        directory.mode(0o700);
        directory
            .create(parent)
            .map_err(|error| file_error("create the settings folder", &error))?;
        let (temporary, mut file) = temporary_file(parent)?;
        let result = (|| {
            file.write_all(text.as_bytes())
                .map_err(|error| file_error("write the temporary file", &error))?;
            file.sync_all()
                .map_err(|error| file_error("sync the temporary file", &error))?;
            // Check again before replacement in case another writer changed the file.
            Self::read(path)?;
            fs::rename(&temporary, path)
                .map_err(|error| file_error("replace the saved choices", &error))?;
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| file_error("sync the settings folder", &error))
        })();
        drop(file);
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

fn temporary_file(parent: &Path) -> Result<(PathBuf, File), String> {
    for _ in 0..8 {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| notice("could not name the temporary file"))?;
        let name = format!(".models-{:032x}.tmp", u128::from_be_bytes(bytes));
        let path = parent.join(name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(file_error("create the temporary file", &error)),
        }
    }
    Err(notice("could not create a unique temporary file"))
}

fn file_error(action: &str, error: &io::Error) -> String {
    notice(&format!("could not {action}: {}", error.kind()))
}

fn notice(reason: &str) -> String {
    format!(
        "~/.talaria/models.json could not be read ({reason}); favourites and saved defaults are off until it is fixed or removed."
    )
}
