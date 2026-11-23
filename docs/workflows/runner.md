# Workflow runner

The runner owns workflow execution independently of the terminal view. A run has one folder,
one journal writer, and one control protocol shared by the command line and progress view.
Closing a viewer leaves the runner and its agents working.

In the terminal, `/workflows` opens the same discovery rows used by `workflow list`.
Use the arrow keys and Enter to open a run. Escape closes only the viewer; reopening
the picker refreshes discovery. With no runs it shows
`no runs yet — talaria workflow run starts one`, then says that with Hivecode on a plain
prompt goes to the talaria-workflows skill, and the agent proposes a workflow and asks before
starting it. Alt+W (`workflow_view`) opens, from the composer, the latest run that this
Talaria window saw start after a Hivecode-routed prompt in the foreground session, and the
run list otherwise.

Enter on an agent in the run view opens that agent's session in the window. The line above the
composer then names the key that goes back (Left by default, `workflow_agent_back`): with an
empty draft and nothing else open, it returns to the session you were in and opens the run view
again with that agent selected; Esc then closes the view as usual. Going back sends nothing to the
agent, and its turn keeps running. The way back is forgotten once you reach a different session
by any other route, after you use it, and when the session you came from cannot be opened. It is
not offered when you came from a session that has no stored id of its own yet.

```sh
talaria workflow run review.js --profile fixture --args '{"branch":"main"}'
talaria workflow run --detach review.js --profile fixture
talaria workflow list --json
talaria workflow resume <run-id>
talaria workflow stop <run-id>
talaria workflow resolve <run-id> --key <key> --attempt 0 --episode 0 --resolution fail
talaria workflow restart <run-id> review
talaria workflow result <run-id> --agent review --json
talaria workflow result <run-id> --json
```

`run`, `resume`, `list`, `stop`, `resolve`, `restart`, and `result` work without a terminal on
standard input.
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
| `outcome.json` | Latest completed execution's result, failure detail, or abort reason; a `budget_exhausted` run keeps its result and the budget reason; a `completed_with_failures` run keeps its result and a `failed_agents` list, and any run with agents Hermes stopped at their step cap lists them in `cut_short_agents`. Both lists hold `label` and `reason` and come right after `outcome`. |
| `runner.json` | Detached child's process id; discovery does not trust it as liveness evidence. |
| `runner.log` | Detached child's appended human output. |
| `control/` | Shared pause, stop, and next-resume restart requests, and the notes the workflow view and the runner leave before they stop an agent's turn. |

The manifest also records `talaria_commit`, the short commit of the Talaria build that wrote the
run, and `talaria_tree` (`clean` or `modified`), both `unknown` for a build made without git and
in older manifests; `approval_policy.source` describes the finite-only policy in plain words.

Runner-owned JSON files are written through a synced temporary file and atomic replacement.
The journal remains the authority for execution status. A stale process-id file does not make
an interrupted run look active.

## Starting and detaching

`run` accepts `--args <json>` or `--args-file <path>`, `--profile`, `--session-model` with
`--session-provider`, `--staffing`, `--cwd`, `--budget-tokens`, `--budget-usd`,
`--on-unknown wait|fail`, `--record`, the location flags, `--json`, and `--detach`. The session
pair replaces the profile's configured model as the roster's current model, which a call
naming no model or tier runs on; the manifest's roster snapshot records it with
`"source":"session"`, and resume and a detached child keep it. Arguments are validated
before the script; the script's literal metadata is parsed before staffing or gateway work. Token budgets must be positive integers;
money budgets must be finite positive amounts in US dollars.

Preflight loads staffing, applies command-line budget overrides, fetches the roster, reports
all tier-check problems and warnings, and computes the cost estimate. A missing staffing file
means an empty table. No run folder exists until preflight succeeds. Unpriced work has unknown
cost, rather than a zero estimate.

