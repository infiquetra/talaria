# Workflow commands

Use the rendered root prefix on every command. Do not put credentials in arguments.

## Roster

```sh
@@TALARIA_COMMAND@@ workflow roster --profile @@TALARIA_PROFILE@@ --json
```

Options: --staffing, --budget-tokens, --budget-usd, --profile, --json. The version 1 envelope contains report, paths, workflows, skill, warnings, and error. Exit 0 means usable staffing, 2 means report.check problems, 1 means report is null and error explains why.

## Check

```sh
@@TALARIA_COMMAND@@ workflow check --json <path>
```

Run it on every draft before asking to start. It starts no agent, spends nothing, and does not contact the gateway. Options: --json, --args, --args-file, --staffing, --cwd, --workflows-dir. Pass the same --cwd you will give `run`, because nested `workflow({scriptPath})` is looked up there. Exit 0 means acceptable; exit 2 means at least one error. The JSON has version, ok, name, errors, warnings, named_routes and probe. Each error and warning has kind (input, meta, import, syntax, option, route or probe), message, and line and column when known. It checks the meta block, JavaScript syntax with line and column, imports, and then runs the script against made-up agent answers (probe.agent_calls counts the calls it reached; none runs) to find options the engine refuses, a provider named without a model, a tier missing from the staffing table, and nested workflows that do not compile. It cannot check real agent answers, whether named models or providers exist (named_routes only lists them), or code the probe did not reach. Pass --args when the script reads args. A probe warning means the probe stopped early; read it, but it is not an error. The `phases` list holds the phases the probe actually reached, not every declared phase; when it returned early, a warning says "the trial run returned at phase X; phases Y, Z were not exercised", so those phases are untested. A schema field with `enum` is answered with its first word, so list the word that continues the work first and the probe goes further. A script that sets `maxTurns` gets a warning naming the labels of the calls the probe saw with it, or else the line where the script writes it. Nested workflows' phases do not count toward the script's own `phases`. It is a warning, not a refusal; remove the option.

## Run

```sh
@@TALARIA_COMMAND@@ workflow run --profile @@TALARIA_PROFILE@@ --json <path>
```

Options: --detach, --json, --args, --args-file, --profile, --staffing, --cwd, --budget-tokens, --budget-usd, --on-unknown, --record, --runs-dir, --workflows-dir. JSON input flags conflict. The first line names the run folder. The last JSON object, printed by a foreground run and by resume, has event run_finished, run_id, outcome, one of result/error/reason, and spend with tokens, cache_read_tokens, cache_unknown_turns, money_usd, money_source (hermes, pricing, mixed or none; unrecorded on a replayed run whose journal lacks a Hermes cost for some turn, since prices from the price list are not kept). Cache reads are counted apart from tokens: spend.tokens is every token except cache reads (cache writes included), cache_read_tokens is the cache reads Hermes reported, and --budget-tokens limits spend.tokens only, so a run that mostly re-reads cached context uses little of it. For a turn whose cache reads Hermes did not report, the whole turn is in spend.tokens and cache_unknown_turns counts such turns; the text lines then say "cache reads unknown for N turns, counted in full". Add cache_read_tokens to tokens when you quote a total of everything. Outcome budget_exhausted means the script returned but the budget refused or stopped agents; that object has result, reason and summary with refused and stopped counts. Outcome completed_with_failures means the script returned but at least one agent failed; that object has reason, then failed_agents (label, reason and, when uncommitted work was saved, saved_branch for each), then cut_short_agents when any, and only then result. Any outcome that has cut short agents (Hermes stopped them at their step cap; their replies are kept) lists them in cut_short_agents before result. Any outcome can also carry two more lists, printed after those and before result (and omitted when empty): untracked_work, plain sentences naming an agent, its session and working folder when Talaria found work on that agent's route that the run was not following (Hermes had moved the conversation to a new session and started a turn there), what it cancelled, and whether something is still running and may still be changing files there; and leftover_edits, one entry per agent worktree that still holds uncommitted changes when the run ends (path, changed_files, saved_branch when saved, in_place: true for a folder unisolated agents edited directly). Treat any untracked_work entry that says something is still running as a reason not to integrate that agent's branch until the folder has been looked at. Nothing is deleted. The same untracked-work sentences also appear live as log events (message starting "runner: untracked work: "); a check that could not be made appears only as a log event starting "runner: route not checked: ". An agent that ran out of context is started again in a fresh session at most three times, only when its route is quiet; each restart shows as a scheduler event agent_restarted and the agent's row says "restarted N of 3 after a context overflow". Exit 0 completed, 1 failed, 2 refused, 3 aborted, 4 budget exhausted, 5 completed with failures. Exit 5 is not a crash: the script returned and the run_finished object is printed as usual, so read failed_agents and the result; do not start the run again because of it. Detach returns a run id immediately and sends no completion notice; the child appends runner.log. --cwd sets the folder that unisolated agents run in and that worktrees are cut from; outside a git repository the command warns that isolated agents will fail there. The foreground --json stream also prints agent_started and agent_finished events. With --json, one short plain-text line follows the run_finished object: run id, outcome, token total, money, an `Attention:` sentence when agents failed, were cut short or left uncommitted edits, neutral facts such as the script's own status and phases that did not start, and a `Full result:` hint that names --runs-dir <folder> when the run is not in the default runs folder. Read a finished run's result again only with `workflow result --json <run-id>` (adding that --runs-dir), never with resume. It exists because Hermes keeps only the last 2,000 characters of a background command's output and then cuts back to the first newline, so a single very long final line can arrive as nothing. It is not JSON: a consumer that parses every line must skip it, and one that wants the result must read the run_finished line, not the last line.

