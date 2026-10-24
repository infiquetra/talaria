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
| `--deadline-secs` | The whole run, in seconds. |
| `--step-scale` | The scale of the per-step limits. |
| `--report-dir` | Where the failure report is written. |

`--step-scale` multiplies the per-step limits only: the shell wait, the startup wait, the reply
wait, the approval windows, the count wait, the settle wait, the quit wait and the resume wait.
It never multiplies the deadline, the quiet periods, the wait after Esc or the poll interval.
`--deadline-secs` is the whole run and is never scaled.

## Preconditions

A running gateway, the fixture profile and a built binary.

The approval step needs the profile to ask for approval itself; with the default mode the
gateway may approve the command on its own, and the step then skips with a plain message and
the run exits 3.

## Exit codes

0 passed, 1 failed, 2 preflight failed, 3 passed with a skip.

## Steps

Startup, plain answer, approval, interrupt, quit and resume.

## Reading a failure

A failing step prints the screen of the pane it was in and writes the same text, with the
version of the multiplexer and the path of the binary, to the report file. The path of the
report is printed with the private parts replaced by placeholders.

A pane text that reports the gateway or profile unavailable makes the acceptance result blocked
by the environment, not failed. The script cannot tell the two apart: it reports a failure and
leaves the reading to the reader.

Every private path and name in the printed text, in the report and in the report path is
replaced by a placeholder: the scratch project, the temporary directory, the home directory, the
login name and the gateway home.