Preflight also reads each `agent()` call whose options object, written in the call itself,
names both `model` and `provider` as plain strings, and checks that pair against the roster.
A pair the roster cannot serve fails the run with exit 2 before any agent starts, one line per
unknown pair with its script line and the nearest known spelling. Anything preflight cannot
read for certain is checked when its agent starts instead: a call naming only a model, a name
that is computed, options built elsewhere or changed by a later assignment, an object with a
spread or computed key, and names beside a `profile` key. No script that would run is refused. When an agent's call settles, the runner leaves its Hermes session with
`session.detach`, naming the subscription the create answer gave (or, after the connection
came back, the one the resume gave) and the end reason `workflow_agent_done`, so Hermes ends
the session once no other viewer holds it. The runner then stops following the session, so a
later reconnect does not resume it and open it again. The session socket serves no
`session.close`. A detach the gateway refuses or does not answer within 15
seconds is noted as one journal log line and changes nothing else; Hermes then ends the
session itself, after its orphan grace period, once the runner's connection closes.

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
runner. Use `resolve` to answer its parked calls while it remains alive. A resume keeps the
token and money budget recorded in the manifest's roster snapshot; `--budget-tokens` or
`--budget-usd` replaces only the one it names.

A completed execution, including one that completed with failures, replays its stored
`outcome.json` result and its failed and cut short agent lists without connecting to the
gateway or executing the script, and so does a budget-exhausted run resumed without a budget
flag (exit 4, with a line saying a larger budget continues it). Neither changes any file in
the run folder. If a completed run predates that file, the runner reports
the missing result and the journal's finish summary; it never reruns paid work to recover a
display value. Failed, aborted and interrupted runs, and budget-exhausted runs given a new
budget, open a new execution. A pending restart request also opens a new, paid execution of a
finished run, so to read a finished run's result use `talaria workflow result <run-id>`, which
never opens one (see below).
Their unchanged completed calls remain reusable through journal replay.
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

A parked call waits until it is answered. Nothing times the wait out, and the runner adds no
limit of its own.

### Stopped agents

Hermes ends a turn as `interrupted` (or a queued one as `cancelled`) when someone asks it to
stop, and Hermes can also interrupt a turn itself. The reason Talaria gives says that the turn
was stopped, who sent the stop when Talaria knows,
Hermes's exit reason, and the first line of Hermes's final text, and it says that this was not
a timeout: Hermes's text such as `waiting for model response (80.0s elapsed)` gives how long
the turn had run, not a limit. For example:

```text
agent stopped: Talaria's runner did not send this stop; a person or Hermes did, for example with a key press in a window showing the agent's session, or Hermes itself when it interrupts a turn; Hermes ended the turn as 'interrupted' (interrupted_during_api_call): Operation interrupted: waiting for model response (80.0s elapsed). This was a stop, not a timeout: a time in Hermes's text is how long the turn had run
```

