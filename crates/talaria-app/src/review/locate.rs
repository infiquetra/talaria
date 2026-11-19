//! Finding the folder a review shows. Issue #245.

use std::collections::BTreeSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use talaria_core::directory::rows::RowKey;
use talaria_core::review::{RecordView, ReviewTarget};

use crate::worktree::record::review_tree_is_contained;
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
/// folder, else `session.info`. A gateway folder opens only when its canonical
/// path sits inside a root this process already holds. Talaria's own process
/// folder stays closed unless that folder is itself one of those roots.
pub async fn locate(row: &RowKey, ctx: &LocateContext<'_>) -> Located {
    for root in ctx.known_roots.roots() {
        let records = records_in(ctx.git, &root).await;
        if let Some(record) = records.into_iter().find(|record| {
            record.session_id.as_deref() == Some(row.session_id.as_str())
                && record.profile == row.profile
                && review_tree_is_contained(&record.main_root, &record.path)
        }) {
            return open_contained_record(ctx.git, row, record).await;
        }
    }
    let title_row = Some(row.clone());
    if let Some((foreground, Some(cwd))) = ctx.foreground
        && foreground == row
        && !cwd.is_empty()
    {
        return open_trusted(ctx, cwd, title_row).await;
    }
    match ctx.info.info(row).await {
        Ok(SessionInfoReply { cwd: Some(cwd), .. }) if !cwd.is_empty() => {
            open_trusted(ctx, &cwd, title_row).await
        }
        Ok(_) | Err(SessionInfoError::Refused(_) | SessionInfoError::Other(_)) => {
            Located::NotFound(NO_FOLDER.to_owned())
        }
        Err(SessionInfoError::NotFound) => Located::NotFound(SESSION_GONE.to_owned()),
    }
}

/// Open `cwd` only when the candidate, its main checkout, and its git toplevel
/// are each inside a locally trusted root. The review keeps the candidate's
/// canonical path.
async fn open_trusted(ctx: &LocateContext<'_>, cwd: &str, session: Option<RowKey>) -> Located {
    let Some(folder) = trusted_canonical(Path::new(cwd), ctx.known_roots) else {
        return Located::NotFound(NO_FOLDER.to_owned());
    };
    if is_own_folder(&folder) && !ctx.known_roots.explicit_operator_folder(&folder) {
        return Located::NotFound(NO_FOLDER.to_owned());
    }
    let main = match main_root_of(ctx.git, &folder).await {
        Ok(root) => root,
        Err(error) => return Located::NotFound(error.what),
    };
    let Some(main) = trusted_canonical(&main, ctx.known_roots) else {
        return Located::NotFound(NO_FOLDER.to_owned());
    };
    let Some(toplevel) = git_text(
        ctx.git,
        &folder,
        &["-c", "core.fsmonitor=false", "rev-parse", "--show-toplevel"],
    )
    .await
    else {
        return Located::NotFound(NO_FOLDER.to_owned());
    };
    let Some(toplevel) = trusted_canonical(Path::new(&toplevel), ctx.known_roots) else {
        return Located::NotFound(NO_FOLDER.to_owned());
    };
    if toplevel != folder && !folder.starts_with(&toplevel) {
        return Located::NotFound(NO_FOLDER.to_owned());
    }
    open_folder(ctx.git, &folder, session, Some((main, None))).await
}

/// Canonical path of `folder` when it is equal to or inside a trusted root.
///
/// Both sides are canonicalized and compared by path component, so a sibling
/// such as `trusted-evil` and a symlink that resolves outside the root are refused.
fn trusted_canonical(folder: &Path, roots: &KnownRepos) -> Option<PathBuf> {
    let canon = std::fs::canonicalize(folder).ok()?;
    canon_inside(&canon, roots).then_some(canon)
}

