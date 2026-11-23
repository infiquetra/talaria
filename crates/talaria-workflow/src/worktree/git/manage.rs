//! Owner-driven inspection and cleanup of one run's retained worktrees and branches.
//!
//! Nothing here runs by itself: a person asks for it. The list of worktrees comes only from the
//! run's own provenance records (the sidecar files next to the worktrees), and every git call
//! goes through the hardened runner of the parent module.

use super::*;
use serde::Serialize;

fn path_text<S: serde::Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&path.to_string_lossy())
}

/// What one retained worktree holds right now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WorktreeReport {
    pub run_id: String,
    /// The agent's label, filled in by the caller from the run's journal when it names one.
    pub agent: Option<String>,
    pub call: String,
    #[serde(serialize_with = "path_text")]
    pub path: PathBuf,
    pub branch: String,
    pub folder_exists: bool,
    pub branch_exists: bool,
    /// Entries `git status` reports in the folder: modified, staged and new files.
    pub changed_files: usize,
    /// Ignored files and folders in the folder (a whole ignored folder counts once). Removal
    /// deletes them with the folder.
    pub ignored_entries: usize,
    /// How many of `ignored_entries` are verified build output: a folder with a listed name
    /// (see `BUILD_OUTPUT`) whose contents or neighbours confirm what it is. They do not stop a
    /// removal without `--force`.
    pub ignored_build_output: usize,
    /// How many of `ignored_entries` have a listed build output name but failed the check on
    /// disk (for example a `build` folder with no project manifest beside it). They keep the
    /// worktree like any other ignored entry.
    pub ignored_unverified: usize,
    /// Every ignored entry, saying whether it is verified build output and, when a listed name
    /// was not accepted, why.
    pub ignored_details: Vec<IgnoredEntry>,
    /// Commits on the branch, or checked out in the folder, that the source checkout's current
    /// HEAD does not contain.
    pub unmerged_commits: usize,
    pub branch_tip: Option<String>,
    /// The commit the folder has checked out, when it is not the branch tip.
    pub folder_head: Option<String>,
    /// Why this worktree could not be examined completely, when that happened.
    pub problem: Option<String>,
}

