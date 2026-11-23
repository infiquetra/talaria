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
files is also kept, because removal deletes them, unless every ignored entry is a folder
named like regenerable build output (`target`, `node_modules`, `__pycache__`,
`.pytest_cache`, `.mypy_cache`, `.ruff_cache`, `dist`, `build`, `.venv`); those are deleted
with the folder and the output says so. `list` shows the ignored count and how many of them
are build output. Every worktree
with uncommitted changes or unmerged commits is kept and listed with the reason, and so is a
folder that no longer sits on its recorded branch. With `--force` it removes everything for
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
