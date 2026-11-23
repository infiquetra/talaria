# Workflow commands

Use the rendered root prefix on every command. Do not put credentials in arguments.

## Roster

```sh
@@TALARIA_COMMAND@@ workflow roster --profile @@TALARIA_PROFILE@@ --json
```

Options: --staffing, --budget-tokens, --budget-usd, --profile, --json. The version 1 envelope contains report, paths, workflows, skill, warnings, and error. Exit 0 means usable staffing, 2 means report.check problems, 1 means report is null and error explains why.

## Run

```sh
@@TALARIA_COMMAND@@ workflow run --profile @@TALARIA_PROFILE@@ --json <path>
```

Options: --detach, --json, --args, --args-file, --profile, --staffing, --cwd, --budget-tokens, --budget-usd, --on-unknown, --record, --runs-dir, --workflows-dir. JSON input flags conflict. The first line names the run folder. The final foreground object has event run_finished, run_id, outcome, one of result/error/reason, and spend with tokens, money_usd, money_source. Outcome budget_exhausted means the script returned but the budget refused or stopped agents; that object has result, reason and summary with refused and stopped counts. Outcome completed_with_failures means the script returned but at least one agent failed; that object has reason, then failed_agents (label and reason for each), then cut_short_agents when any, and only then result. Any outcome that has cut short agents (Hermes stopped them at their step cap; their replies are kept) lists them in cut_short_agents before result. Any outcome can also carry two more lists, printed after those and before result (and omitted when empty): untracked_work, plain sentences naming an agent, its session and working folder when Talaria found work on that agent's route that the run was not following (Hermes had moved the conversation to a new session and started a turn there), what it cancelled, and whether something is still running and may still be changing files there; and leftover_edits, one entry per agent worktree that still holds uncommitted changes when the run ends (path and changed_files). Treat any untracked_work entry that says something is still running as a reason not to integrate that agent's branch until the folder has been looked at. Nothing is deleted. The same untracked-work sentences also appear live as log events (message starting "runner: untracked work: "); a check that could not be made appears only as a log event starting "runner: route not checked: ". An agent that ran out of context is started again in a fresh session at most three times, only when its route is quiet; each restart shows as a scheduler event agent_restarted and the agent's row says "restarted N of 3 after a context overflow". Exit 0 completed or completed with failures, 1 failed, 2 refused, 3 aborted, 4 budget exhausted. Detach returns a run id immediately and sends no completion notice; the child appends runner.log. --cwd sets the folder that unisolated agents run in and that worktrees are cut from; outside a git repository the command warns that isolated agents will fail there. The foreground --json stream also prints agent_started and agent_finished events.

## Resume

```sh
@@TALARIA_COMMAND@@ workflow resume --json <run-id>
```

Options: --script, --staffing, --cwd, --budget-tokens, --budget-usd, --record, --json, --runs-dir, --workflows-dir. A completed or completed_with_failures run prints its stored result, with its failed and cut short agent lists, without doing paid work again. Other runs open a new execution and reuse kept calls. A live runner holds its lock and refuses another resume. Exit codes match run.

## Resolve

```sh
@@TALARIA_COMMAND@@ workflow resolve <run-id> --key <key> --attempt <n> --episode <n> --resolution fail
```

The needs operator line prints the exact resolve command. Choose fail, rerun, or abort only as the developer directs. This answers the same live runner, without starting another. --runs-dir selects its location. Exit 0 written, 1 duplicate/error, 2 usage.

## List

```sh
@@TALARIA_COMMAND@@ workflow list --json --limit 20
```

Options: --runs-dir, --json, --limit. Status is running, parked, completed, completed_with_failures, budget_exhausted, failed, aborted, interrupted, or unreadable. Rows include run_id, status, agents_finished, agents_total_known, parked, tokens_spent and money_spent_usd, but no result. For a running run the agent counts show progress; for finished runs tokens_spent and the counts are the basis for a cost estimate. Missing history is an empty array. Exit 0 listed, 1 filesystem failure, 2 usage.

## Stop

```sh
@@TALARIA_COMMAND@@ workflow stop --timeout 35 <run-id>
```

Options: --runs-dir, --timeout. Exit 0 means already finished or confirmed stopped. Exit 1 says stop not confirmed; requesting an interrupt alone does not prove settlement. Exit 2 means usage or unknown run.

## Skill status

```sh
@@TALARIA_COMMAND@@ workflow skill status --profile @@TALARIA_PROFILE@@ --json
```

Options: --profile, --json. Shows current, outdated, modified, foreign, or missing, versions and differences. Exit 0 inspected, 1 I/O or mismatch, 2 invalid input or missing profile. Installation is an explicit developer action. It never happens merely by enabling hivecode.
