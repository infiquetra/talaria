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

Use the command exactly as rendered, including HERMES_HOME=. Exit 2 means a tier problem: report `report.check` and stop. Exit 1 means no usable roster: report `error.message` and stop. Use only usable tiers or a model and provider listed in `report.providers`; never invent a model name.

Prefer a built-in listed in `workflows`: ultracode, review-branch, research, or fix-failing-tests. Run its listed `path`. Otherwise read references/script-api.md and references/examples.md with skill_view, then write a short uniquely named script in `paths.drafts_dir`. Read references/commands.md for command details.

Before every run, show the phases, number of agents, tiers or models, arguments, and cost estimate. Then ask whether to start. Start only after an explicit yes for this run. A yes for an earlier run does not count. Pass the developer's budget using --budget-usd or --budget-tokens.

Run a foreground workflow in a background terminal with completion notification:

```text
terminal(command="@@TALARIA_COMMAND@@ workflow run --profile @@TALARIA_PROFILE@@ --json <path>", background=true, notify=true)
process_manage(action="wait", session_id="<returned process session_id>", timeout=600)
```

Keep the returned process session_id; it differs from the conversation id. Repeat the wait while that process runs. Never launch a second run for the same task. For work that must outlive this session, add --detach and keep the printed run id.

The final foreground line is {"event":"run_finished",...}. Read its `run_id`, `outcome`, `result`, and `spend`. Report the run id, findings or changes, and spend. For a detached run, poll:

```sh
@@TALARIA_COMMAND@@ workflow list --json
```

Find its run_id until status is completed. Then retrieve its saved result without executing work again:

```sh
@@TALARIA_COMMAND@@ workflow resume --json <run-id>
```

Report failed, aborted, or parked status and ask the developer how to proceed. Continue an interrupted run with:

```sh
@@TALARIA_COMMAND@@ workflow resume <run-id>
```

A `needs operator` line means a parked call. Show the exact `workflow resolve` command it prints. Explain that fail gives the script null, rerun permits a new paid attempt, and abort ends the execution. Wait for the developer's chosen resolution; never choose rerun yourself. Execute only that choice. Never replace an interrupted run with a new run: finished agents could be paid for twice.

To request and confirm stopping:

```sh
@@TALARIA_COMMAND@@ workflow stop <run-id>
```

Use only these toolsets: ["file"] for code reading, ["file","terminal"] for editing and tests, ["web"] for web research, or ["todo"] for planning and synthesis. The file toolset can write files. Every agent with file or terminal must set isolation: 'worktree'. Tell readers explicitly not to edit. A worktree is a separate checkout, not a sandbox. Workers commit on their own branches; report their commits for integration.

Never put secrets, keys, or tokens in --args or prompts. Workflow agents cannot answer approval requests. Whether dangerous commands are denied depends on the profile's approval settings. Never ask the developer to change those settings for a workflow.