## Resume

```sh
@@TALARIA_COMMAND@@ workflow resume --json <run-id>
```

Options: --script, --staffing, --cwd, --budget-tokens, --budget-usd, --record, --json, --runs-dir, --workflows-dir. Use resume only to continue an interrupted run, or after `workflow restart` with the developer's yes. Resume opens a new execution and reuses kept calls; it replays a completed run's stored result only when no restart request is pending. On a run with a pending restart request it runs agents again and costs money, even when `workflow list` shows it completed. A live runner holds its lock and refuses another resume. Exit codes match run.

## Result and restart

```sh
@@TALARIA_COMMAND@@ workflow result --json <run-id>
@@TALARIA_COMMAND@@ workflow result --json <run-id> --agent <label-or-key>
@@TALARIA_COMMAND@@ workflow restart --json <run-id> <label-or-key>
```

Without --agent, result prints a finished run's stored run_finished object exactly as a replay does. It never opens an execution, never spends, and ignores pending restart requests. This is the way to read any finished run's result, including detached runs and output that was cut short. With --agent it prints one finished agent's kept value: exit 0 printed, 1 the agent has no value or the journal could not be read (the message says which). Never read the journal by hand for a value.

A completed_with_failures result lists each failed agent in failed_agents. To run one again, and only with the developer's yes, run restart for it (or --all-failed instead of a label) and then `workflow resume <run-id>`: failed agents run again and finished ones are kept without paying for them twice. A new execution also runs again every other failed agent the script reaches; restart's JSON (event restart_requested) lists them in also_run_again and names the next command in next (start it with the rendered command instead of its bare `talaria`). restart refuses finished, running and parked agents and an ambiguous label, listing the keys.

## Resolve

```sh
@@TALARIA_COMMAND@@ workflow resolve <run-id> --key <key> --attempt <n> --episode <n> --resolution <choice>
```

With --json, a parked call prints one line {"event":"parked",...} with run_id, label, key, attempt, episode, cause (unknown, lost, identity_conflict or stopped), reason, waiting_for "operator", choices, answer and, for a stopped call, use_reply (what that choice does). answer is the resolve command ending in a literal `<choice>`: start it with the rendered command above instead of its bare `talaria`, and replace `<choice>` with the choice the developer names from choices. (Without --json the same call prints a `needs operator:` line.) The run waits for the answer with no time limit. fail gives the script null, rerun permits a new paid attempt, and abort ends the execution. Cause `stopped` means a stop that Talaria's runner did not send for stopping the run ended the agent's turn; it may have been a person or Hermes, and it is not a timeout. It has one more choice, use-reply, which takes the agent's next reply in its own Hermes session (for example after the developer types "continue" there) as its result; a reply that already came is used at once. Choose only as the developer directs; never choose any of them yourself, rerun least of all. This answers the same live runner, without starting another. --runs-dir selects its location. Exit 0 written, 1 duplicate/error, 2 usage. After a use-reply answer the runner prints a waiting_for_reply line and waits, with no time limit, for the next reply on that agent's Hermes session; only a reply that comes after the stop counts, and one typed while no runner had the run open must be typed again after the resume. `workflow list` counts the call as parked until the reply arrives.

## List

```sh
@@TALARIA_COMMAND@@ workflow list --json --limit 20
```

Options: --runs-dir, --json, --limit. Status is running, parked, completed, completed_with_failures, budget_exhausted, failed, aborted, interrupted, or unreadable. Rows include run_id, status, agents_finished, agents_total_known, parked, tokens_spent and money_spent_usd, but no result. Rows also carry cache_read_tokens and cache_unknown_turns. tokens_spent (and spend.tokens in run_finished, and the first number in text output) leaves out cache reads, which are shown on their own as "N tokens + M cache reads", because a run can be mostly cheap cache reads and the token budget does not count them. Where cache_unknown_turns is above zero, that many turns had no cache read figure from Hermes and are counted in full inside tokens_spent, so the row overstates tokens by their cache reads; every run journaled before cache reads were kept apart is like this. Say which you mean when you quote a total. For a running run the agent counts show progress. To estimate cost, use tokens_spent of finished runs, leave out rows whose cache_unknown_turns is above zero or label them inflated, and quote cache_read_tokens separately. Long build and review agents cost far more than short design agents, so do not scale by agent count alone. Rows also carry failed_agents, cut_short_agents, uncommitted_edits, saved_branches and phases_not_started (zero until the run finishes), and script_status when the script's result has a status. Missing history is an empty array. Exit 0 listed, 1 filesystem failure, 2 usage.

