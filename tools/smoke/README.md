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
| `--deadline-secs` | The whole run, in seconds: 300 for `phase1`, 1800 for `phase2a`, and 1500 for `phase2b` unless given. |
| `--step-scale` | The scale of the per-step limits. |
| `--report-dir` | Where the failure report is written. |
| `--suite` | `phase1` (the default), `phase2a`, or `phase2b`, the suite for session controls. |
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

## Session-control suite

Run `smoke run --suite phase2b` with the fixture profile, provider `ollama-cloud` and model
`minimax-m3.1-flash`. Other models and providers are refused during preflight because the
expected screen texts name these defaults. This suite needs no tmux. Help lists all three
suites; the unknown-suite error retains its earlier wording for compatibility.

The fixture profile must have `approvals.mode: manual` and at least two available Ollama Cloud
models. Its instructions must keep the rewind check's code words in conversation context,
without writing persistent memory, and answer those setup prompts with only `ok`. File
checkpoints are unnecessary because the check rewinds only the conversation. Use a short
temporary root and, for a gateway outside the default home, the launcher described above.

The fifteen steps run in this order: `startup`, `context-empty`, `permission`, `model-picker`,
`model-refusal`, `rewind`, `plain-answer`, `steer-idle`, `context`, `hermes-commands`,
`rename-export`, `queue`, `cd`, `quit`, and `headless`. Startup and plain-answer reuse the
original checks. The exit check uses the plain-answer token for both retained-output checks.
The headless section verifies the answer and empty error stream, then a clean no-gateway
failure with one JSON line and exit code 4.

The run creates two private scratch git folders and removes the entire scratch directory,
including exports and print outputs, after success, failure, panic, or interruption. File
contents and permissions are checked before cleanup. Terminal status and overlays are read
from the visible screen; notices and replies use unwrapped recent text. A failed step stops
the run and captures its pane. Only `steer-idle` and the queue's missing-indicator check retry,
once, after the screen settles. The repeat reads only its own prompt's subsequent lines.

Only `permission` may skip, when Herdr rejects both toggle-key names. A skip stops the run
with exit 3, or fails with exit 1 when `TALARIA_LIVE_REQUIRE=1`. A failed toggle check attempts
to restore this session's default. The suite never changes the profile approval setting.
Its exact screen wording and source citations live in `src/wording2b.rs`.

Set `CARGO_TARGET_DIR` to a private build directory before running the offline checks:

```sh
cargo test -p smoke
cargo fmt -p smoke --check
cargo clippy -p smoke --all-targets -- -D warnings
```

The ignored live tests run only when the live gateway is available for this suite.

## Phase 3 suite

Run `smoke run --suite phase3` with the fixture profile, provider `ollama-cloud` and model
`minimax-m3.1-flash`; other values are refused during preflight. The default deadline is 1200 s.
It needs no tmux. Every shell line must stay under 600 characters with the absolute binary path
and the scratch root, so a very long `TMPDIR` or binary path is refused before the run. The run
copies the caller's `HERMES_HOME` into the tab's shell, because a new Herdr tab does not inherit it.

Preconditions: the gateway serves the fixture profile, the fleet is quiet (no session that needs
you or is still working), the tab is at least 100 columns by 24 rows, and the profile has
`approvals.mode: manual`. Without manual approvals the `needs-you` step skips (exit 3).

| Step | What it checks |
|---|---|
| `fleet-quiet` | `talaria fleet list` works, the tab is big enough, nothing needs you or works |
| `bg-launch` | `--bg` prints one `launched` JSON line; `--bg -p` is refused with exit 2 |
| `worktree-launch` | `--bg --worktree` starts from the fetched `origin/main` and leaves the checkout clean |
| `fleet-list` | both sessions are listed; an unserved profile and an empty Hermes home are refused |
| `attach` | `talaria attach <8 hex>` resumes the session; an unknown reference exits 1 |
| `startup`, `plain-answer` | the first suite's checks |
| `local-commands` | `/tasks`, `/agents frobnicate`, `/processes` and `/bg` without a prompt |
| `fleet-view` | `/fleet` marks this session `(here)`, filters to nothing, closes with Ctrl+C |
| `open-in-pane` | `o` opens the first session in a new Herdr pane |
| `quit` | `/quit` exits 0 and the answer stays in the pane |
| `needs-you` | an approval that waited while Talaria was closed is announced, then denied from the fleet view |

The suite never answers an approval except by denying the one it provoked. If `needs-you` fails
while that request waits, the report says so; Hermes denies it after 300 s, and the next run's
`fleet-quiet` fails until then. The scratch root (two clones, a bare origin, the worktree, the
probe folder and the output files) is removed at the end unless `--report-dir` is given. The
sessions the suite started stay in the gateway. Exact screen wording lives in `src/wording3.rs`.

## Phase 4a suite

Run `smoke run --suite phase4a` to check the settings, providers, model roles, profiles,
and doctor screens, their input refusals, and the profiles and doctor command-line output.
The suite does not save configuration, submit a key, sign in, create a profile, or restart
the gateway. It needs `jq` on `PATH`, a gateway at the pinned Hermes release, and a served
`fixture` profile with its provider key. OpenRouter must have no key for the providers-tab
check; if it already has one, that check is skipped.

The suite uses the fixture profile, `ollama-cloud`, and `minimax-m3.1-flash`, and has a
900-second default deadline. Exit 0 means every step passed, 1 means a step failed or the
run was interrupted, 2 means preflight failed, and 3 means a step was skipped. Setting
`TALARIA_LIVE_REQUIRE=1` turns a skip into a failure. A FAIL names the step and reason and
includes a scrubbed pane capture; compare its text with the expected screen and check the
gateway and profile preconditions before classifying the result.

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

For `phase2b`, a startup failure can mean that the gateway is unavailable or the launcher
selected the wrong home. A skipped `permission` check means Herdr rejected both toggle-key
names. A queue check that never observes an indicator can mean the model did not run the
requested 25-second sleep; read the captured tool output before classifying that failure.

Every private path and name in the printed text, in the report and in the report path is
replaced by a placeholder: the scratch project, the temporary directory, the home directory, the
login name and the gateway home.
