# Workflow commands

Use the rendered root prefix on every command. Do not put credentials in arguments.

## Roster

```sh
@@TALARIA_COMMAND@@ workflow roster --profile @@TALARIA_PROFILE@@ --json
```

Options: --staffing, --budget-tokens, --budget-usd, --profile, --json. The version 1 envelope contains report, paths, workflows, skill, warnings, and error. Exit 0 means usable staffing, 2 means report.check problems, 1 means report is null and error explains why.

## Run

```sh
@@TALARIA_COMMAND@@ workflow run --profile @@TALARIA_PROFILE@@ --json <script.js>
```

Options: --detach, --json, --args, --args-file, --profile, --staffing, --cwd, --budget-tokens, --budget-usd, --on-unknown, --record, --runs-dir, --workflows-dir. JSON input flags conflict. The first line names the run folder. The final foreground object has event run_finished, run_id, outcome, one of result/error/reason, and spend with tokens, money_usd, money_source. Exit 0 completed, 1 failed, 2 refused, 3 aborted. Detach returns a run id immediately; the child appends runner.log.

## Resume

```sh
@@TALARIA_COMMAND@@ workflow resume --json <run-id>
```

Options: --script, --staffing, --cwd, --budget-tokens, --budget-usd, --record, --json, --runs-dir, --workflows-dir. A completed run prints its stored result without doing paid work again. Other runs open a new execution and reuse kept calls. A live runner holds its lock and refuses another resume. Exit codes match run.

## Resolve

```sh
@@TALARIA_COMMAND@@ workflow resolve <run-id> --key <key> --attempt <n> --episode <n> --resolution fail
```

The needs operator line prints the exact resolve command. Choose fail, rerun, or abort only as the developer directs. This answers the same live runner, without starting another. --runs-dir selects its location. Exit 0 written, 1 duplicate/error, 2 usage.

## List

```sh
@@TALARIA_COMMAND@@ workflow list --json --limit 20
```

Options: --runs-dir, --json, --limit. Status is running, parked, completed, failed, aborted, interrupted, or unreadable. Rows include counts and observed spend, but no result. Missing history is an empty array. Exit 0 listed, 1 filesystem failure, 2 usage.

## Stop

```sh
@@TALARIA_COMMAND@@ workflow stop --timeout 35 <run-id>
```

Options: --runs-dir, --timeout. Exit 0 means already finished or confirmed stopped. Exit 1 says stop not confirmed; requesting an interrupt alone does not prove settlement. Exit 2 means usage or unknown run.

## Skill status

```sh
@@TALARIA_COMMAND@@ workflow skill status --profile @@TALARIA_PROFILE@@ --json
```

Options: --profile, --json. Shows current, outdated, modified, foreign, or missing, versions and differences. Exit 0 inspected, 1 I/O or mismatch, 2 invalid input or missing profile. Installation is an explicit developer action. It never happens merely by enabling ultracode.
