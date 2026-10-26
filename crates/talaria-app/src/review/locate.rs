//! Finding the folder a review shows. Issue #245.

use std::collections::BTreeSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use talaria_core::directory::rows::RowKey;
use talaria_core::review::{RecordView, ReviewTarget};

use crate::worktree::{GitRunner, WorktreeRecord, main_root_of, records_in};

const NO_FOLDER: &str = "Talaria cannot find this session's folder.";
const SESSION_GONE: &str = "This session no longer exists on the gateway.";

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
    for root in ctx.known_roots.roots() {
        let records = records_in(ctx.git, &root).await;
        if let Some(record) = records.into_iter().find(|record| {
            record.session_id.as_deref() == Some(row.session_id.as_str())
                && record.profile == row.profile
        }) {
            return open_record(ctx.git, row, record).await;
        }
    }
    let title_row = Some(row.clone());
    if let Some((foreground, Some(cwd))) = ctx.foreground
        && foreground == row
        && !cwd.is_empty()
    {
        return open_folder(ctx.git, Path::new(cwd), title_row, None).await;
    }
    match ctx.info.info(row).await {
        Ok(SessionInfoReply { cwd: Some(cwd), .. }) if !cwd.is_empty() => {
            if is_own_folder(Path::new(&cwd)) {
                return Located::NotFound(NO_FOLDER.to_owned());
            }
            open_folder(ctx.git, Path::new(&cwd), title_row, None).await
        }
        Ok(_) | Err(SessionInfoError::Refused(_) | SessionInfoError::Other(_)) => {
            Located::NotFound(NO_FOLDER.to_owned())
        }
        Err(SessionInfoError::NotFound) => Located::NotFound(SESSION_GONE.to_owned()),
    }
}

/// Talaria's own working folder is never a session's folder by default.
fn is_own_folder(folder: &Path) -> bool {
    let (Ok(own), Ok(folder)) = (std::env::current_dir(), std::fs::canonicalize(folder)) else {
        return false;
    };
    std::fs::canonicalize(own).is_ok_and(|own| own == folder)
}

/// A recorded tree: gone from disk is its own message.
async fn open_record(git: &GitRunner, row: &RowKey, record: WorktreeRecord) -> Located {
    if !record.path.is_dir() {
        let name = record
            .path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        return Located::NotFound(format!(
            "The worktree {} is gone. Hermes may have archived it: run hermes worktree restore {name} in {}, then press r.",
            record.path.to_string_lossy(),
            record.main_root.to_string_lossy(),
        ));
    }
    let view = record_view(&record);
    let folder = record.path.clone();
    open_folder(
        git,
        &folder,
        Some(row.clone()),
        Some((record.main_root, view)),
    )
    .await
}

fn record_view(record: &WorktreeRecord) -> RecordView {
    RecordView {
        branch: record.branch.clone(),
        base_ref: record.base_ref.clone(),
        base_commit: record.base_commit.clone(),
    }
}

/// Check a folder holds a work tree and build the review target for it.
async fn open_folder(
    git: &GitRunner,
    folder: &Path,
    session: Option<RowKey>,
    known: Option<(PathBuf, RecordView)>,
) -> Located {
    if !folder.is_dir() {
        return Located::NotFound(format!(
            "The folder {} does not exist on this machine.",
            folder.to_string_lossy()
        ));
    }
    let main_root = match &known {
        Some((main_root, _)) => main_root.clone(),
        None => match main_root_of(git, folder).await {
            Ok(root) => root,
            Err(error) => return Located::NotFound(error.what),
        },
    };
    let record = match known {
        Some((_, view)) => Some(view),
        None => record_at(git, &main_root, folder).await,
    };
    let title = folder
        .file_name()
        .or_else(|| main_root.file_name())
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    Located::Found(ReviewTarget {
        folder: folder.to_path_buf(),
        main_root,
        session,
        record,
        title,
    })
}

/// The record of the tree whose folder this is, when it is one Talaria launched.
async fn record_at(git: &GitRunner, main_root: &Path, folder: &Path) -> Option<RecordView> {
    let folder = std::fs::canonicalize(folder).ok()?;
    records_in(git, main_root)
        .await
        .into_iter()
        .find(|record| std::fs::canonicalize(&record.path).is_ok_and(|path| path == folder))
        .map(|record| record_view(&record))
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
    match open_folder(git, folder, session, None).await {
        Located::Found(mut target) => {
            if !title.is_empty() {
                target.title = title;
            }
            Located::Found(target)
        }
        refused @ Located::NotFound(_) => refused,
    }
}

/// The main roots this Talaria has seen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnownRepos {
    roots: BTreeSet<PathBuf>,
}

impl KnownRepos {
    /// No known roots.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Remember a main root.
    pub fn insert(&mut self, main_root: PathBuf) {
        self.roots.insert(main_root);
    }

    /// The known roots.
    #[must_use]
    pub fn roots(&self) -> Vec<PathBuf> {
        self.roots.iter().cloned().collect()
    }
}

/// `session.info` on a short-lived link minted for the foreground profile, keeping the last
/// reply so one call serves both locating the folder and the drafting check.
pub struct LinkInfoSource {
    home: PathBuf,
    foreground_profile: String,
    last: std::sync::Mutex<Option<Result<SessionInfoReply, SessionInfoError>>>,
}

impl LinkInfoSource {
    /// A source that opens its link on the first call.
    #[must_use]
    pub fn new(home: PathBuf, foreground_profile: String) -> Self {
        Self {
            home,
            foreground_profile,
            last: std::sync::Mutex::new(None),
        }
    }

    /// The reply of the last call, when one was made.
    #[must_use]
    pub fn last(&self) -> Option<Result<SessionInfoReply, SessionInfoError>> {
        self.last
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    async fn fetch(&self, row: &RowKey) -> Result<SessionInfoReply, SessionInfoError> {
        let link = crate::bg::LaunchLink::open(&self.home, &self.foreground_profile)
            .await
            .map_err(|error| SessionInfoError::Other(error.what))?;
        let conn = crate::bg::LaunchConn::Link(link);
        let mut params = serde_json::json!({"session_id": row.session_id});
        if let Some(profile) = conn.routed_profile(&row.profile) {
            params["profile"] = serde_json::Value::String(profile);
        }
        let reply = conn
            .call("session.info", params, std::time::Duration::from_secs(10))
            .await;
        conn.close().await;
        match reply {
            Ok(info) => Ok(SessionInfoReply {
                cwd: non_empty(&info["cwd"]),
                model: non_empty(&info["model"]),
                // A missing `lazy` counts as true, which keeps drafting off.
                lazy: info["lazy"].as_bool().unwrap_or(true),
                branch: non_empty(&info["branch"]),
            }),
            Err(crate::bg::CallFailure::Refused { reason, .. }) => match reason.as_deref() {
                Some("not_found") => Err(SessionInfoError::NotFound),
                _ => Err(SessionInfoError::Refused(
                    reason.unwrap_or_else(|| "refused".to_owned()),
                )),
            },
            Err(other) => Err(SessionInfoError::Other(format!("{other:?}"))),
        }
    }
}

impl SessionInfoSource for LinkInfoSource {
    fn info<'a>(
        &'a self,
        row: &'a RowKey,
    ) -> Pin<Box<dyn Future<Output = Result<SessionInfoReply, SessionInfoError>> + Send + 'a>> {
        Box::pin(async move {
            let result = self.fetch(row).await;
            *self
                .last
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(result.clone());
            result
        })
    }
}

/// A string field, with null, absent and empty all meaning none.
fn non_empty(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}
