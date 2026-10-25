//! The directory service: listing, picker renames and exports (#231).

use talaria_core::directory::export::ExportFormat;
use talaria_core::directory::rows::RowKey;
use talaria_core::mutation::MutationLedger;

use super::app::AppMessage;
use super::side_connection::{SideOpenError, SideScope};

/// Where an export goes: a folder with an automatic name, or an explicit path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportTarget {
    Auto { folder: std::path::PathBuf },
    Explicit { path: std::path::PathBuf },
}

/// How a rename ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenameResult {
    Renamed {
        title: String,
        revision: Option<i64>,
    },
    Failed {
        text: String,
    },
}

/// How an export ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportResult {
    Written {
        path: std::path::PathBuf,
        bytes: u64,
    },
    Failed {
        text: String,
    },
}

/// Lists sessions and renames and exports rows over the side connection.
pub struct DirectoryService {
    _held: (),
}

impl DirectoryService {
    /// A service with no connection.
    pub fn new() -> Self {
        Self { _held: () }
    }

    /// Open the side connection for the foreground profile.
    pub async fn open(
        &mut self,
        _home: &std::path::Path,
        _foreground_profile: &str,
    ) -> Result<SideScope, SideOpenError> {
        unimplemented!("#231")
    }

    /// List both scopes of every reachable profile. Returns the generation;
    /// each reply arrives as a message. One task per profile and scope.
    pub fn start_list(&mut self, _tx: tokio::sync::mpsc::Sender<AppMessage>) -> u64 {
        unimplemented!("#231")
    }

    /// Rename a picker row over the side connection.
    pub async fn rename(
        &mut self,
        _ledger: &mut MutationLedger,
        _key: &RowKey,
        _title: &str,
    ) -> RenameResult {
        unimplemented!("#231")
    }

    /// Export a row over the side connection and write the file.
    pub async fn export(
        &mut self,
        _key: &RowKey,
        _format: ExportFormat,
        _redact: bool,
        _target: ExportTarget,
    ) -> ExportResult {
        unimplemented!("#231")
    }

    /// Close the side connection, if one is open.
    pub async fn close(&mut self) {
        unimplemented!("#231")
    }

    /// List only these profiles. The recorder and live tests use it.
    #[doc(hidden)]
    pub fn restrict_profiles(&mut self, _only: Vec<String>) {
        unimplemented!("#231")
    }
}

impl Default for DirectoryService {
    fn default() -> Self {
        Self::new()
    }
}
