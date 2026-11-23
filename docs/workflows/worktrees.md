# Workflow worktrees

`GitWorktrees` supplies one local Git worktree per workflow call. The runner owns one
provider per run. Agents can then edit in parallel; only worktree setup and explicit
management operations share a mutex. Git 2.31 or newer is required.

The provider never fetches, stashes, copies the checkout or removes a worktree at the
end of an agent, run or runner process. Retained work is available for review and resume.
A worktree is a starting directory, not a filesystem sandbox: an agent's file and shell
tools can still address paths outside it.

## Placement and identity

For a run folder `runs/<run-id>` and a call with a 64-character lowercase hexadecimal
digest and occurrence `0`, the folder is
`runs/<run-id>/worktrees/<full-digest>-0`. Its branch is
`talaria/wf/<run-id>/<full-digest>-0`. No label, prompt, time or shortened digest names a
folder. Invalid digests are refused before creation.

The source folder can be a subdirectory or another worktree of the source repository.
Discovery caches that repository's root on first use. Each new worktree starts at the
repository's committed `HEAD`, captured as an object id before creation. A later new
call can use a newer `HEAD`; an existing call keeps its folder and recorded base.
Dirty source checkouts are allowed. Uncommitted edits, untracked files and ignored
files (including environment files and build output) are not copied. The sidecar
records whether the source repository was dirty.

All paths use a canonical existing prefix. This keeps a symlinked run folder and its
resolved spelling identical for creation, listing, reuse and removal, including macOS
temporary-folder aliases.

## Durable ownership and recovery

Each worktree has an adjacent `<call>.json` sidecar with exactly `call_digest`,
`occurrence`, `branch`, `base_commit`, `repo_root` and `base_dirty`. A provisional
sidecar is written before fresh Git creation. Publication uses a create-new temporary
file, file sync, atomic rename and directory sync. An existing temporary file is left
untouched. Failed creation removes its provisional sidecar only when no folder exists.

An existing folder is reused only if its absolute Git common directory matches the
source repository, its checked-out branch matches the expected branch, and its
Git worktree root is exactly that folder. A plain subdirectory cannot inherit the
parent repository's identity to pass these checks. Its
sidecar identity and repository must also match. Reuse preserves edits and the sidecar.
A mismatched folder or sidecar fails closed; it never falls back to the source folder.

A folder with the correct repository and branch but no sidecar can recover provenance
on its next provider call. Recovery must publish a listable, removable sidecar before
returning success. Until that explicit call, `list()` hides sidecar-less worktrees and
`remove()` refuses them; neither adopts a folder from Git's inventory.

If the folder vanished but its branch survives, the provider reattaches that branch
and preserves its recorded base. It runs `git worktree prune` only after Git's
machine-readable inventory confirms an entry for that missing folder. Modern Git
uses NUL delimiters; older Git uses its line-based porcelain format. The latter
fails closed for a newline in the target path because that format is ambiguous. Prune is repository
wide: other administrative entries for already-deleted folders can also disappear.
It does not delete a live folder or branch. If both folder and branch are missing but
provenance remains, the call is reported lost; no fresh tree conceals the data loss.
Moving a run's data folder is unsupported because stored launches use absolute paths.

## Saving work an agent left uncommitted

When a call that ran with `isolation: 'worktree'` ends badly (Hermes cut it short at its step
cap, it failed, or it was stopped) and its worktree holds uncommitted changes, the runner saves
them to a new branch before it records how the call ended. Tracked changes, staged or not, and
new files are saved; files the repository ignores are not. The worktree is not disturbed: its
branch, its index and its files stay exactly as they were, and nothing is stashed. The snapshot
is built in a temporary index file under the worktree's own Git folder (`GIT_INDEX_FILE`, then
`git add --all`, `git write-tree`, `git commit-tree` with the worktree's `HEAD` as parent and
`git update-ref` that refuses to overwrite an existing branch), and the temporary index is
deleted afterwards.

The branch is the worktree's own branch with `-saved` added:
`talaria/wf/<run-id>/<full-digest>-0-saved`. If that name already holds exactly the same work on
the same `HEAD`, it is named again and nothing new is made; if it holds other work, the next
free name (`-saved-2`, `-saved-3` and so on) is used. The commit takes the repository's
configured identity. When none is configured, it is written as `Talaria workflow
<talaria-workflow@localhost.invalid>` through the environment of that one command; the
repository's configuration is never changed. No hook runs (`core.hooksPath` points nowhere for
these commands) and no signing program starts.

A path Git cannot read does not stop the save. A file the user may not open, a folder that
cannot be listed, or a nested repository with no commit is left out, and everything else is
still saved. The saved commit's message lists the paths that were left out, under "Git could not
read these paths", and the journal note that names the branch says how many there were and names
the first few. Those paths are still in the worktree. When nothing else changed, no branch is
made and the note still names them. The save fails only when Git cannot write the snapshot at
all.