/// Whether `canon` is equal to or inside a trusted root. `canon` is already canonical.
fn canon_inside(canon: &Path, roots: &KnownRepos) -> bool {
    roots.roots().iter().any(|root| {
        let Ok(root) = std::fs::canonicalize(root) else {
            return false;
        };
        canon == root || canon.starts_with(&root)
    })
}

/// Talaria's own working folder is never a session's folder by default.
fn is_own_folder(folder: &Path) -> bool {
    let (Ok(own), Ok(folder)) = (std::env::current_dir(), std::fs::canonicalize(folder)) else {
        return false;
    };
    std::fs::canonicalize(own).is_ok_and(|own| own == folder)
}

/// A recorded tree opens only when Git's linked worktree is that directory.
async fn open_contained_record(git: &GitRunner, row: &RowKey, record: WorktreeRecord) -> Located {
    if validated_worktree_root(git, &record.main_root, &record.path)
        .await
        .is_none()
    {
        return Located::NotFound(format!(
            "The worktree {} is not a linked worktree of {}.",
            record.path.to_string_lossy(),
            record.main_root.to_string_lossy(),
        ));
    }
    let view = record_view(&record);
    open_folder(
        git,
        &record.path,
        Some(row.clone()),
        Some((record.main_root, Some(view))),
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
///
/// `known` carries an already identified main checkout and any record view.
async fn open_folder(
    git: &GitRunner,
    folder: &Path,
    session: Option<RowKey>,
    known: Option<(PathBuf, Option<RecordView>)>,
) -> Located {
    if !folder.is_dir() {
        return Located::NotFound(format!(
            "The folder {} does not exist on this machine.",
            folder.to_string_lossy()
        ));
    }
    let (mut main_root, preset) = match known {
        Some((main_root, record)) => (main_root, record),
        None => match main_root_of(git, folder).await {
            Ok(root) => (root, None),
            Err(error) => return Located::NotFound(error.what),
        },
    };
    let record = match preset {
        Some(view) => Some(view),
        None => record_at(git, &main_root, folder).await,
    };
    let folder = if record.is_some() {
        let Some(validated) = validated_worktree_root(git, &main_root, folder).await else {
            return Located::NotFound(format!(
                "The worktree {} is not a linked worktree of {}.",
                folder.to_string_lossy(),
                main_root.to_string_lossy(),
            ));
        };
        match std::fs::canonicalize(&main_root) {
            Ok(root) => main_root = root,
            Err(_) => return Located::NotFound(NO_FOLDER.to_owned()),
        }
        validated
    } else {
        folder.to_path_buf()
    };
    let title = folder
        .file_name()
        .or_else(|| main_root.file_name())
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    Located::Found(ReviewTarget {
        folder,
        main_root,
        session,
        record,
        title,
    })
}

/// The record of the tree whose folder this is, when it is one Talaria launched.
///
/// An out-of-scope record is not attached: its base commit would otherwise
/// become the branch diff.
async fn record_at(git: &GitRunner, main_root: &Path, folder: &Path) -> Option<RecordView> {
    let folder = std::fs::canonicalize(folder).ok()?;
    let record = records_in(git, main_root)
        .await
        .into_iter()
        .find(|record| {
            review_tree_is_contained(&record.main_root, &record.path)
                && std::fs::canonicalize(&record.path).is_ok_and(|path| path == folder)
        })?;
    let validated = validated_worktree_root(git, main_root, &folder).await?;
    (validated == folder).then(|| record_view(&record))
}

const GIT_READ: Duration = Duration::from_secs(10);

/// The canonical root of `path` when it is `main_root`'s linked worktree there.
///
/// Git must report that directory as the work tree root and as a worktree of
/// `main_root`. The check fails closed when the path is missing, is only a
/// directory inside the main checkout, or belongs to another repository.
pub(crate) async fn validated_worktree_root(
    git: &GitRunner,
    main_root: &Path,
    path: &Path,
) -> Option<PathBuf> {
    let folder = std::fs::canonicalize(path).ok()?;
    let main = std::fs::canonicalize(main_root).ok()?;
    if folder.parent() != Some(main.join(".worktrees").as_path()) {
        return None;
    }
    let toplevel = git_text(git, &folder, &["rev-parse", "--show-toplevel"]).await?;
    if std::fs::canonicalize(toplevel).ok()? != folder {
        return None;
    }
    let main_git = git_text(git, &main, &["rev-parse", "--absolute-git-dir"]).await?;
    let tree_git = git_text(git, &folder, &["rev-parse", "--absolute-git-dir"]).await?;
    let main_git = std::fs::canonicalize(main_git).ok()?;
    let tree_git = std::fs::canonicalize(tree_git).ok()?;
    if !worktree_admin_matches(&main_git, &tree_git, &folder) {
        return None;
    }
    let listing = git_stdout(git, &main, &["worktree", "list", "--porcelain"]).await?;
    listing
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .any(|listed| {
            let listed = Path::new(listed.trim());
            let listed = if listed.is_absolute() {
                listed.to_path_buf()
            } else {
                main.join(listed)
            };
            std::fs::canonicalize(listed).is_ok_and(|listed| listed == folder)
        })
        .then_some(folder)
}

/// `tree_git` is the per-worktree admin directory of `main_git`, and its
/// `gitdir` file points back at `folder`'s `.git`.
fn worktree_admin_matches(main_git: &Path, tree_git: &Path, folder: &Path) -> bool {
    let Some(parent) = tree_git.parent() else {
        return false;
    };
    let Ok(expected) = std::fs::canonicalize(main_git.join("worktrees")) else {
        return false;
    };
    if parent != expected {
        return false;
    }
    let Ok(pointer) = std::fs::read_to_string(tree_git.join("gitdir")) else {
        return false;
    };
    let Ok(pointed) = std::fs::canonicalize(pointer.trim()) else {
        return false;
    };
    let Ok(dot_git) = std::fs::canonicalize(folder.join(".git")) else {
        return false;
    };
    if pointed != dot_git {
        return false;
    }
    let Ok(common_raw) = std::fs::read_to_string(tree_git.join("commondir")) else {
        return false;
    };
    let common_raw = common_raw.trim();
    let common = if Path::new(common_raw).is_absolute() {
        PathBuf::from(common_raw)
    } else {
        tree_git.join(common_raw)
    };
    std::fs::canonicalize(common).is_ok_and(|common| common == main_git)
}

async fn git_text(git: &GitRunner, cwd: &Path, args: &[&str]) -> Option<String> {
    let stdout = git_stdout(git, cwd, args).await?;
    let text = stdout.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

async fn git_stdout(git: &GitRunner, cwd: &Path, args: &[&str]) -> Option<String> {
    let out = git.run_read_only(cwd, args, GIT_READ).await.ok()?;
    (out.status == 0).then_some(out.stdout)
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

/// Folders this Talaria may review.
///
/// `roots` are the containment boundary. `explicit` is the subset the operator
/// chose. A main checkout discovered from a gateway folder is neither.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnownRepos {
    roots: BTreeSet<PathBuf>,
    explicit: BTreeSet<PathBuf>,
}

impl KnownRepos {
    /// No known roots.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Remember a containment root.
    pub fn insert(&mut self, main_root: PathBuf) {
        self.roots.insert(main_root);
    }

    /// Remember a folder the operator selected. It is also a containment root.
    pub fn insert_explicit(&mut self, folder: PathBuf) {
        self.explicit.insert(folder.clone());
        self.roots.insert(folder);
    }

    /// The containment roots.
    #[must_use]
    pub fn roots(&self) -> Vec<PathBuf> {
        self.roots.iter().cloned().collect()
    }

    /// Whether `canon` is a folder the operator selected, not a discovered checkout.
    fn explicit_operator_folder(&self, canon: &Path) -> bool {
        self.explicit
            .iter()
            .any(|folder| std::fs::canonicalize(folder).is_ok_and(|folder| folder == canon))
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
