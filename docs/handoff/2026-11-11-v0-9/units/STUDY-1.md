# STUDY-1: Study: a viewer for the plans Hermes saves

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: the issue lists the questions. This file is a map of Talaria names that are easy to confuse with a saved Hermes plan. It does not answer the questions and it states no recommendation.

## Names that are not a saved plan

`Plan` in `crates/talaria-core/src/context/usage.rs:55` is the subscription plan Hermes reports for `/usage`. Its fields are `name`, `status`, `spendable`, `renews`, `plan_bar`, and `topup_bar`. `plan_rows` in `crates/talaria-tui/src/context_view.rs:612` draws that plan. A viewer of saved plans does not start from either of these.

## The sentence to check

`PLAN_MODE_NOTE` in `crates/talaria-core/src/automation/present.rs:26-28` says plan mode should only write a plan under `.hermes/plans/` in the session folder, that nothing stops a tool call, and that approvals still apply. Question 1 checks all three claims against `agent/plan_prompt.py` at the tag in `crates/hermes-contract/PINNED`. On the revision this map was written against, that tag is v0.22.4. `SHELL_MODE_TITLE` in `crates/talaria-tui/src/side.rs:23` says a shell draft runs in the session folder on the gateway host. That title is not the plan directory.

## What this file does not decide

It does not say whether a client can read a saved plan through the gateway, what a remote gateway does, what a viewer would show, or whether to build it. Questions 2 and 3 mean the gateway Talaria attaches to. Its sources are under `tui_gateway/` at the pinned tag. `crates/hermes-client/tests/live_config_surface.rs` cites files in that tree.