The save does not run filter programs that the repository's own configuration names. An agent
can write `filter.<name>.clean` or `filter.<name>.process` into the repository's configuration
(or a file it includes) and point `.gitattributes` at it, so for every such filter the save's
commands set the clean and process programs to nothing and `required` to false, through
`GIT_CONFIG_COUNT` for those commands only. A file such a filter manages is saved as it is in the
folder. Filters from the user's global or system configuration, such as Git LFS, still run: they
are the user's own programs, and without them every file they manage would be saved in its raw
form. The read-only `git status` checks (the leftover-edits list, `worktrees list`) do not set
these overrides, so Git can run a repository's clean filter there on a file whose recorded
details changed, as it would for any `git status` the user runs.

The branch name is carried everywhere the call's end is reported: in the journal's
`call_finished` record (as `saved_branch` in the `cut_short` mark of a call cut short, or at the
end of the reason of a failed or stopped call, as "(uncommitted work saved to branch ...)"), in
the mark the script reads (`savedBranch` in the hidden `cutShort` property and in the entries of
`cutShort()`), in the run's `failed_agents` and `cut_short_agents` entries (`saved_branch`), and
in a journal note that is also printed live. A call that completed in full is not saved.

When the run ends, every agent worktree that still holds uncommitted changes is listed in
`leftover_edits`. For an agent that ended badly, including one that never finished because the
run was aborted, the save is made then if it was not made already, and the entry's
`saved_branch` names the branch. The worktree itself keeps its changes either way.

