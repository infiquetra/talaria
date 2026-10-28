# Workflow runner

The runner owns workflow execution independently of the terminal view. A run has one folder,
one journal writer, and one control protocol shared by the command line and progress view.
Closing a viewer leaves the runner and its agents working.

In the terminal, `/workflows` opens the same discovery rows used by `workflow list`.
Use the arrow keys and Enter to open a run. Escape closes only the viewer; reopening
the picker refreshes discovery. With no runs it shows
`no runs yet — talaria workflow run starts one`.

```sh
talaria workflow run review.js --profile fixture --args '{"branch":"main"}'
talaria workflow run --detach review.js --profile fixture
talaria workflow list --json
talaria workflow resume <run-id>
talaria workflow stop <run-id>
talaria workflow resolve <run-id> --key <key> --attempt 0 --episode 0 --resolution fail
```

`run`, `resume`, `list`, `stop`, and `resolve` work without a terminal on standard input.
Use `--help` on a verb for its flags. The runner performs no gateway administration and never
changes gateway configuration.

## Locations and files

Runs use `--runs-dir`, then non-blank `TALARIA_RUNS_DIR`, then `runs/` under the configuration
folder. The configuration folder is non-blank `TALARIA_CONFIG_DIR`, otherwise `~/.talaria`.
Saved workflow names use `--workflows-dir`, otherwise the configuration folder's `workflows/`.
Neither location helper creates a directory. Starting a run creates a private runs directory;
listing a missing one prints `No workflow runs found.` or `[]`.

Each run folder holds:

| File | Purpose |
|---|---|
| `manifest.json` | Immutable run identity, inputs, timestamp, seed, profile, policy, and roster snapshot. |
| `journal.jsonl`, `run.lock` | Durable records and the journal's single-writer lock. |
| `scripts/`, `blobs/`, `resolutions/` | Journal-owned content and parked-call answers. |
| `launch.json` | Original script path and folder, absolute working folder, and saved-workflow folder. |
| `outcome.json` | Latest completed execution's result, failure detail, or abort reason. |
| `runner.json` | Detached child's process id; discovery does not trust it as liveness evidence. |
| `runner.log` | Detached child's appended human output. |
| `control/` | Shared pause, stop, and next-resume restart requests. |

Runner-owned JSON files are written through a synced temporary file and atomic replacement.
The journal remains the authority for execution status. A stale process-id file does not make
an interrupted run look active.

## Starting and detaching

`run` accepts `--args <json>` or `--args-file <path>`, `--profile`, `--staffing`, `--cwd`,
`--budget-tokens`, `--budget-usd`, `--on-unknown wait|fail`, `--record`, the location flags,
`--json`, and `--detach`. Arguments are validated before the script; the script's literal
metadata is parsed before staffing or gateway work. Token budgets must be positive integers;
money budgets must be finite positive amounts in US dollars.

Preflight loads staffing, applies command-line budget overrides, fetches the roster, reports
all tier-check problems and warnings, and computes the cost estimate. A missing staffing file
means an empty table. No run folder exists until preflight succeeds. Unpriced work has unknown
cost, rather than a zero estimate.

The manifest records the parsed workflow name, actual script hash, version, profile, input,
roster, and the finite-only approval-policy intent. The deterministic seed hashes the original
timestamp and script hash. Resume reuses the stored timestamp and seed. Approval behavior
remains the backend's responsibility; a finite turn can still use the operator's configured
approval bypasses.

An agent that omits `toolsets` receives no tools. Name the required toolsets in the script
to grant them; an explicit list takes precedence over the runner's empty default.

`--detach` creates the folder after preflight, then starts the current executable in its own
process group with null standard input and output appended to `runner.log`. The parent prints
the run id and returns. The child reads the stored script, rather than reopening the original
script path, takes the journal lock, rebuilds its routes, and executes. The hidden child-entry
flags are internal lifecycle plumbing and are excluded from help.
With `--record`, the parent flushes its preflight segment before the child appends through
the same recorder implementation. The resulting log keeps one header and increasing counters.

## Resume and parked calls

`resume` accepts a run id or folder, an optional replacement `--script`, staffing and budget
overrides, `--cwd`, location flags, `--record`, and `--json`. A locked run refuses a second
runner. Use `resolve` to answer its parked calls while it remains alive.

A completed execution replays its stored `outcome.json` result without connecting to the
gateway or executing the script. If a completed run predates that file, the runner reports
the missing result and the journal's finish summary; it never reruns paid work to recover a
display value. Failed, aborted, and interrupted runs open a new execution. Their unchanged
completed calls remain reusable through journal replay.
Before admitting new work, resume reattaches the unfinished sessions recorded in the
journal. Their terminal results still come from receipts through the backend.
An explicit pending restart of a failed key requests a new execution even when the previous
script handled that failure and returned normally. Requests naming only completed or skipped
keys leave the completed replay path unchanged.

