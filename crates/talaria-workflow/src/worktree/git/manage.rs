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
    /// Commits on the branch, or checked out in the folder, that the source checkout's current
    /// HEAD does not contain.
    pub unmerged_commits: usize,
    pub branch_tip: Option<String>,
    /// Why this worktree could not be examined completely, when that happened.
    pub problem: Option<String>,
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

fn changed_files(folder: &Path) -> Result<usize, String> {
    let out = checked(
        folder,
        &[
            "status".as_ref(),
            "--porcelain".as_ref(),
            "--ignore-submodules=all".as_ref(),
        ],
    )?;
    Ok(out
        .split(|&b| b == b'\n')
        .filter(|line| !line.is_empty())
        .count())
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
            unmerged_commits: 0,
            branch_tip: None,
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
            match changed_files(&report.path) {
                Ok(count) => report.changed_files = count,
                Err(error) => problems.push(format!("cannot read the folder's status: {error}")),
            }
            match head_of(&report.path) {
                Ok(head) => tips.push(head),
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
        let mut discarded = Vec::new();
        if force {
            if report.changed_files > 0 {
                discarded.push(format!(
                    "{} in {}",
                    plural(report.changed_files, "uncommitted changed file"),
                    report.path.display()
                ));
            }
            if report.unmerged_commits > 0 {
                discarded.push(format!(
                    "{} not in the source checkout's HEAD{}",
                    plural(report.unmerged_commits, "commit"),
                    report
                        .branch_tip
                        .as_ref()
                        .map(|tip| format!(" (branch {} was at {tip})", report.branch))
                        .unwrap_or_default()
                ));
            }
            reasons.clear();
        } else if !reasons.is_empty() {
            return entry(report, CleanupAction::Kept, reasons, Vec::new());
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
        } else {
            Self::prune_if_registered(repo, &report.path)?;
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
        assert_eq!(forced[1].discarded.len(), 1);
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
}