Saving never fails the call or the run. When it cannot be done (the folder is gone, Git
refuses, the folder is not one of the run's worktrees), the journal and the run's output say
why, and the run carries on. Only a folder inside the run's own `worktrees` folder is saved.

An agent without worktree isolation, including every agent of a run whose folder is not a Git
repository, runs in the run's folder itself. Anything it changed is already where the run's
caller looks, and nothing is saved for it. When such an agent ends badly, `leftover_edits` lists
that folder once with `"in_place": true` and the agents' labels, and the text output says they
ran there without worktree isolation, that anything they changed is already there, and that
nothing was saved. Talaria does not check whether they changed anything; no Git command runs in
that folder.

## Listing, diffing and explicit removal

`list()` returns sidecar-backed `WorktreeInfo` rows sorted by full call name, including
the canonical path, branch, recorded base and live dirtiness. Missing folders remain
visible with `dirty: false`. Malformed or mismatched sidecars are skipped. A status
error on an existing folder counts as dirty.

`diff(path)` returns bytes containing a binary-capable Git patch against the recorded
base, including agent commits, staged changes, unstaged changes and non-ignored new
files. Paths are supplied as separate arguments; untracked filenames are read with
NUL delimiters. No shell expands them. The patch can be reviewed or applied elsewhere
with `git apply --binary`. It neither commits nor merges work. Ignored files are not
included, and a foreign path is refused.

`remove(path, false)` refuses uncommitted changes. Clean removal deletes the folder
and sidecar but leaves the branch for review. `remove(path, true)` explicitly permits
Git's forced removal, deletes the recorded branch, then deletes the sidecar last.
If a folder is already missing, removal prunes only after confirming its entry,
keeps the branch unless forced, and removes the sidecar. A Git or branch-deletion
failure leaves provenance for recovery. Only a sidecar-backed canonical path can be
removed; even a Git-known worktree from the same repository is refused without it.

## Cleaning up from the command line

Nothing removes a run's worktrees or branches automatically. Two commands do it on request:

```bash
talaria workflow worktrees list [<run-id>] [--json]
talaria workflow worktrees remove <run-id> [--force] [--dry-run] [--json]
```

Both take `--runs-dir`. The run's own sidecar records name the worktrees and `launch.json`
names the source checkout; labels come from the journal when it gives them. `list` covers one
run, or every run in the runs directory when no id is given, and shows each agent worktree's
run id, agent label, folder, branch, whether the folder still exists, how many files `git
status` reports as changed, and how many commits on the branch (or checked out in the folder)
the source checkout's current `HEAD` does not contain. It only reads, so it works on a running
run.

`remove` refuses a run whose runner holds `run.lock`, even with `--force`, and it holds that
lock itself while it works. It also looks at the run's status, derived the way `workflow
list` derives it. Without `--force` it proceeds only when the run reached a recorded end
(completed, completed_with_failures, budget_exhausted, failed or aborted). A running, parked,
interrupted or unreadable run is refused with nothing changed, because its agents may still
be working at Hermes in those folders, or the run may be resumed. With `--force` a parked or
interrupted run is cleaned anyway and the output says the run did not finish; `--dry-run`
follows the same rules. Without `--force` it removes a worktree, its branch and its sidecar only when the folder
is clean and everything it holds is already contained in the source checkout's `HEAD`; the
branch goes with `git branch -d`, which refuses unmerged work a second time. A folder that holds ignored
files is also kept, because removal deletes them, unless every ignored entry is verified
build output: a folder with one of the names `target`, `node_modules`, `__pycache__`,
`.pytest_cache`, `.mypy_cache`, `.ruff_cache`, `dist`, `build` or `.venv` whose contents
or neighbours confirm what it is. A name alone is not enough, because a hand-made folder
can be called `build`. The evidence is: `target` holds `CACHEDIR.TAG` or `.rustc_info.json`
(Cargo writes both); `.pytest_cache`, `.mypy_cache` and `.ruff_cache` hold `CACHEDIR.TAG`;
`.venv` holds `pyvenv.cfg`; `node_modules` has a `package.json` in the folder that holds
it; `dist` and `build` have a project manifest beside them (`package.json`,
`pyproject.toml`, `setup.py`, `setup.cfg`, `Cargo.toml`, `CMakeLists.txt`, `build.gradle`,
`build.gradle.kts`, `pom.xml`, `Makefile` or `meson.build`); `__pycache__` holds only
`.pyc` and `.pyo` files. A symbolic link with one of these names is never build output,
and the checks never follow a link or read file contents. Verified folders are deleted with
the worktree and the output says so. A listed name that fails its check is an ordinary
ignored entry: it keeps the worktree, and the reason names the folder and what is missing,
for example "build/ was not accepted as build output: it has no project manifest beside it".
For `dist` and `build` the manifest shows only that the folder sits in a project that builds;
Talaria does not look inside, so hand-made files in an ignored `dist` or `build` beside a
manifest are deleted with it.
`list` shows the ignored count, how many are verified build output and how many are not;
`--json` adds `ignored_unverified` and an `ignored_details` list whose entries carry a
`path`, a `kind` (`build_output`, `unverified_build_output` or `other`) and, for an
unverified entry, the `reason`. Every worktree
with uncommitted changes or unmerged commits is kept and listed with the reason.

An agent may switch its folder to a branch of its own (`git switch -c`) or to no branch. Such
a folder is judged by what it has checked out: when it is clean by the same check and its
checked-out commit is contained in the source checkout's `HEAD`, the folder is removed. Each
branch is then judged on its own. The run's recorded branch is deleted with `git branch -d`
only when `HEAD` contains it; otherwise it is kept with its sidecar, so it is still listed, and
the output says how many commits it holds. The agent's branch is deleted the same way, and only
when the oldest line of its reflog records its creation (its old object id is all zeros) at a
time no earlier than the run's own branch was made. In every other case the branch is kept and
the output says why: its reflog shows it was created before the run; its reflog no longer holds
its creation, because older entries expired (`git gc` and `git reflog expire` drop them), so the
branch may have existed long before; or Talaria can read no reflog entry for it (a repository
using the reftable format keeps no reflog files Talaria reads). A branch that existed before the
run is never deleted. Nothing unmerged is ever force-deleted without `--force`.
`list --json` shows the agent's branch as `folder_branch`, and `remove --json` names the branches
in `deleted_branches` and `kept_branches`. The closing sentence of `remove` names only the
reasons the kept worktrees actually have (uncommitted changes, commits the source checkout
lacks, ignored files that are not verified build output, a folder that could not be examined,
or a folder or branch Talaria does not own), and mentions `--force` only when it would help.

With `--force` it removes everything for
that run and prints what was discarded: the changed-file count, the ignored entries, the
unmerged commit count, and always the branch tip (and the folder's own commit when it is
not the branch tip), which `git branch <name> <tip>` can restore until Git collects it. If
the folder could not be examined it says "could not tell what is lost" with the reason. `--dry-run` prints the same decisions and changes nothing.

When a folder is already gone, only that worktree's own registration is dropped (with
`git worktree remove <path>`); other stale registrations in the source repository are left
alone, and `git worktree prune` finishes any that remain.

Only a folder that resolves to the run's own `worktrees` directory and a branch under
`talaria/wf/<run-id>/` are ever touched. A folder that is a link leading elsewhere is kept
without any git command running in it, and a `worktrees` directory that is itself a link is
refused. Git runs through the same hardened runner as the provider (`core.fsmonitor` and
`core.untrackedCache` off, no optional locks).

## Offline demonstration

```bash
cargo run -p talaria-workflow --features mock --example worktree_agent -- --help
cargo run -p talaria-workflow --features mock --example worktree_agent -- \
  --source "$SOURCE" --run-dir "$RUN" --profile fixture
```

The example resolves one isolated request, runs it through the real Hermes port
with a scripted wire, and prints exactly the `session.create` frame's `cwd`.
It needs no gateway. The source defaults to the current directory and the profile
to `fixture`. Without `--run-dir`, it creates a retained temporary run folder.
Repeating the same source and run folder reuses the call's worktree, branch and
edits. The scripted reply completes a call without editing files; live file-location
proof remains separate.

## Runner integration

A new run stores its canonical source `cwd` in `launch.json`. Each execution reads
that original stored value and constructs one shared provider with the run folder
and run id, installing it in `LaunchDefaults.worktrees`. A resume `--cwd` override
affects ordinary sessions for that execution, while isolated calls still discover
the original launch repository. Detached child execution uses the same construction.

The independent resolver and runner tests exercise the actual binding and runner,
including resume from another repository. The ignored live test checks file edits
through a real gateway; offline checks do not replace that acceptance proof.