With `--script`, the replacement source is stored by hash. A later bare resume reads the hash
from the latest `run_resumed` record. Missing stored source is a refusal. Resume keeps the
original `launch.json` unchanged; a working-folder override applies only to that execution.
Staffing drift compares the stored snapshot with the current staffing table and prints the
upstream before/after descriptions verbatim.

An unknown admission parks by default. The notice contains the exact command to answer its
key, attempt, and episode:

```sh
talaria workflow resolve <run-id> --key <key> --attempt <n> --episode <n> --resolution fail
```

`fail` returns null to the script, `rerun` permits one new attempt, and `abort` ends the
execution. The live runner consumes the answer; no second runner is launched. A duplicate
answer is refused. `--on-unknown fail` at initial launch selects the journal's automatic
failure policy; it does not itself fail the entire workflow. Resume keeps that stored policy.

## Shared controls and confirmed stopping

The progress view and CLI use the same durable writer. Requests are published without
overwriting an existing request:

```json
{"v":1,"kind":"pause","at_ms":1792627200000}
{"v":1,"kind":"stop","at_ms":1792627200000}
{"v":1,"kind":"restart","key":"0123456789abcdef0123456789abcdef","at_ms":1792627200000}
```

The corresponding names are `pause.json`, `stop.json`, and `restart-<key>.json` under
`control/`. The runner polls every 250 ms. A pause gates fresh admissions without consuming
a scheduler permit; already admitted work and reattachments continue. Removing `pause.json`
resumes fresh admissions. Restart files remain pending until the next resume; completed and
skipped calls remain kept. No second stop file or gateway-wide pause operation exists.

`workflow stop` writes `stop.json` and waits for confirmation. The runner cancels the engine,
consumes the request, stops each targeted session once, then polls terminal receipts within
the 30-second settlement bound. It journals an aborted finish and atomically writes
`outcome.json` with reason `stopped` only after all targets settle. An uncertain target gives
`stopped-unconfirmed`. Consuming a request or accepting an interrupt alone is not proof of
settlement.

The waiting command succeeds only after the file is consumed, the journal records an aborted
finish, and a fresh outcome file confirms `stopped`. Its default timeout is 35 seconds;
`--timeout 0` polls once. Timeout or an unconfirmed outcome prints
`stop requested; run <id> is still finishing — stop not confirmed` and exits 1. A request
left for a dead runner stays pending until a later resume discards controls from the old
execution.

The first Ctrl+C or SIGTERM takes the graceful stop path. A second signal exits immediately
without writing a terminal outcome, leaving the execution interrupted and resumable. An
operator abort through `resolve` records `operator-abort`.

## Listing and output

`list` reads only files and briefly probes existing locks without creating them. It sorts
newest first, breaks timestamp ties by run id, and applies `--limit` in both output modes.
It folds executions: a new `run_resumed` clears a previous terminal status. Otherwise the
precedence is terminal outcome, unresolved parked calls, held writer lock, then interrupted.
Folders with run artifacts but corrupt or missing files produce one `unreadable` row with
an explicit warning; unrelated folders are ignored. Unreadable top-level directories fail
instead of appearing to be empty histories.

The JSON array always includes `run_id`, `name`, `profile`, `started_at_ms`, `status`,
`agents_finished`, `agents_total_known`, `parked`, `tokens_spent`, `money_spent_usd`, `run_dir`,
and `warning` (including null). Token spend comes from settled usage. Money sums each
attempt's last settled session cost; any missing or unknown settled cost makes the total
unknown. File-only discovery never reprices tokens.

Foreground output names the run folder, shows phases, logs, agents, and scheduler events,
then prints the result. `--json` emits compact JSON lines and one final `run_finished`
object. That object has exactly one of `result`, `error`, or `reason`, together with token
and money spend. Diagnostic and refusal lines go to standard error. Detached children
always write human output to `runner.log`; the parent prints one detached-run object or line.

Exit codes are 0 completed, 1 failed or unconfirmed, 2 usage or refusal, and 3 aborted.

## Integration and verification

This implementation is pre-staged against the journal and progress interfaces. The engine,
journal, Hermes binding, staffing scheduler, and progress view retain their separate ownership.
No upstream implementation is copied into the runner. The shared wire factory must provide
the live and recorded construction used by both the roster and retained stop port; the
scripted CLI path must use the Hermes module's existing scripted wire.
On resume the runner passes pending keys to `RunJournal::request_restart_keys` before
building replay configuration. Only after the journal accepts the requests are those files
removed. Failed and interrupted attempts advance; completed and skipped calls stay kept.
Running admissions must be stopped and confirmed settled first. A submit intent whose
admission was never recorded requires recovery before a restart can be granted.

The progress-view change merges first; the runner merges after it. Final acceptance still
requires the independent runner tests, the two enabled discovery picker tests, the real
pause/resume and confirmed-stop scenarios, next-resume restart with frame counts, detached
viewer close/reopen, and unanswered-control behavior. An offline harness cannot stand in
for those live checks.

Run formatting, Clippy, unit tests, and the specified integration commands after the upstream
modules are available. Local coverage is intentionally omitted; GitHub Actions runs it.