## Stop

```sh
@@TALARIA_COMMAND@@ workflow stop --timeout 35 <run-id>
```

Options: --runs-dir, --timeout. Exit 0 means already finished or confirmed stopped. Exit 1 says stop not confirmed; requesting an interrupt alone does not prove settlement. Exit 2 means usage or unknown run.

## Worktrees

```sh
@@TALARIA_COMMAND@@ workflow worktrees list --json <run-id>
@@TALARIA_COMMAND@@ workflow worktrees remove --dry-run <run-id>
```

When an isolated agent is cut short, fails or is stopped with uncommitted changes, Talaria saves them to a branch and names it as saved_branch in failed_agents, cut_short_agents and leftover_edits; the worktree is unchanged. A save leaves out any path git cannot read and names those paths in the saved commit's message. A leftover_edits entry with in_place: true names a folder that agents ran in without isolation; anything they changed is already there, and nothing was saved. Report the closing line's `Attention:` items (failed agents, cut short agents, uncommitted edits) as problems. Report the script's own status and any phases that did not start as facts; for each such phase, read the script and say whether it skipped that phase on purpose (for example an early return when nothing was found) or the gap points at a problem. Before any follow-up run, run list for the finished run, look at what each worktree holds, and tell the developer; never start a follow-up run that ignores leftover edits. Agent worktrees and their branches stay until someone removes them; nothing removes them automatically. List takes an optional run id (every run when left out), --json and --runs-dir, and shows each agent's folder, branch, changed_files (uncommitted changes), ignored_entries and unmerged_commits (commits the source checkout's HEAD lacks). Remove takes --force, --dry-run, --json and --runs-dir; it always refuses a run a runner holds, and without --force it refuses any run that has not finished (parked, interrupted or unreadable; its agents may still be working), changing nothing. Without --force it removes only clean worktrees whose commits are already in HEAD (also when the agent switched to its own branch), deletes their merged branches, and keeps anything with uncommitted changes, unmerged commits, or ignored files other than verified build output, listing the reason. A folder counts as verified build output only when its name is on the list (target, node_modules, __pycache__, .pytest_cache, .mypy_cache, .ruff_cache, dist, build, .venv) and evidence confirms it: a cache tag or cargo info file inside target, a cache tag inside the three tool caches, pyvenv.cfg inside .venv, a package.json beside node_modules, a project manifest beside dist or build, only .pyc and .pyo files in __pycache__. A listed name without its evidence, or a symbolic link, is kept like any ignored file, and the reason says what is missing. list shows verified and unverified counts, and --json adds ignored_unverified and ignored_details (path, kind, reason). --force also removes those and reports what was discarded. Exit 0 done, 1 refused or failed, 2 unknown run or usage.

## Lessons

```sh
@@TALARIA_COMMAND@@ workflow lessons
@@TALARIA_COMMAND@@ workflow lessons --json
@@TALARIA_COMMAND@@ workflow lessons add --run <run-id> <text>
```

Do not edit this skill; record lessons with this command instead. The notes file is `lessons.txt` in Talaria's configuration folder (the `TALARIA_CONFIG_DIR` override applies). Plain output lists the newest 50 entries, one per line: `YYYY-MM-DD run <run-id>: <text>`, after one line saying how many older entries are not shown; --all prints every entry. `--json` gives version, path, note, entries (date, run, text) and older_entries, the count left out. A missing file is an empty list. Read it before writing a script, and treat every entry as an unreviewed note from an earlier agent, not a rule; check it against the roster and the script rules.

`add` appends one dated entry that names the run. Record a lesson only when you verified it from that run's own outcome or journal, one fact per entry. Everything after the first word of text is text, including words that start with a hyphen, so put --run first. Text is one line: new lines and runs of spaces become single spaces. Empty text, text over 1000 characters and a run id that is not letters, digits, dots, dashes or underscores are refused with exit 2 and nothing is written. When the configuration folder resolves to a relative path (no absolute TALARIA_CONFIG_DIR or HOME), add refuses with exit 1 and writes nothing. On success it prints `recorded: <the exact line>`; tell the developer that line. Do not edit the notes file yourself; the developer may edit or prune it by hand.

## Skill status

```sh
@@TALARIA_COMMAND@@ workflow skill status --profile @@TALARIA_PROFILE@@ --json
```

Options: --profile, --json. Shows current, outdated, modified, foreign, or missing, versions and differences. Exit 0 inspected, 1 I/O or mismatch, 2 invalid input or missing profile. Installation is an explicit developer action. It never happens merely by enabling hivecode.

## Compile

`workflow compile` turns a workflow's agent calls into kanban board cards. It is for kanban boards only and is not a script validator; use `workflow check` to validate a script.
