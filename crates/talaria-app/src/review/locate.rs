//! Finding the folder a review shows. Step 0 stubs. Issue #245.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use talaria_core::directory::rows::RowKey;
use talaria_core::review::ReviewTarget;

use crate::worktree::GitRunner;

/// A folder lookup answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Located {
    /// The review target.
    Found(ReviewTarget),
    /// The screen text. No review opens.
    NotFound(String),
}

/// Find a row's folder: the worktree record, else the foreground row's own
/// folder, else `session.info`. Never Talaria's own folder.
pub async fn locate(row: &RowKey, ctx: &LocateContext<'_>) -> Located {
    let _ = (row, ctx);
    Located::NotFound("stub".to_owned())
}

/// What a lookup needs.
#[derive(Clone, Copy)]
pub struct LocateContext<'a> {
    /// The git runner.
    pub git: &'a GitRunner,
    /// The known repository roots.
    pub known_roots: &'a KnownRepos,
    /// The foreground row and its launch folder, when there is one.
    pub foreground: Option<(&'a RowKey, Option<&'a str>)>,
    /// Where `session.info` answers come from.
    pub info: &'a dyn SessionInfoSource,
}

/// Where `session.info` answers come from. A boxed future keeps the trait
/// object-safe without an async-trait crate.
pub trait SessionInfoSource: Send + Sync {
    /// Read one session's folder, branch and model.
    fn info<'a>(
        &'a self,
        row: &'a RowKey,
    ) -> Pin<Box<dyn Future<Output = Result<SessionInfoReply, SessionInfoError>> + Send + 'a>>;
}

/// One `session.info` reply, as locating needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionInfoReply {
    /// The session's working folder, when reported.
    pub cwd: Option<String>,
    /// The session's own model, when reported.
    pub model: Option<String>,
    /// Whether the session is stored away.
    pub lazy: bool,
    /// The session's branch, when reported.
    pub branch: Option<String>,
}

/// Why no `session.info` reply came back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionInfoError {
    /// The session no longer exists on the gateway.
    NotFound,
    /// A 4001 refusal. Locating continues as if no folder were reported.
    Refused(String),
    /// Any other failure.
    Other(String),
}

/// Find `/changes` a folder: the given folder, or the foreground session's.
pub async fn locate_folder(
    git: &GitRunner,
    folder: &Path,
    session: Option<RowKey>,
    title: String,
) -> Located {
    let _ = (git, folder, session, title);
    Located::NotFound("stub".to_owned())
}

/// The main roots this Talaria has seen. Step 0 keeps it empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnownRepos;

impl KnownRepos {
    /// No known roots.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Remember a main root.
    pub fn insert(&mut self, main_root: PathBuf) {
        let _ = main_root;
        todo!()
    }

    /// The known roots.
    #[must_use]
    pub fn roots(&self) -> Vec<PathBuf> {
        todo!()
    }
}
