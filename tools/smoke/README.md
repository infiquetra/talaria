# Live smoke test

Drives the built binary in a Herdr pane against a running gateway and checks five behaviours.

## Usage

Run `cargo run -q -p smoke -- run` from a Herdr pane. The options are:

| Option | What it does |
|---|---|
| `--talaria` | The binary under test; the environment variable `TALARIA_SMOKE_BIN` and then the workspace build are the fallbacks. |
| `--herdr` | The command line of the terminal multiplexer. |
| `--profile` | The profile the session runs in. |
| `--model` | The model the session uses. |
| `--provider` | The provider the model comes from. |
| `--deadline-secs` | The whole run, in seconds: 300 for `phase1` and 1800 for `phase2a` unless given. |
| `--step-scale` | The scale of the per-step limits. |
| `--report-dir` | Where the failure report is written. |
| `--suite` | `phase1` (the default) or `phase2a`, the suite for the phase 2a features. |
| `--tmux` | The tmux command line the `phase2a` suite pastes through; the default is `tmux`. |

`--step-scale` multiplies the per-step limits only: the shell wait, the startup wait, the reply
wait, the approval windows, the count wait, the settle wait, the quit wait and the resume wait.
It never multiplies the deadline, the quiet periods, the wait after Esc or the poll interval.
`--deadline-secs` is the whole run and is never scaled.

With `--suite phase2a` the preflight also checks that tmux runs and that the temporary folder
holds no space and no single quote, because its path is written into single-quoted scripts.

## Preconditions

A running gateway, the fixture profile and a built binary.

The approval step needs the profile to ask for approval itself; with the default mode the
gateway may approve the command on its own, and the step then skips with a plain message and
the run exits 3.

The `phase2a` suite also needs `tmux`, a model on the profile that uses its file editing tool
and its todo tool when asked, and a local terminal backend on the profile, so that the shell
commands run in the scratch project. Each run leaves one small `notes-<hex>.txt` copy in the
profile's attachments folder (the dropped file), the sessions it created, and its prompts in
Talaria's prompt history.

Run the `phase2a` suite with `TMPDIR` set to a short folder (on macOS the root-level temporary
folder, not the per-user one). Hermes cuts a long
path in a tool cell's subject, and with the default macOS temporary folder the edit cell no
longer names `notes.txt`. New Herdr tabs start from Herdr's own environment, not the caller's:
when the gateway lives in a Hermes home other than the default, pass `--talaria` a small
launcher script that sets `HERMES_HOME` and runs the binary as a child (not with `exec`, so the
foreground check still sees the launcher).

## Exit codes

0 passed, 1 failed, 2 preflight failed, 3 passed with a skip. In the `phase2a` suite only
`pane-state` (when `herdr pane get` shows no agent state) and `checklist` (when the model did
not use its todo tool in two attempts) can skip.

## Steps

Startup, plain answer, approval, interrupt, quit and resume.

The `phase2a` suite runs twenty-one steps: `startup` and `plain-answer` as above, with an
`EDITOR` setting on the launch line; `tool-shell` (a failed shell command's tool cell and output
row); `tool-edit` (a numbered diff, and the edit on disk); `palette` (`/vers` completes and runs
`/version`; a bare `/` lists the Talaria group); `mention` (`@calc` inserts `@file:src/calc.py`);
`shell` (an `!echo` cell, and the refused `!rm -r ./keepme`); `btw` (a side answer); `pager`
(the transcript pager follows, searches and closes); `history-search` (Ctrl+R loads the first
prompt unsent); `editor` (Ctrl+G returns the edited draft unsent); `theme` (the `/theme` report
and two usage lines); `pane-state` (Herdr reports the pane idle, working, then idle); `checklist`
(the task panel, Ctrl+T, and one record); `quit` and `resume` as above, with checks that no local
cell came back and that the resumed pager finds the first token; then, in a third pane running
Talaria inside tmux, `tmux-pane`, `paste-collapse` (a 120-line bracketed paste becomes a label),
`file-drop` (a pasted outside path becomes a file label), `paste-editor` (a paste label survives
Ctrl+G) and `tmux-quit`.

## Reading a failure

A failing step prints the screen of the pane it was in and writes the same text, with the
version of the multiplexer and the path of the binary, to the report file. The path of the
report is printed with the private parts replaced by placeholders.

A `phase2a` reason that names a screen text comes from a constant in `src/wording.rs`, each
with the issue it was copied from. When the pane text differs from that issue's text, either
Talaria drifted from its issue (a defect for that issue's owner) or the issue was edited (the
constant and its test change together); it is never a reason to relax the step.

A pane text that reports the gateway or profile unavailable makes the acceptance result blocked
by the environment, not failed. The script cannot tell the two apart: it reports a failure and
leaves the reading to the reader.

Every private path and name in the printed text, in the report and in the report path is
replaced by a placeholder: the scratch project, the temporary directory, the home directory, the
login name and the gateway home.