/// How one ignored entry in a worktree was classified.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IgnoredKind {
    /// A listed name, and the evidence on disk confirms it is regenerable build output.
    BuildOutput,
    /// A listed name, but the evidence is missing; treated like any other ignored entry.
    UnverifiedBuildOutput,
    /// Anything else: an environment file, a note, a folder with an unlisted name.
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IgnoredEntry {
    /// Path inside the worktree, with a trailing slash for a folder.
    pub path: String,
    pub kind: IgnoredKind,
    /// For an unverified entry: why it was not accepted as build output.
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupAction {
    Removed,
    WouldRemove,
    Kept,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CleanupEntry {
    #[serde(flatten)]
    pub worktree: WorktreeReport,
    pub action: CleanupAction,
    /// Why the worktree was kept, or what went wrong.
    pub reasons: Vec<String>,
    /// Exactly what removal discards (or would discard) that exists nowhere else.
    pub discarded: Vec<String>,
}

fn plural(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

/// Ignored folders with these names are candidates for regenerable build output. A name alone
/// is not enough: `verify_build_output` also needs evidence on disk. A verified folder is
/// deleted by a removal without `--force`, which says so; every other ignored entry (an
/// environment file, a note, an unlisted folder, a listed name without its evidence) keeps the
/// worktree until `--force`.
const BUILD_OUTPUT: &[&str] = &[
    "target",
    "node_modules",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    "dist",
    "build",
    ".venv",
];

/// Files a project keeps beside a `dist` or `build` folder.
const PROJECT_MANIFESTS: &[&str] = &[
    "package.json",
    "pyproject.toml",
    "setup.py",
    "setup.cfg",
    "Cargo.toml",
    "CMakeLists.txt",
    "build.gradle",
    "build.gradle.kts",
    "pom.xml",
    "Makefile",
    "meson.build",
];

struct FolderStatus {
    changed: usize,
    ignored: Vec<IgnoredEntry>,
}

impl FolderStatus {
    fn count(&self, kind: IgnoredKind) -> usize {
        self.ignored.iter().filter(|e| e.kind == kind).count()
    }
}

/// A regular file, judged without following a link.
fn is_file(path: &Path) -> bool {
    path.symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_file())
}

fn has_any(folder: &Path, names: &[&str]) -> bool {
    names.iter().any(|name| is_file(&folder.join(name)))
}

/// Check that `entry`, an ignored folder whose name is `name`, really is what the name says.
/// The error says what evidence is missing. Nothing here follows a link, reads file contents
/// or walks into the folder, except that a `__pycache__` folder is listed once.
fn verify_build_output(entry: &Path, name: &str) -> Result<(), String> {
    let meta = entry
        .symlink_metadata()
        .map_err(|error| format!("it cannot be inspected ({error})"))?;
    if meta.file_type().is_symlink() {
        return Err("it is a symbolic link".to_string());
    }
    if !meta.is_dir() {
        return Err("it is not a folder".to_string());
    }
    let beside = |names: &[&str]| entry.parent().is_some_and(|parent| has_any(parent, names));
    let missing = |what: &str| Err(format!("it {what}"));
    match name {
        "target" => {
            if has_any(entry, &["CACHEDIR.TAG", ".rustc_info.json"]) {
                Ok(())
            } else {
                missing("holds neither CACHEDIR.TAG nor .rustc_info.json")
            }
        }
        ".pytest_cache" | ".mypy_cache" | ".ruff_cache" => {
            if has_any(entry, &["CACHEDIR.TAG"]) {
                Ok(())
            } else {
                missing("holds no CACHEDIR.TAG")
            }
        }
        ".venv" => {
            if has_any(entry, &["pyvenv.cfg"]) {
                Ok(())
            } else {
                missing("holds no pyvenv.cfg")
            }
        }
        "node_modules" => {
            if beside(&["package.json"]) {
                Ok(())
            } else {
                missing("has no package.json beside it")
            }
        }
        "dist" | "build" => {
            if beside(PROJECT_MANIFESTS) {
                Ok(())
            } else {
                missing(
                    "has no project manifest beside it (such as package.json, pyproject.toml or Cargo.toml)",
                )
            }
        }
        "__pycache__" => {
            let only_bytecode = fs::read_dir(entry)
                .map_err(|error| format!("it cannot be listed ({error})"))?
                .all(|item| {
                    item.is_ok_and(|item| {
                        is_file(&item.path())
                            && item
                                .path()
                                .extension()
                                .is_some_and(|ext| ext == "pyc" || ext == "pyo")
                    })
                });
            if only_bytecode {
                Ok(())
            } else {
                missing("holds something other than .pyc or .pyo files")
            }
        }
        _ => missing("is not a known build output folder"),
    }
}

fn os_path(bytes: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        PathBuf::from(OsStr::from_bytes(bytes))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
    }
}

/// Classify one ignored entry reported by `git status`, given its path relative to `folder`.
fn classify_ignored(folder: &Path, raw: &[u8]) -> IgnoredEntry {
    let relative = os_path(raw);
    let shown = relative.to_string_lossy().into_owned();
    let other = IgnoredEntry {
        path: shown.clone(),
        kind: IgnoredKind::Other,
        reason: None,
    };
    let trimmed = shown.strip_suffix('/').unwrap_or(&shown);
    let name = trimmed.rsplit('/').next().unwrap_or(trimmed);
    if !BUILD_OUTPUT.contains(&name) {
        return other;
    }
    // The bytes git printed, not the text shown to people, so a folder under a path that is not
    // UTF-8 is still found. No trailing slash: with one, the checks would follow a link.
    let real = folder.join(os_path(raw.strip_suffix(b"/").unwrap_or(raw)));
    match verify_build_output(&real, name) {
        Ok(()) => IgnoredEntry {
            kind: IgnoredKind::BuildOutput,
            ..other
        },
        Err(why) => IgnoredEntry {
            kind: IgnoredKind::UnverifiedBuildOutput,
            reason: Some(format!("{shown} was not accepted as build output: {why}")),
            ..other
        },
    }
}

fn folder_status(folder: &Path) -> Result<FolderStatus, String> {
    let out = checked(
        folder,
        &[
            "status".as_ref(),
            "--porcelain".as_ref(),
            "-z".as_ref(),
            "--ignored".as_ref(),
            // A user's `status.showUntrackedFiles=no` would hide every untracked and ignored file
            // and make a folder that holds work look empty.
            "--untracked-files=normal".as_ref(),
            "--ignore-submodules=all".as_ref(),
        ],
    )?;
    let mut status = FolderStatus {
        changed: 0,
        ignored: Vec::new(),
    };
    let mut fields = out.split(|&b| b == 0).filter(|field| !field.is_empty());
    while let Some(field) = fields.next() {
        if let Some(path) = field.strip_prefix(b"!! ") {
            status.ignored.push(classify_ignored(folder, path));
        } else {
            status.changed += 1;
            // A rename or copy entry is followed by a second field holding the old path.
            if field.iter().take(2).any(|&b| b == b'R' || b == b'C') {
                fields.next();
            }
        }
    }
    Ok(status)
}

fn head_of(folder: &Path) -> Result<String, String> {
    text(folder, &["rev-parse".as_ref(), "HEAD".as_ref()])
}

impl GitWorktrees {
    /// This run's worktrees folder, refused when a link would point it somewhere else.
    fn worktrees_root(&self) -> Result<PathBuf, String> {
        let root = canonical(&self.directory())?;
        if root == self.run_dir.join("worktrees") {
            Ok(root)
        } else {
            Err(format!(
                "the run's worktrees folder resolves to {}, outside the run folder; nothing was touched",
                root.display()
            ))
        }
    }

    /// Whether `folder` is a worktree of the source repository, whatever it has checked out.
    fn is_repo_worktree(folder: &Path, repo: &Path) -> bool {
        folder.is_dir()
            && checked(folder, &["rev-parse".as_ref(), "--show-toplevel".as_ref()])
                .ok()
                .and_then(|root| path_output(&root).ok())
                .as_deref()
                == Some(folder)
            && Self::common(folder)
                .ok()
                .zip(Self::common(repo).ok())
                .is_some_and(|(a, b)| a == b)
    }

    /// Look at one worktree. The second value is false when its path leaves this run's
    /// worktrees folder; such a path is never examined with git and never touched.
    fn examine(&self, repo: &Path, root: &Path, side: &Sidecar) -> (WorktreeReport, bool) {
        let call = side.call();
        let expected = root.join(&call);
        let resolved = self.path(&call);
        let contained = resolved.as_deref() == Ok(expected.as_path());
        let path = resolved.clone().unwrap_or_else(|_| expected.clone());
        let mut report = WorktreeReport {
            run_id: self.run_id.clone(),
            agent: None,
            call,
            folder_exists: present(&path),
            path,
            branch: side.branch.clone(),
            branch_exists: false,
            changed_files: 0,
            ignored_entries: 0,
            ignored_build_output: 0,
            ignored_unverified: 0,
            ignored_details: Vec::new(),
            unmerged_commits: 0,
            branch_tip: None,
            folder_head: None,
            problem: None,
        };
        if !contained {
            report.problem = Some(match resolved {
                Ok(path) => format!(
                    "the folder resolves to {}, outside this run's worktrees folder; left untouched",
                    path.display()
                ),
                Err(error) => format!("the folder path cannot be resolved: {error}"),
            });
            return (report, false);
        }
        let mut problems = Vec::new();
        let mut tips: Vec<String> = Vec::new();
        match Self::branch_exists(repo, &side.branch) {
            Ok(true) => {
                report.branch_exists = true;
                let reference = format!("refs/heads/{}", side.branch);
                match text(repo, &["rev-parse".as_ref(), reference.as_ref()]) {
                    Ok(tip) => {
                        report.branch_tip = Some(tip);
                        tips.push(reference);
                    }
                    Err(error) => problems.push(format!("cannot read the branch: {error}")),
                }
            }
            Ok(false) => {}
            Err(error) => problems.push(format!("cannot look up the branch: {error}")),
        }
        if report.folder_exists {
            match folder_status(&report.path) {
                Ok(status) => {
                    report.changed_files = status.changed;
                    report.ignored_entries = status.ignored.len();
                    report.ignored_build_output = status.count(IgnoredKind::BuildOutput);
                    report.ignored_unverified = status.count(IgnoredKind::UnverifiedBuildOutput);
                    report.ignored_details = status.ignored;
                }
                Err(error) => problems.push(format!("cannot read the folder's status: {error}")),
            }
            match head_of(&report.path) {
                Ok(head) => {
                    if report.branch_tip.as_deref() != Some(head.as_str()) {
                        report.folder_head = Some(head.clone());
                    }
                    tips.push(head);
                }
                Err(error) => problems.push(format!("cannot read the folder's HEAD: {error}")),
            }
        }
        if !tips.is_empty() {
            let mut args: Vec<&OsStr> = vec!["rev-list".as_ref(), "--count".as_ref()];
            args.extend(tips.iter().map(|tip| OsStr::new(tip.as_str())));
            args.push(OsStr::new("--not"));
            args.push(OsStr::new("HEAD"));
            match text(repo, &args).and_then(|n| n.parse::<usize>().map_err(|e| e.to_string())) {
                Ok(count) => report.unmerged_commits = count,
                Err(error) => problems.push(format!(
                    "cannot compare with the source checkout's HEAD: {error}"
                )),
            }
        }
        if !problems.is_empty() {
            report.problem = Some(problems.join("; "));
        }
        (report, true)
    }

    /// Describe every worktree this run's records name. Read only.
    pub fn inspect(&self) -> Result<Vec<WorktreeReport>, String> {
        let mut state = self.lock();
        let repo = self.repo(&mut state)?;
        let root = self.worktrees_root()?;
        Ok(self
            .owned(&repo)
            .iter()
            .map(|(_, side)| self.examine(&repo, &root, side).0)
            .collect())
    }

    /// Remove this run's worktrees, their branches and their provenance records.
    ///
    /// Without `force`, only a worktree that is clean and whose work the source checkout's HEAD
    /// already contains is removed; everything else is kept with the reason. With `force`,
    /// everything inside the run's own worktrees folder goes, and the entry says what was
    /// discarded. `dry_run` decides and reports but changes nothing.
    pub fn clean_up(&self, force: bool, dry_run: bool) -> Result<Vec<CleanupEntry>, String> {
        let mut state = self.lock();
        let repo = self.repo(&mut state)?;
        let root = self.worktrees_root()?;
        Ok(self
            .owned(&repo)
            .into_iter()
            .map(|(file, side)| {
                let (report, contained) = self.examine(&repo, &root, &side);
                self.settle(&repo, &file, report, contained, force, dry_run)
            })
            .collect())
    }

    fn settle(
        &self,
        repo: &Path,
        sidecar: &Path,
        report: WorktreeReport,
        contained: bool,
        force: bool,
        dry_run: bool,
    ) -> CleanupEntry {
        let entry = |report, action, reasons, discarded| CleanupEntry {
            worktree: report,
            action,
            reasons,
            discarded,
        };
        if !contained {
            let why = report.problem.clone().unwrap_or_default();
            return entry(report, CleanupAction::Kept, vec![why], Vec::new());
        }
        let owned_prefix = format!("talaria/wf/{}/", self.run_id);
        if !report.branch.starts_with(&owned_prefix) {
            let why = format!("branch {} is not under {owned_prefix}", report.branch);
            return entry(report, CleanupAction::Kept, vec![why], Vec::new());
        }
        let mut reasons = Vec::new();
        if report.folder_exists {
            let folder_ok = if force {
                Self::is_repo_worktree(&report.path, repo)
            } else {
                self.owns_folder(&report.path, repo, &report.branch)
            };
            if !folder_ok {
                reasons.push(if force {
                    "the folder is not a git worktree of the source repository; left untouched"
                        .to_string()
                } else {
                    "the folder is not a worktree on its recorded branch (an agent may have switched branches); use --force to remove it anyway".to_string()
                });
                return entry(report, CleanupAction::Kept, reasons, Vec::new());
            }
        }
        if let Some(problem) = &report.problem {
            reasons.push(problem.clone());
        }
        if report.changed_files > 0 {
            reasons.push(format!(
                "{} in the folder",
                plural(report.changed_files, "uncommitted changed file")
            ));
        }
        if report.unmerged_commits > 0 {
            reasons.push(format!(
                "{} not contained in the source checkout's HEAD",
                plural(report.unmerged_commits, "commit")
            ));
        }
        let other_ignored = report.ignored_entries - report.ignored_build_output;
        if other_ignored > 0 {
            reasons.push(format!(
                "holds {} that removal would delete (not verified build output)",
                plural(other_ignored, "ignored file or folder")
            ));
            reasons.extend(
                report
                    .ignored_details
                    .iter()
                    .filter_map(|entry| entry.reason.clone()),
            );
        }
        let mut discarded = Vec::new();
        if force {
            if let Some(problem) = &report.problem {
                discarded.push(format!("could not tell what is lost: {problem}"));
            }
            if report.changed_files > 0 {
                discarded.push(format!(
                    "{} in {}",
                    plural(report.changed_files, "uncommitted changed file"),
                    report.path.display()
                ));
            }
            if report.unmerged_commits > 0 {
                discarded.push(format!(
                    "{} not in the source checkout's HEAD",
                    plural(report.unmerged_commits, "commit")
                ));
            }
            if report.ignored_entries > 0 {
                discarded.push(format!(
                    "{} in {} ({} of them verified build output)",
                    plural(report.ignored_entries, "ignored file or folder"),
                    report.path.display(),
                    report.ignored_build_output
                ));
            }
            if let Some(tip) = &report.branch_tip {
                discarded.push(format!(
                    "branch {} is deleted; it was at {tip} (git branch <name> {tip} restores it)",
                    report.branch
                ));
            }
            if let Some(head) = &report.folder_head {
                discarded.push(format!(
                    "the folder had {head} checked out (git branch <name> {head} restores it)"
                ));
            }
            reasons.clear();
        } else if !reasons.is_empty() {
            return entry(report, CleanupAction::Kept, reasons, Vec::new());
        } else if report.ignored_build_output > 0 {
            discarded.push(format!(
                "{} deleted with the folder",
                plural(report.ignored_build_output, "verified build output folder")
            ));
        }
        if dry_run {
            return entry(report, CleanupAction::WouldRemove, Vec::new(), discarded);
        }
        match self.delete(repo, sidecar, &report, force) {
            Ok(()) => entry(report, CleanupAction::Removed, Vec::new(), discarded),
            Err(error) => entry(report, CleanupAction::Failed, vec![error], Vec::new()),
        }
    }

    /// Folder first, then branch, then the record, so a failure leaves what is needed to retry.
    fn delete(
        &self,
        repo: &Path,
        sidecar: &Path,
        report: &WorktreeReport,
        force: bool,
    ) -> Result<(), String> {
        if report.folder_exists {
            let mut args = vec![OsStr::new("worktree"), OsStr::new("remove")];
            if force {
                args.push(OsStr::new("--force"));
            }
            args.push(report.path.as_os_str());
            checked(repo, &args).map_err(|e| format!("cannot remove the folder: {e}"))?;
        } else if Self::registered(repo, &report.path)? {
            // The folder is already gone. `worktree remove` on its path drops only this entry;
            // a bare `worktree prune` would drop every stale entry in the repository.
            let mut args = vec![OsStr::new("worktree"), OsStr::new("remove")];
            if force {
                args.push(OsStr::new("--force"));
            }
            args.push(report.path.as_os_str());
            checked(repo, &args).map_err(|e| format!("cannot drop the worktree entry: {e}"))?;
        }
        if Self::branch_exists(repo, &report.branch)? {
            // The safe form refuses a branch whose work HEAD lacks; only --force discards it.
            let flag = if force { "-D" } else { "-d" };
            checked(
                repo,
                &[
                    "branch".as_ref(),
                    flag.as_ref(),
                    "--".as_ref(),
                    report.branch.as_ref(),
                ],
            )
            .map_err(|e| format!("the folder is gone but the branch stays: {e}"))?;
        }
        fs::remove_file(sidecar).map_err(|e| format!("cannot remove the provenance record: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(repo: &Path, args: &[&str]) -> String {
        let args: Vec<&OsStr> = args.iter().map(|a| OsStr::new(*a)).collect();
        text(repo, &args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
    }

    fn commit(repo: &Path, name: &str) {
        fs::write(repo.join(name), name).unwrap();
        git(repo, &["add", "--", name]);
        git(
            repo,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-m",
                name,
            ],
        );
    }

    struct Fixture {
        _temp: tempfile::TempDir,
        repo: PathBuf,
        run: PathBuf,
        provider: GitWorktrees,
    }

    fn fixture() -> Fixture {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        commit(&repo, "base.txt");
        let run = temp.path().join("run1");
        fs::create_dir(&run).unwrap();
        let provider = GitWorktrees::new(run.clone(), repo.clone(), "run1".into());
        let repo = canonical(&repo).unwrap();
        let run = canonical(&run).unwrap();
        Fixture {
            _temp: temp,
            repo,
            run,
            provider,
        }
    }

    impl Fixture {
        fn tree(&self, occurrence: u32) -> PathBuf {
            self.provider.for_call(&"a".repeat(64), occurrence).unwrap()
        }
        fn actions(&self, force: bool, dry_run: bool) -> Vec<(String, CleanupAction)> {
            self.provider
                .clean_up(force, dry_run)
                .unwrap()
                .into_iter()
                .map(|e| (e.worktree.call, e.action))
                .collect()
        }
        fn branch(&self, occurrence: u32) -> String {
            format!("talaria/wf/run1/{}-{occurrence}", "a".repeat(64))
        }
        fn has_branch(&self, occurrence: u32) -> bool {
            GitWorktrees::branch_exists(&self.repo, &self.branch(occurrence)).unwrap()
        }
        fn sidecar(&self, occurrence: u32) -> PathBuf {
            self.run
                .join("worktrees")
                .join(format!("{}-{occurrence}.json", "a".repeat(64)))
        }
    }

    #[test]
    fn a_clean_merged_worktree_is_removed_with_its_branch_and_record() {
        let f = fixture();
        let tree = f.tree(0);
        let report = f.provider.inspect().unwrap().remove(0);
        assert!(report.folder_exists && report.branch_exists);
        assert_eq!((report.changed_files, report.unmerged_commits), (0, 0));
        assert_eq!(f.actions(false, false)[0].1, CleanupAction::Removed);
        assert!(!tree.exists());
        assert!(!f.has_branch(0));
        assert!(!f.sidecar(0).exists());
        assert!(f.provider.inspect().unwrap().is_empty());
    }

    #[test]
    fn a_branch_merged_into_head_is_removed_even_though_it_has_commits() {
        let f = fixture();
        let tree = f.tree(0);
        commit(&tree, "work.txt");
        assert_eq!(f.provider.inspect().unwrap()[0].unmerged_commits, 1);
        git(&f.repo, &["merge", "--ff-only", &f.branch(0)]);
        assert_eq!(f.provider.inspect().unwrap()[0].unmerged_commits, 0);
        assert_eq!(f.actions(false, false)[0].1, CleanupAction::Removed);
        assert!(!f.has_branch(0) && !tree.exists());
    }

    #[test]
    fn a_git_setting_that_hides_untracked_files_does_not_make_a_folder_look_empty() {
        let f = fixture();
        let tree = f.tree(0);
        git(&tree, &["config", "status.showUntrackedFiles", "no"]);
        fs::write(tree.join("new.txt"), "x").unwrap();
        let entry = f.provider.clean_up(false, false).unwrap().remove(0);
        assert_eq!(entry.action, CleanupAction::Kept);
        assert_eq!(entry.worktree.changed_files, 1);
        assert!(tree.join("new.txt").exists());
    }

    #[test]
    fn a_dirty_worktree_is_kept_with_its_changed_file_count() {
        let f = fixture();
        let tree = f.tree(0);
        fs::write(tree.join("new.txt"), "x").unwrap();
        fs::write(tree.join("base.txt"), "edited").unwrap();
        let entry = f.provider.clean_up(false, false).unwrap().remove(0);
        assert_eq!(entry.action, CleanupAction::Kept);
        assert_eq!(entry.worktree.changed_files, 2);
        assert!(
            entry
                .reasons
                .join(" ")
                .contains("2 uncommitted changed files")
        );
        assert!(tree.join("new.txt").exists());
        assert!(f.has_branch(0) && f.sidecar(0).exists());
    }

    #[test]
    fn a_worktree_with_unmerged_commits_is_kept_and_its_branch_survives() {
        let f = fixture();
        let tree = f.tree(0);
        commit(&tree, "work.txt");
        let entry = f.provider.clean_up(false, false).unwrap().remove(0);
        assert_eq!(entry.action, CleanupAction::Kept);
        assert_eq!(entry.worktree.unmerged_commits, 1);
        assert!(entry.reasons.join(" ").contains("1 commit not contained"));
        assert!(tree.exists() && f.has_branch(0));
    }

    #[test]
    fn commits_made_on_a_detached_head_in_the_folder_count_as_unmerged() {
        let f = fixture();
        let tree = f.tree(0);
        git(&tree, &["checkout", "--detach"]);
        commit(&tree, "work.txt");
        let entry = f.provider.clean_up(false, false).unwrap().remove(0);
        assert_eq!(entry.action, CleanupAction::Kept);
        assert_eq!(entry.worktree.unmerged_commits, 1);
        assert!(tree.join("work.txt").exists());
    }

    #[test]
    fn force_removes_dirty_and_unmerged_work_and_says_what_was_discarded() {
        let f = fixture();
        let tree = f.tree(0);
        commit(&tree, "work.txt");
        fs::write(tree.join("new.txt"), "x").unwrap();
        let tip = f.provider.inspect().unwrap()[0].branch_tip.clone().unwrap();
        let entry = f.provider.clean_up(true, false).unwrap().remove(0);
        assert_eq!(entry.action, CleanupAction::Removed);
        let said = entry.discarded.join("\n");
        assert!(said.contains("1 uncommitted changed file"), "{said}");
        assert!(
            said.contains("1 commit not in the source checkout's HEAD"),
            "{said}"
        );
        assert!(said.contains(&tip), "{said}");
        assert!(!tree.exists() && !f.has_branch(0) && !f.sidecar(0).exists());
    }

    #[test]
    fn dry_run_decides_but_changes_nothing() {
        let f = fixture();
        let clean = f.tree(0);
        let dirty = f.tree(1);
        fs::write(dirty.join("new.txt"), "x").unwrap();
        let rows = f.actions(false, true);
        assert_eq!(rows[0].1, CleanupAction::WouldRemove);
        assert_eq!(rows[1].1, CleanupAction::Kept);
        let forced = f.provider.clean_up(true, true).unwrap();
        assert!(
            forced
                .iter()
                .all(|e| e.action == CleanupAction::WouldRemove)
        );
        assert!(
            forced[1].discarded[0].contains("1 uncommitted changed file"),
            "{:?}",
            forced[1].discarded
        );
        assert!(clean.exists() && dirty.exists());
        assert!(f.has_branch(0) && f.has_branch(1));
        assert!(f.sidecar(0).exists() && f.sidecar(1).exists());
    }

    #[test]
    fn a_missing_folder_with_a_merged_branch_is_cleaned_up_too() {
        let f = fixture();
        let tree = f.tree(0);
        fs::remove_dir_all(&tree).unwrap();
        let report = f.provider.inspect().unwrap().remove(0);
        assert!(!report.folder_exists && report.branch_exists);
        assert_eq!(f.actions(false, false)[0].1, CleanupAction::Removed);
        assert!(!f.has_branch(0) && !f.sidecar(0).exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_that_is_a_link_out_of_the_worktrees_folder_is_never_touched() {
        let f = fixture();
        let tree = f.tree(0);
        // Move the real worktree out of the run and leave a link where it was.
        let outside = f.repo.parent().unwrap().join("elsewhere");
        fs::rename(&tree, &outside).unwrap();
        std::os::unix::fs::symlink(&outside, &tree).unwrap();
        fs::write(outside.join("precious.txt"), "keep").unwrap();
        for force in [false, true] {
            let entry = f.provider.clean_up(force, false).unwrap().remove(0);
            assert_eq!(entry.action, CleanupAction::Kept, "force={force}");
            assert!(
                entry
                    .reasons
                    .join(" ")
                    .contains("outside this run's worktrees folder")
            );
        }
        assert!(outside.join("precious.txt").exists());
        assert!(f.has_branch(0) && f.sidecar(0).exists());
        let report = f.provider.inspect().unwrap().remove(0);
        assert_eq!(report.changed_files, 0, "no git ran in the foreign folder");
    }

    #[cfg(unix)]
    #[test]
    fn a_worktrees_folder_that_is_a_link_elsewhere_is_refused() {
        let f = fixture();
        f.tree(0);
        let moved = f.repo.parent().unwrap().join("moved-worktrees");
        fs::rename(f.run.join("worktrees"), &moved).unwrap();
        std::os::unix::fs::symlink(&moved, f.run.join("worktrees")).unwrap();
        assert!(f.provider.inspect().is_err());
        assert!(f.provider.clean_up(true, false).is_err());
        assert!(moved.read_dir().unwrap().count() >= 2);
    }

    #[test]
    fn a_branch_outside_the_runs_prefix_is_never_deleted() {
        let f = fixture();
        f.tree(0);
        git(&f.repo, &["branch", "talaria/wf/other/keep"]);
        git(&f.repo, &["branch", "feature"]);
        f.provider.clean_up(true, false).unwrap();
        assert!(GitWorktrees::branch_exists(&f.repo, "talaria/wf/other/keep").unwrap());
        assert!(GitWorktrees::branch_exists(&f.repo, "feature").unwrap());
    }

    #[test]
    fn a_sidecar_naming_another_branch_is_not_listed_or_removed() {
        let f = fixture();
        f.tree(0);
        git(&f.repo, &["branch", "feature"]);
        let file = f.sidecar(0);
        let mut side = Sidecar::read(&file).unwrap();
        side.branch = "feature".into();
        side.publish(&file).unwrap();
        assert!(f.provider.inspect().unwrap().is_empty());
        assert!(f.provider.clean_up(true, false).unwrap().is_empty());
        assert!(GitWorktrees::branch_exists(&f.repo, "feature").unwrap());
    }

    #[test]
    fn a_folder_switched_to_another_branch_is_kept_without_force() {
        let f = fixture();
        let tree = f.tree(0);
        git(&tree, &["checkout", "-b", "agent-own"]);
        let entry = f.provider.clean_up(false, false).unwrap().remove(0);
        assert_eq!(entry.action, CleanupAction::Kept);
        assert!(entry.reasons.join(" ").contains("recorded branch"));
        assert!(tree.exists());
    }

    fn ignore_in_source(f: &Fixture) {
        fs::write(f.repo.join(".gitignore"), "target/\n.env\n").unwrap();
        git(&f.repo, &["add", ".gitignore"]);
        git(
            &f.repo,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-m",
                "ignore",
            ],
        );
    }

    #[test]
    fn force_with_an_unreadable_folder_says_it_could_not_tell_what_is_lost() {
        let f = fixture();
        let tree = f.tree(0);
        // Damage the worktree's index so git cannot report its status.
        let gitdir = fs::read_to_string(tree.join(".git")).unwrap();
        let gitdir = PathBuf::from(gitdir.trim().strip_prefix("gitdir: ").unwrap());
        fs::write(gitdir.join("index"), "not an index").unwrap();
        let report = f.provider.inspect().unwrap().remove(0);
        assert!(report.problem.is_some(), "{report:?}");
        let kept = f.provider.clean_up(false, false).unwrap().remove(0);
        assert_eq!(kept.action, CleanupAction::Kept);
        assert!(tree.exists());
        let tip = report.branch_tip.clone().unwrap();
        let forced = f.provider.clean_up(true, false).unwrap().remove(0);
        assert_eq!(forced.action, CleanupAction::Removed);
        let said = forced.discarded.join("\n");
        assert!(said.contains("could not tell what is lost"), "{said}");
        assert!(
            said.contains(&tip),
            "the branch tip is always printed: {said}"
        );
    }

    #[test]
    fn force_always_prints_the_branch_tip_and_a_detached_folder_head() {
        let f = fixture();
        let tree = f.tree(0);
        let tip = f.provider.inspect().unwrap()[0].branch_tip.clone().unwrap();
        git(&tree, &["checkout", "--detach"]);
        commit(&tree, "work.txt");
        let head = git(&tree, &["rev-parse", "HEAD"]);
        let said = f
            .provider
            .clean_up(true, false)
            .unwrap()
            .remove(0)
            .discarded
            .join("\n");
        assert!(said.contains(&tip) && said.contains(&head), "{said}");
    }

    #[test]
    fn ignored_files_keep_a_worktree_without_force_but_build_output_does_not() {
        let f = fixture();
        ignore_in_source(&f);
        let build = f.tree(0);
        fs::create_dir(build.join("target")).unwrap();
        fs::write(build.join("target").join("out.bin"), "x").unwrap();
        fs::write(build.join("target").join("CACHEDIR.TAG"), "tag").unwrap();
        let secret = f.tree(1);
        fs::write(secret.join(".env"), "TOKEN=1").unwrap();
        let rows = f.provider.inspect().unwrap();
        assert_eq!(
            (
                rows[0].changed_files,
                rows[0].ignored_entries,
                rows[0].ignored_build_output
            ),
            (0, 1, 1)
        );
        assert_eq!(
            (
                rows[1].changed_files,
                rows[1].ignored_entries,
                rows[1].ignored_build_output
            ),
            (0, 1, 0)
        );
        let preview = f.provider.clean_up(false, true).unwrap();
        assert_eq!(preview[0].action, CleanupAction::WouldRemove);
        assert!(
            preview[0]
                .discarded
                .join(" ")
                .contains("1 verified build output folder")
        );
        assert_eq!(preview[1].action, CleanupAction::Kept);
        assert!(
            preview[1]
                .reasons
                .join(" ")
                .contains("1 ignored file or folder")
        );
        let rows = f.provider.clean_up(false, false).unwrap();
        assert_eq!(rows[0].action, CleanupAction::Removed);
        assert!(!build.exists());
        assert_eq!(rows[1].action, CleanupAction::Kept);
        assert!(secret.join(".env").exists());
        let forced = f.provider.clean_up(true, false).unwrap().remove(0);
        assert_eq!(forced.action, CleanupAction::Removed);
        assert!(
            forced
                .discarded
                .join(" ")
                .contains("1 ignored file or folder")
        );
        assert!(!secret.exists());
    }

    /// A worktree whose source repository ignores every listed name, plus `.env`.
    fn ignoring_fixture() -> (Fixture, PathBuf) {
        let f = fixture();
        fs::write(
            f.repo.join(".gitignore"),
            "target\nnode_modules\n__pycache__\n.pytest_cache\n.mypy_cache\n.ruff_cache\n\
             dist\nbuild\n.venv\n.env\nnotes\n",
        )
        .unwrap();
        git(&f.repo, &["add", ".gitignore"]);
        git(
            &f.repo,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-m",
                "ignore",
            ],
        );
        let tree = f.tree(0);
        (f, tree)
    }

    fn make(tree: &Path, relative: &str, files: &[&str]) {
        let folder = tree.join(relative);
        fs::create_dir_all(&folder).unwrap();
        for file in files {
            fs::write(folder.join(file), "x").unwrap();
        }
    }

    /// Commit a project file in the worktree and merge it into the source checkout, so the
    /// worktree stays clean and has no unmerged commits.
    fn track(f: &Fixture, tree: &Path, relative: &str) {
        let file = tree.join(relative);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "x").unwrap();
        git(tree, &["add", "--", relative]);
        git(
            tree,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-m",
                relative,
            ],
        );
        git(&f.repo, &["merge", "--ff-only", &f.branch(0)]);
    }

    /// The preview of a removal without `--force`; it changes nothing.
    fn preview(f: &Fixture) -> CleanupEntry {
        f.provider.clean_up(false, true).unwrap().remove(0)
    }

    fn assert_accepted(f: &Fixture, name: &str) {
        let report = f.provider.inspect().unwrap().remove(0);
        assert_eq!(
            (
                report.ignored_entries,
                report.ignored_build_output,
                report.ignored_unverified
            ),
            (1, 1, 0),
            "{name}: {report:?}"
        );
        assert_eq!(report.ignored_details[0].kind, IgnoredKind::BuildOutput);
        assert_eq!(report.ignored_details[0].reason, None);
        let entry = preview(f);
        assert_eq!(
            entry.action,
            CleanupAction::WouldRemove,
            "{name}: {entry:?}"
        );
    }

    fn assert_rejected(f: &Fixture, name: &str, why: &str) {
        let report = f.provider.inspect().unwrap().remove(0);
        assert_eq!(
            (
                report.ignored_entries,
                report.ignored_build_output,
                report.ignored_unverified
            ),
            (1, 0, 1),
            "{name}: {report:?}"
        );
        assert_eq!(
            report.ignored_details[0].kind,
            IgnoredKind::UnverifiedBuildOutput
        );
        let entry = preview(f);
        assert_eq!(entry.action, CleanupAction::Kept, "{name}: {entry:?}");
        let said = entry.reasons.join("; ");
        assert!(
            said.contains(&format!("{name}/ was not accepted as build output")),
            "{said}"
        );
        assert!(said.contains(why), "{said}");
        assert!(
            report.ignored_details[0]
                .reason
                .as_deref()
                .is_some_and(|r| r.contains(why))
        );
    }

    #[test]
    fn target_needs_a_cache_tag_or_cargo_info_file() {
        let (f, tree) = ignoring_fixture();
        make(&tree, "target", &["out.bin"]);
        assert_rejected(&f, "target", "neither CACHEDIR.TAG nor .rustc_info.json");
        make(&tree, "target", &["CACHEDIR.TAG"]);
        assert_accepted(&f, "target");
        fs::remove_file(tree.join("target/CACHEDIR.TAG")).unwrap();
        make(&tree, "target", &[".rustc_info.json"]);
        assert_accepted(&f, "target");
    }

    #[test]
    fn node_modules_needs_a_package_json_beside_it() {
        let (f, tree) = ignoring_fixture();
        make(&tree, "node_modules", &["left-pad.js"]);
        assert_rejected(&f, "node_modules", "no package.json beside it");
        track(&f, &tree, "package.json");
        assert_accepted(&f, "node_modules");
    }

    #[test]
    fn a_nested_folder_is_checked_against_the_folder_that_holds_it() {
        let (f, tree) = ignoring_fixture();
        track(&f, &tree, "app/main.js");
        make(&tree, "app/node_modules", &["x.js"]);
        // A manifest at the top does not vouch for a folder deeper down.
        track(&f, &tree, "package.json");
        let report = f.provider.inspect().unwrap().remove(0);
        assert_eq!(
            (report.ignored_build_output, report.ignored_unverified),
            (0, 1),
            "{report:?}"
        );
        assert_eq!(report.ignored_details[0].path, "app/node_modules/");
        track(&f, &tree, "app/package.json");
        assert_accepted(&f, "app/node_modules");
    }

    #[test]
    fn pycache_must_hold_only_bytecode() {
        let (f, tree) = ignoring_fixture();
        make(
            &tree,
            "__pycache__",
            &["mod.cpython-314.pyc", "mod.cpython-38.pyo"],
        );
        assert_accepted(&f, "__pycache__");
        make(&tree, "__pycache__", &["notes.txt"]);
        assert_rejected(&f, "__pycache__", "something other than .pyc or .pyo files");
        fs::remove_file(tree.join("__pycache__/notes.txt")).unwrap();
        make(&tree, "__pycache__/inner", &["mod.pyc"]);
        assert_rejected(&f, "__pycache__", "something other than .pyc or .pyo files");
    }

    #[test]
    fn an_empty_pycache_folder_is_accepted() {
        let temp = tempfile::tempdir().unwrap();
        make(temp.path(), "__pycache__", &[]);
        assert_eq!(
            verify_build_output(&temp.path().join("__pycache__"), "__pycache__"),
            Ok(())
        );
    }

    #[test]
    fn tool_caches_need_a_cache_tag() {
        for name in [".pytest_cache", ".mypy_cache", ".ruff_cache"] {
            let (f, tree) = ignoring_fixture();
            make(&tree, name, &["data.json"]);
            assert_rejected(&f, name, "no CACHEDIR.TAG");
            make(&tree, name, &["CACHEDIR.TAG"]);
            assert_accepted(&f, name);
        }
    }

    #[test]
    fn a_virtual_environment_needs_pyvenv_cfg() {
        let (f, tree) = ignoring_fixture();
        make(&tree, ".venv", &["notes.txt"]);
        assert_rejected(&f, ".venv", "no pyvenv.cfg");
        make(&tree, ".venv", &["pyvenv.cfg"]);
        assert_accepted(&f, ".venv");
    }

    #[test]
    fn dist_and_build_need_a_project_manifest_beside_them() {
        for name in ["dist", "build"] {
            let (f, tree) = ignoring_fixture();
            make(&tree, name, &["hand-made.txt"]);
            assert_rejected(&f, name, "no project manifest beside it");
            let entry = preview(&f);
            assert!(tree.join(name).join("hand-made.txt").exists());
            assert_eq!(entry.action, CleanupAction::Kept);
            track(&f, &tree, "Makefile");
            assert_accepted(&f, name);
        }
    }

    #[test]
    fn every_manifest_name_vouches_for_a_build_folder() {
        for manifest in PROJECT_MANIFESTS {
            let (f, tree) = ignoring_fixture();
            make(&tree, "dist", &["out.js"]);
            track(&f, &tree, manifest);
            assert_accepted(&f, manifest);
        }
    }

    #[test]
    fn a_manifest_that_is_a_folder_does_not_vouch() {
        let (f, tree) = ignoring_fixture();
        make(&tree, "build", &["out.o"]);
        make(&tree, "Makefile", &[]);
        assert_rejected(&f, "build", "no project manifest beside it");
    }

    #[test]
    fn a_rejected_folder_keeps_the_worktree_and_its_files_without_force() {
        let (f, tree) = ignoring_fixture();
        make(&tree, "build", &["hand-made.txt"]);
        let entry = f.provider.clean_up(false, false).unwrap().remove(0);
        assert_eq!(entry.action, CleanupAction::Kept);
        assert!(tree.join("build/hand-made.txt").exists());
        assert!(f.has_branch(0) && f.sidecar(0).exists());
        let forced = f.provider.clean_up(true, false).unwrap().remove(0);
        assert_eq!(forced.action, CleanupAction::Removed);
        let said = forced.discarded.join(" ");
        assert!(said.contains("1 ignored file or folder"), "{said}");
        assert!(said.contains("0 of them verified build output"), "{said}");
        assert!(!tree.exists());
    }

    #[test]
    fn unlisted_ignored_entries_are_other_and_carry_no_reason() {
        let (f, tree) = ignoring_fixture();
        make(&tree, "notes", &["a.txt"]);
        fs::write(tree.join(".env"), "TOKEN=1").unwrap();
        let report = f.provider.inspect().unwrap().remove(0);
        assert_eq!(report.ignored_entries, 2);
        assert!(
            report
                .ignored_details
                .iter()
                .all(|e| e.kind == IgnoredKind::Other && e.reason.is_none()),
            "{report:?}"
        );
        assert_eq!(
            (report.ignored_build_output, report.ignored_unverified),
            (0, 0)
        );
    }

    #[test]
    fn a_file_with_a_listed_name_is_not_build_output() {
        let (f, tree) = ignoring_fixture();
        fs::write(tree.join("target"), "not a folder").unwrap();
        let report = f.provider.inspect().unwrap().remove(0);
        assert_eq!(
            (report.ignored_build_output, report.ignored_unverified),
            (0, 1),
            "{report:?}"
        );
        assert!(
            report.ignored_details[0]
                .reason
                .as_deref()
                .unwrap()
                .contains("it is not a folder")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_with_a_listed_name_is_never_build_output_and_is_not_followed() {
        for name in ["target", "node_modules", ".venv", "build"] {
            let (f, tree) = ignoring_fixture();
            // The link's destination has every piece of evidence; it must still not count.
            let outside = f
                .repo
                .parent()
                .unwrap()
                .join(format!("outside-{}", name.trim_matches('.')));
            fs::create_dir(&outside).unwrap();
            for file in [
                "CACHEDIR.TAG",
                ".rustc_info.json",
                "pyvenv.cfg",
                "precious.txt",
            ] {
                fs::write(outside.join(file), "x").unwrap();
            }
            track(&f, &tree, "package.json");
            track(&f, &tree, "Makefile");
            std::os::unix::fs::symlink(&outside, tree.join(name)).unwrap();
            let report = f.provider.inspect().unwrap().remove(0);
            assert_eq!(
                (report.ignored_build_output, report.ignored_unverified),
                (0, 1),
                "{name}: {report:?}"
            );
            assert!(
                report.ignored_details[0]
                    .reason
                    .as_deref()
                    .unwrap()
                    .contains("it is a symbolic link"),
                "{name}: {report:?}"
            );
            let kept = f.provider.clean_up(false, false).unwrap().remove(0);
            assert_eq!(kept.action, CleanupAction::Kept, "{name}");
            assert!(outside.join("precious.txt").exists());
            let forced = f.provider.clean_up(true, false).unwrap().remove(0);
            assert_eq!(forced.action, CleanupAction::Removed, "{name}");
            assert!(
                outside.join("precious.txt").exists(),
                "{name}: removal followed the link"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_link_inside_pycache_is_not_bytecode() {
        let (f, tree) = ignoring_fixture();
        make(&tree, "__pycache__", &[]);
        let outside = f.repo.parent().unwrap().join("outside.pyc");
        fs::write(&outside, "x").unwrap();
        std::os::unix::fs::symlink(&outside, tree.join("__pycache__/mod.pyc")).unwrap();
        assert_rejected(&f, "__pycache__", "something other than .pyc or .pyo files");
    }

    #[test]
    fn a_verified_folder_next_to_an_unverified_one_still_keeps_the_worktree() {
        let (f, tree) = ignoring_fixture();
        make(&tree, "target", &["CACHEDIR.TAG"]);
        make(&tree, "dist", &["out.js"]);
        let report = f.provider.inspect().unwrap().remove(0);
        assert_eq!(
            (
                report.ignored_entries,
                report.ignored_build_output,
                report.ignored_unverified
            ),
            (2, 1, 1)
        );
        let entry = preview(&f);
        assert_eq!(entry.action, CleanupAction::Kept);
        let said = entry.reasons.join("; ");
        assert!(
            said.contains("dist/ was not accepted as build output"),
            "{said}"
        );
        assert!(!said.contains("target/ was not accepted"), "{said}");
        let body = serde_json::to_value(&report).unwrap();
        assert_eq!(body["ignored_unverified"], 1);
        let kinds: Vec<_> = body["ignored_details"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["kind"].as_str().unwrap().to_string())
            .collect();
        assert!(kinds.contains(&"build_output".to_string()), "{kinds:?}");
        assert!(
            kinds.contains(&"unverified_build_output".to_string()),
            "{kinds:?}"
        );
    }

    #[test]
    fn a_path_with_unusual_characters_is_checked_where_it_really_is() {
        let (f, tree) = ignoring_fixture();
        // A space and a non-ASCII letter make git quote the path without -z.
        make(&tree, "my app/caf\u{e9}/node_modules", &["x.js"]);
        track(&f, &tree, "my app/caf\u{e9}/package.json");
        let report = f.provider.inspect().unwrap().remove(0);
        assert_eq!(
            (report.ignored_build_output, report.ignored_unverified),
            (1, 0),
            "{report:?}"
        );
    }

    #[test]
    fn a_renamed_file_is_one_changed_entry() {
        let f = fixture();
        let tree = f.tree(0);
        git(&tree, &["mv", "base.txt", "moved.txt"]);
        let report = f.provider.inspect().unwrap().remove(0);
        assert_eq!(report.changed_files, 1, "{report:?}");
        assert_eq!(report.ignored_entries, 0);
    }

    #[test]
    fn removing_a_worktree_whose_folder_is_gone_leaves_other_stale_entries_alone() {
        let f = fixture();
        let tree = f.tree(0);
        // An unrelated worktree of the same repository whose folder is also gone.
        let stranger = f.repo.parent().unwrap().join("stranger");
        git(
            &f.repo,
            &[
                "worktree",
                "add",
                "-b",
                "stranger",
                stranger.to_str().unwrap(),
            ],
        );
        fs::remove_dir_all(&stranger).unwrap();
        fs::remove_dir_all(&tree).unwrap();
        let before = git(&f.repo, &["worktree", "list", "--porcelain"]);
        assert!(before.contains("stranger"));
        let entry = f.provider.clean_up(false, false).unwrap().remove(0);
        assert_eq!(entry.action, CleanupAction::Removed, "{entry:?}");
        let after = git(&f.repo, &["worktree", "list", "--porcelain"]);
        assert!(
            after.contains("stranger"),
            "the other entry survives: {after}"
        );
        assert!(
            !after.contains(&"a".repeat(64)),
            "this run's entry is gone: {after}"
        );
        assert!(!f.has_branch(0) && !f.sidecar(0).exists());
    }
}