Talaria records who sent a stop it knows about in the settled turn's `result_meta.stopped_by`:
`run_stop` when the runner stopped the turn because the run was being stopped (`workflow stop`,
the stop-run key, or a signal), `budget` when the runner stopped it because the run's budget
ran out, `workflow_view` for the workflow view's stop-agent key, and `workflow_view_restart`
when the view stopped a running agent to restart it. The runner knows this even when the turn
settles only after it went away, for example when a run stop could not confirm the turn's
settlement within its bound, when a second Ctrl+C ended the runner, or when the runner crashed
after a budget stop: before it sends a stop it leaves a note (see
[Shared controls](#shared-controls-and-confirmed-stopping)), and the execution that later reads
the settlement takes the source from it. Talaria cannot tell a person's stop from one Hermes
made itself, for example when the gateway restarts, so a stop from anywhere else, such as a key
press in a Talaria window that shows the agent's session, leaves the key out and the reason says
that Talaria's runner did not send it and that a person or Hermes did.

A stopped agent is not failed at once. Unless the stop was part of the runner stopping the run
(a run stop, a budget stop, which ends the call as `stopped` in the execution that sent it, or
the view's restart), the call parks as needing the operator, with cause `stopped`, through the same parked-call mechanism as
an unknown admission. A stop from the view's stop-agent key parks too. The answers are `fail`,
`rerun` and `abort` as above, and `use-reply`:

```sh
talaria workflow resolve <run-id> --key <key> --attempt <n> --episode <n> --resolution use-reply
```

`use-reply` takes the next turn that settles on the agent's same Hermes session after the stop
as the call's result. Continue the session, for example by typing `continue` into it from a
Talaria window, before or after answering: a reply that already settled after the stop is used
at once, and otherwise the runner waits for one, with no time limit. Only a turn that follows
the stop counts. When the call parks, the runner records in its `call_attention` record, as
`seen_admissions`, the other turns it had already seen on the session, for example the answer
to a message a person typed while the agent was still working; none of those is taken, however
the turns were ordered. The adopted reply goes through the same checks as the agent's own
reply, including the schema check and its one correction turn. If that turn is stopped too, the
call parks again with the next episode. `use-reply` is refused for a call parked for any other
cause. A run started with `--on-unknown fail` answers every parked call with `fail` at once, a
stopped agent included, and the reason keeps the stop's reason.

When the runner starts waiting for that reply it says so: text mode prints a
`waiting for a reply: run <id> agent <label> key <key> waits for the next reply on Hermes session <id>`
line followed by how to give the reply, and `--json` prints one line:

```json
{"event":"waiting_for_reply","run_id":"…","label":"review","key":"…","session_id":"…","waiting_for":"operator","how":"continue the agent's Hermes session …"}
```

While it waits, `workflow list` still counts the call in `parked` and shows the run as
`parked`: it waits for someone to give the reply. The count drops once the reply settles the
turn.

The runner sees the other window's turn through the session's events, whose envelopes name the
admission that produced them, and through the session's snapshot, which lists the turns still
queued or running when the runner attaches; Hermes serves the receipt of any admission on the
session to the same owner. A runner follows a stopped parked call's session from the moment
the call parks, and a resumed runner attaches to it again as soon as it parks the call again,
so a reply typed while the call waits for its answer is seen. A turn that started and settled
while no runner had the run open cannot be seen, because Hermes lists only queued and running
turns: continue the session again after the resume. If the runner cannot attach to the
session, it says so in a log line.

When the parked agent ran in its own worktree and left uncommitted changes there, the runner
saves them to a branch as it parks the call, with the same save the end of a failed call uses
(see [Worktrees](worktrees.md)), because `rerun` starts the next attempt in that same worktree.
It saves again before a `rerun` starts the next attempt, in case the folder changed meanwhile;
the same work keeps the same branch. The parked reason ends with
`(uncommitted work saved to branch <branch>)`, the `parked` line names it in `saved_branch`,
and text mode adds an `its uncommitted work was saved to branch <branch>` line.

In `--json` mode the runner prints one line when a call parks:

```json
{"event":"parked","run_id":"…","label":"review","key":"…","attempt":0,"episode":0,"cause":"stopped","reason":"agent stopped: …","waiting_for":"operator","choices":["fail","rerun","abort","use-reply"],"answer":"talaria workflow resolve … --resolution <choice>","use_reply":"continue the agent's Hermes session …"}
```

`answer` names `--runs-dir` when the run is not in the default runs folder; `use_reply` appears
only when `use-reply` is one of the choices, and `saved_branch` only when the parked attempt's
uncommitted work was saved. Text mode prints the `needs operator` line with the
same command, then the cause and reason, a line saying the run waits for the answer, and what
`use-reply` does. When the agent's session is open in a Talaria window, the line above the
composer calls the agent `stopped, waiting for you`, and the first message sent there says that
its reply counts as the agent's result only once the parked call is answered with `use-reply`.

### Running a failed agent again

`talaria workflow restart <run-id> <label-or-key>` writes the same `restart-<key>.json` request
the workflow view's restart key writes. It accepts exactly the agents the resume that reads the
request can restart: one that failed or that the budget stopped, one the run's own stop left
unfinished (its turn settled stopped, or the stop could not confirm the settlement), one
waiting for a reply after a `use-reply` answer, and one parked for the operator, which the
resume then answers with `rerun`. `--all-failed` asks for all of these except parked agents,
which wait for their own answer, and `--json` prints one `restart_requested` object with
`requested` (each `label`, `key`, and `written`, false when the request was already there),
`also_run_again`, and `next`. Then run `talaria workflow resume <run-id>`, as `next` says: the
pending request opens a new execution even for a run that completed with failures, the failed
agent runs again, and finished agents are replayed from the journal without another gateway
call. A finished (completed or skipped) agent is refused, because it is kept and running it
again would pay for its work twice. An agent whose turn was sent but not yet confirmed by
Hermes is refused too: resume the run first so the runner can recover that turn. If an
agent's turn is still running in Hermes when the resume that reads its restart starts, the
resume refuses to start and says so; once that turn has settled, resume again. A label more than one agent shares is refused
with their keys. A new execution runs every failed agent the script reaches again, not only the
ones asked for; `also_run_again` lists the other failed agents so this is not a surprise. Exit
0 written, 1 unreadable journal, 2 refusal or unknown run or agent.

`talaria workflow result <run-id> --agent <label-or-key>` prints the value one agent's call
returned to the script, read from the journal: the pretty JSON value in text mode, or with
`--json` one `agent_result` object with `run_id`, `label`, `key`, `state`, and `value`, or
`reason` when the agent did not complete, and `cut_short` for a reply kept at Hermes's step
cap. It exits 0 when a value was printed and 1 when the agent has none (failed, skipped,
stopped, parked, or unfinished). A `completed_with_failures` result already lists each failed
agent's `label` and `reason` in `failed_agents`, also from `workflow result --json`.

`talaria workflow result <run-id>` without `--agent` prints the run's own stored result, read
from the run folder only: exactly what a resume of the finished run prints when it replays it,
the `run_started` line and the same `run_finished` object with `--json` and the same text
otherwise, with the same exit codes (0 completed, 5 completed with failures, 4 budget
exhausted). A run that failed or was aborted prints the end its runner printed, with exit 1 or
3. It never opens an execution or contacts the gateway, and it leaves a pending restart request
for the next resume, so use it, not `resume`, to read a result. A run that has not finished
(running, parked, interrupted) is reported on standard error, naming its status, with exit 1.

## Shared controls and confirmed stopping

The progress view and CLI use the same durable writer. Requests are published without
overwriting an existing request:

```json
{"v":1,"kind":"pause","at_ms":1792627200000}
{"v":1,"kind":"stop","at_ms":1792627200000}
{"v":1,"kind":"restart","key":"0123456789abcdef0123456789abcdef","at_ms":1792627200000}
```

The corresponding names are `pause.json`, `stop.json`, and `restart-<key>.json` under
`control/`. Before the workflow view sends a stop to an agent, from the stop-agent key or as the
first step of restarting a running agent, it writes `agent-stop-<key>.json` (kind `agent_stop`
or `agent_restart`) and sends the stop once the note is written, or could not be. The runner
writes the same note, with kind `run_stop` or `budget_stop`, before it sends the stops of a run
stop or of a budget stop. Each note names the turn it is about by `attempt`, `turn` and, once
the turn was admitted, `admission_id`:

```json
{"v":1,"kind":"run_stop","key":"0123456789abcdef0123456789abcdef","at_ms":1792627200000,"attempt":0,"turn":0,"admission_id":"…"}
```

The runner reads the note when the stopped turn settles, in whichever execution reads that
settlement, records `stopped_by` from it, and removes it. A note that names another turn (an
earlier attempt, for example) is ignored, and so is one written before notes named their turn.
When a new execution starts, the runner removes every note whose turn is no longer running; a
note for a turn still waiting to settle is kept for that turn's settlement.

In the workflow view, the stop-agent key (`x`, `workflow_stop_agent`) asks first, like restart
and stop-run: the footer reads `stop agent <label>? x or y confirms · esc cancels`, and the
question lapses after three seconds. A subagent's stop is not asked about. Once the stopped turn
settles, the footer says the run waits for an answer and the agent shows as needing you. The runner polls every 250 ms. A pause gates fresh admissions without consuming
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
precedence is terminal outcome, unresolved parked calls (including a call answered with
`use-reply` that still waits for its reply), held writer lock, then interrupted.
Folders with run artifacts but corrupt or missing files produce one `unreadable` row with
an explicit warning; unrelated folders are ignored. Unreadable top-level directories fail
instead of appearing to be empty histories.

The JSON array always includes `run_id`, `name`, `profile`, `started_at_ms`, `status`,
`agents_finished`, `agents_total_known`, `parked`, `tokens_spent`, `cache_read_tokens`,
`cache_unknown_turns`, `money_spent_usd`, `run_dir`, and `warning` (including null). Token spend
comes from settled usage. `tokens_spent` leaves out cache reads, which `cache_read_tokens` gives
on their own; `cache_unknown_turns` counts settled turns whose cache reads Hermes did not report,
which are counted in full inside `tokens_spent` (every turn of a run journaled before cache
reads were kept apart). `cache_read_tokens` is null when no turn reported tokens. See
[Staffing and budgets](staffing.md#spend-and-budgets). Money sums each
attempt's last settled session cost; any missing or unknown settled cost makes the total
unknown. File-only discovery never reprices tokens.

Rows also carry `failed_agents`, `cut_short_agents`, `uncommitted_edits`, `saved_branches` and
`phases_not_started` (zero until the run finishes), `phases_not_started_names` when any phase did
not start, and `script_status` when the script's result has one; text rows add marks such as
`failed 2`, then `script status "reviewed"` and `phases not started: Verify, Synthesize`.

In `--json` mode the closing text line states, before `Full result:`, the script's own `status`
and the declared phases that did not start (up to three named, the rest counted), as information.
Then it adds an `Attention:` sentence when agents left uncommitted edits (and how many were saved
to a branch), were cut short or failed, most important last. The hint is `Full result: talaria
workflow result --json <run-id>`, with `--runs-dir` for a custom runs folder. `failed_agents`,
`cut_short_agents` and `leftover_edits` entries name `saved_branch` when an isolated agent's
uncommitted work was saved (see [Worktrees](worktrees.md)); a `leftover_edits` entry with
`in_place: true` names a folder agents ran in without isolation: anything they changed is already
there, and nothing was saved.

Foreground output names the run folder, shows phases, logs, agents, and scheduler events,
then prints the result. `--json` emits compact JSON lines and one final `run_finished`
object. That object has exactly one of `result`, `error`, or `reason`, together with token
and money spend (`spend.tokens` leaves out cache reads, with `spend.cache_read_tokens` and
`spend.cache_unknown_turns` beside it); a `budget_exhausted` object has both `result` and `reason`, plus the
journal's finish `summary` with its `refused` and `stopped` counts. A `completed_with_failures` object
(the script returned, but at least one agent failed) has `reason`, then `failed_agents`, then
`cut_short_agents` when any, then `result` and `summary`; a plain `completed` object lists
`cut_short_agents` before `result` when Hermes stopped any agent at its step cap. Foreground
text prints the same lists above the result. Agents the budget
refused or stopped appear as `agent_finished` lines with state `refused` or `stopped`, and an
agent stopped at its step cap with state `cut short`. Diagnostic and refusal lines go to standard error. Detached children
always write human output to `runner.log`; the parent prints one detached-run object or line.

Exit codes are 0 completed, 1 failed or unconfirmed, 2 usage or refusal, 3 aborted, 4 budget exhausted (the script
returned, but the budget refused or stopped at least one agent), and 5 completed with failures (the script returned
and its result is printed, but at least one agent failed).

## Lessons

`talaria workflow lessons` prints the newest 50 entries of the notes file `lessons.txt` in
Talaria's configuration folder (`TALARIA_CONFIG_DIR`, default `~/.talaria`), with one line saying
how many older entries exist; `--all` prints every entry. `talaria workflow lessons add --run
<run-id> <text>` appends one dated entry naming the run, and refuses when the configuration folder
resolves to a relative path. Agents do not edit the workflow skill; they record lessons with this
command instead. The entries are unreviewed notes, not rules, and the developer may edit or prune
the file by hand.

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
