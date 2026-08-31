# Talaria v0.5.0 installed-artifact acceptance harness

This directory prepares issue #110's real-terminal acceptance without executing it. The harness
builds one frozen candidate wheel, installs that same wheel independently for `talaria-t1` and
`talaria-t2`, drives only each environment's installed executable through a real pseudo-terminal,
and creates evidence receipts.

Acceptance is not run from this checkout. Application behavior remains absent until the six feature
children are integrated on the run branch.

## Safety properties

- The candidate build refuses a dirty or detached integration tree. A commit identifier must name
  the bytes that were built.
- Candidate output and all unsanitized captures stay outside the integration tree.
- Each install uses a randomly named scratch root, a fresh virtual environment, and its own
  `TALARIA_CONFIG_DIR`. The real `~/.talaria` path is rejected.
- The installed-artifact probe uses isolated Python startup, removes `PYTHONPATH`, and verifies the
  package file, distribution metadata, and executable all live inside the tester virtual
  environment. A source checkout, editable install, or global executable is rejected.
- The pseudo-terminal driver repeats that provenance check before every drive. It preserves raw
  American National Standards Institute (ANSI) terminal bytes and escape sequences in the capture.
- Raw captures and screenshots stay in tester scratch until credential and private-identifier
  review. A receipt cannot pass while that review is pending or withheld.
- The only live routes are OpenCode Muse Spark 1.2 Contributor Free and, for one of the four approved
  reasons, Ollama GLM 5.3 Flash. The receipt validator rejects every other route. If the fallback is
  needed but unavailable, the leg is blocked and cannot pass.
- The harness does not use Computer Use, GUI automation, mocks, or a test-only Talaria application.
  Deterministic legs may drive the shipped `talaria replay`; live legs drive a real Hermes-backed
  session.

## 1. Build and freeze one candidate

Run the harness from the clean, integrated run tree. Choose a new path that does not already exist:

```bash
uv run python -m scripts.acceptance.v050_install_probe build \
  --integration-tree /path/to/clean/integrated-run-tree \
  --candidate-dir /tmp/talaria-v050-candidate
```

The command prints `candidate.json`. It records the branch, full and short commit, wheel filename,
version, and Secure Hash Algorithm 256-bit (SHA-256) digest. Do not rebuild separately per tester;
both testers install this manifest's exact digest.

## 2. Install and probe independently per tester

Run once for each tester:

```bash
uv run python -m scripts.acceptance.v050_install_probe install \
  --candidate-manifest /tmp/talaria-v050-candidate/candidate.json \
  --tester talaria-t1

uv run python -m scripts.acceptance.v050_install_probe install \
  --candidate-manifest /tmp/talaria-v050-candidate/candidate.json \
  --tester talaria-t2
```

Each command prints its `install-receipt.json` path. The probe installs dependencies and the wheel,
then verifies:

1. distribution and package version `0.5.0`;
2. an exact `talaria --version` result;
3. a bare installed launch in a pseudo-terminal, exited by scripted `ctrl+d` or `ctrl+q`; and
4. `talaria gate --deltas 5000 --json <scratch receipt path>`.

The bare probe starts with an empty isolated config. Reaching either Talaria's credential prompt or
the interface proves the bare console entry point loaded; the later live item proves a working
Hermes session. Populate each scratch config with only its throwaway acceptance profile and
credential before a live leg. Never point `TALARIA_CONFIG_DIR` back at the operator config.

## 3. Drive a checklist flow

Create an event script in the tester scratch root. Events are ordered by seconds from child start and
each event has exactly one action:

```json
{
  "events": [
    {"at_seconds": 1.0, "text": "/theme"},
    {"at_seconds": 1.1, "key": "ENTER"},
    {"at_seconds": 2.0, "resize": {"rows": 36, "columns": 119}},
    {"at_seconds": 3.0, "key": "ESCAPE"},
    {"at_seconds": 4.0, "key": "CTRL_Q"}
  ]
}
```

Supported actions are `text`, named `key`, raw `hex_bytes`, `resize`, and `signal`. Named keys include
arrows, Enter, Escape, Tab, Shift+Tab, Home, End, Page Up, Page Down, F5, and `CTRL_A` through
`CTRL_Z`. A signal is restricted to `SIGHUP`, `SIGINT`, `SIGTERM`, or `SIGKILL` and targets only the
harness-owned child process group.

Drive the installed executable. The result and capture paths must be under the scratch root named in
the install receipt:

```bash
uv run python -m scripts.acceptance.v050_pty_driver \
  --install-receipt /tmp/talaria-v050-talaria-t1-XXXX/install-receipt.json \
  --tester talaria-t1 \
  --terminal-program "Terminal.app 2.14" \
  --term xterm-256color \
  --rows 36 \
  --columns 132 \
  --event-script /tmp/talaria-v050-talaria-t1-XXXX/item-04-events.json \
  --capture /tmp/talaria-v050-talaria-t1-XXXX/raw/item-04.ansi \
  --result /tmp/talaria-v050-talaria-t1-XXXX/receipts/item-04-pty.json \
  --expect "visible literal required by this leg" \
  --timeout 60 \
  -- replay /path/to/sanitized/frame-log.jsonl
```

Omit command arguments after `--` for a bare live launch. Live legs use the scratch profile and a real
Hermes gateway. The driver exits nonzero on timeout, an empty capture, a missing expected literal, or
an unaccepted child exit. Use repeated `--accept-exit` values only when a checklist failure path
requires a specific nonzero exit.

The driver does not manufacture screenshots. The tester captures the real terminal through an
operator-approved, non-automated path, saves it under the tester scratch `screenshots/` directory,
and supplies that file to the receipt command.

## 4. Record and validate each item

For a successful primary live leg:

```bash
uv run python -m scripts.acceptance.v050_receipt record \
  --install-receipt /tmp/talaria-v050-talaria-t1-XXXX/install-receipt.json \
  --pty-result /tmp/talaria-v050-talaria-t1-XXXX/receipts/item-02-pty.json \
  --screenshot /tmp/talaria-v050-talaria-t1-XXXX/screenshots/item-02.png \
  --output /tmp/talaria-v050-talaria-t1-XXXX/receipts/item-02-talaria-t1.json \
  --tester talaria-t1 \
  --item 2 \
  --verdict pass \
  --session-mode live \
  --session-profile throwaway-profile-name \
  --route-requested primary \
  --route-observed primary \
  --route-status used \
  --fallback-availability unavailable \
  --redaction-review passed \
  --observation "real prompt and response traffic observed"
```

`fallback-availability unavailable` is allowed when the primary succeeded. If the primary fails and
the named fallback remains unavailable, record the fallback request as `not-reached`, select one of
the four allowed reason codes, include the exact reason detail, and use verdict `blocked`. The command
writes the blocked receipt and exits nonzero. It never permits a different model.

For replay and install-only items, use route aliases `none`, status `not-applicable`, and fallback
availability `not-applicable`. For a failure that never reaches a model, use the route that was
requested, observed route `none`, and status `not-reached`.

After all items are recorded, validate the combined receipt directory:

```bash
uv run python -m scripts.acceptance.v050_receipt validate-matrix /path/to/combined-receipts
```

The matrix requires both tester receipts for shared items, one owning-tester receipt for every
assigned item, and only pass or explicitly reserved terminal verdicts. The source item registry is
`docs/acceptance/v0.5.0/checklist-items.json`; the full steps and pass observations remain in the
visual specification.

## Harness-only check

This check drives the harness's small echo terminal. It does not run Talaria acceptance:

```bash
uv run pytest scripts/acceptance/test_v050_harness.py -q
```
