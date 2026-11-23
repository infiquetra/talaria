---
name: talaria-workflows
description: "Run substantive tasks as Talaria workflows: a short JavaScript script fans the work out to many Hermes agents, each on its own model and provider, checks their results and can resume after a crash."
version: 1.0.0
metadata:
  hermes:
    tags: [Workflows, Orchestration, Multi-Agent, Talaria]
  talaria:
    skill_version: 1
    profile: "@@TALARIA_PROFILE@@"
---

Use a Talaria workflow for substantive changes across files, reviews, research, or debugging several hypotheses. For trivial requests, answer directly and say that no workflow was needed.

First obtain the available staffing and paths:

```sh
@@TALARIA_COMMAND@@ workflow roster --profile @@TALARIA_PROFILE@@ --json
```

Use the command exactly as rendered, including HERMES_HOME=. Exit 2 means a tier problem: report `report.check` and stop. Exit 1 means no usable roster: report `error.message` and stop. Show the developer every entry in `warnings`. Use only usable tiers or a model and provider listed in `report.providers`; never invent a model name.

When the request ends with a line `Session model: <model> on <provider>`, that is the model the user picked for this session. Add `--session-model <model> --session-provider <provider>` to the roster command above and to `workflow run`, so `report.current` and every agent call that names no tier or model use that pair.

Prefer a built-in listed in `workflows`: hivecode, review-branch, research, or fix-failing-tests. Run its listed `path`. Otherwise read references/script-api.md and references/examples.md with skill_view, then write a short uniquely named script in `paths.drafts_dir`, named `<name>.workflow`. Hermes lints `.js` files with `node --check`, which rejects a workflow script's top-level `return`; `.workflow` avoids that, and a lint error on a draft is not a script error. Before asking to start, run `@@TALARIA_COMMAND@@ workflow check <path>`; exit 2 lists each problem with its line. Read references/commands.md for command details.

Never write the deliverable in this session: no project files, scaffold, stubs or commits before the run, and do not build a first version yourself. This session plans, writes the script, and asks. Anything the workers must share, such as interfaces, a skeleton or a build setup, is work for the workflow's first phase. The one exception is a new project folder outside a git repository; see "New project" below.

Before every run, run `workflow list --json` (full command below) and base the cost estimate on recent runs' `tokens_spent` and agent counts. Give a range, not one number, and label anything else a guess. Show the phases, number of agents, tiers or models, arguments, and cost estimate. Then ask whether to start. Start only after an explicit yes for this run. A yes for an earlier run does not count. Pass the developer's budget using --budget-usd or --budget-tokens.

Run a foreground workflow in a background terminal with completion notification:

```text
terminal(command="@@TALARIA_COMMAND@@ workflow run --profile @@TALARIA_PROFILE@@ --json <path>", background=true, notify=true)
process_manage(action="wait", session_id="<returned process session_id>", timeout=180)
```

Keep the returned process session_id; it differs from the conversation id. Hermes caps a wait at 180 seconds. Wait at most once, post one progress line, then end your turn; do not wait again, the completion notice resumes you. For progress, `workflow list --json` shows `agents_finished` and `agents_total_known`, and the process output has `agent_started` and `agent_finished` events. If the output after a background completion is empty or cut short, find the run's status with `workflow list --json`. When it is completed, completed_with_failures or budget_exhausted, `workflow resume --json <run-id>` (full command below) prints the stored result without doing paid work again. For any other status, report it and ask the developer: resume would run the unfinished agents again. Never launch a second run for the same task. For work that must outlive this session, add --detach and keep the printed run id; a detached run sends no completion notice.

The last JSON line is {"event":"run_finished",...}, then one short text line. Read its `run_id`, `outcome`, `result`, `spend` and, when present, `failed_agents`, `cut_short_agents`, `untracked_work` and `leftover_edits` (references/commands.md); report those before the result. Report the run id, findings or changes, and spend. For a detached run, poll `workflow list --json` the same way: find its run_id until status is completed, completed_with_failures or budget_exhausted. Then retrieve its saved result without executing work again:

```sh
@@TALARIA_COMMAND@@ workflow resume --json <run-id>
```

Report failed, aborted, budget_exhausted, completed_with_failures, or parked status and ask the developer how to proceed. A budget_exhausted run continues only with a larger --budget-usd or --budget-tokens that the developer approves for it. Continue an interrupted run with:

```sh
@@TALARIA_COMMAND@@ workflow resume <run-id>
```

A `needs operator` line means a parked call. Show the exact `workflow resolve` command it prints. Explain that fail gives the script null, rerun permits a new paid attempt, and abort ends the execution. Wait for the developer's chosen resolution; never choose rerun yourself. Execute only that choice. Never replace an interrupted run with a new run: finished agents could be paid for twice.

To request and confirm stopping:

```sh
@@TALARIA_COMMAND@@ workflow stop <run-id>
```

Use only these toolsets: ["file"] for code reading, ["file","terminal"] for editing and tests, ["web"] for web research, or ["todo"] for planning and synthesis. The file toolset can write files. Every agent with file or terminal must set isolation: 'worktree' (not the new-project bootstrap). Tell readers explicitly not to edit. A worktree is a separate checkout, not a sandbox. Workers commit on their own branches; report their commits for integration.

Check every agent result. A failed or skipped agent gives null: retry a failed reviewer or verifier, or name it in the final result; never drop it silently (for example by filtering out nulls). Start independent phases together, such as a playtest that does not need the reviewers' output.

Worktrees: a new worktree starts at the source checkout's committed HEAD when that agent starts; uncommitted files are not copied. Tell each isolated worker to work in its own folder with relative paths, create named branches there with `git switch -c` (branches are shared across worktrees, so the integrator merges them by name), and never `cd` or `git -C` into the source checkout or switch branches there. Never paste the source checkout's absolute path into a prompt; workers must not commit absolute checkout paths.

New project: worktree isolation fails outside a git repository. When the target folder is not inside one, create the empty folder, run with `--cwd <that folder>`, and make the script's first agent unisolated (no `isolation`). It does only `git init` and an initial commit (`--allow-empty` without a scaffold), or the shared scaffold. Every later agent uses isolation: 'worktree'.

Never put secrets, keys, or tokens in --args or prompts. Workflow agents cannot answer approval requests. Under default Hermes settings these are refused without asking: `rm` on any absolute path (even inside the worktree), any recursive `rm`, and `sh -c` or `bash -c`. Put this in every terminal worker's prompt: keep temporary files under a relative path in your worktree, delete with a non-recursive `rm` on a relative path or skip cleanup, and run commands directly. Never ask the developer to change approval settings for a workflow.
