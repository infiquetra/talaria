# SEAT-4A: Smoke harness: an hc-backed terminal beside the old Herdr one

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the trait that exists today and for where the harness's terminal name and folder come from. The requirements, the tests and the acceptance checks are only in the issue. This page points at them by id.

## What exists today

`tools/smoke` is a separate package. Its root comment says it drives the built binary through the Herdr command line (`tools/smoke/src/lib.rs:1`). The binary is `smoke`. `HerdrCli` and the `Terminal` trait live in `tools/smoke/src/terminal.rs`. The crate root re-exports them at `lib.rs:65`.

`Terminal` (`terminal.rs:51-75`) is `create_tab`, `split_pane`, `run`, `send_text`, `send_keys`, `read`, `foreground`, `pane_get`, `tab_exists`, `close_tab` and `version`. `pane_get` has a default that returns `TermError::Parse("pane get is not supported")` (`terminal.rs:68-69`). The other methods have no default.

`HerdrCli` (`terminal.rs:78-81`) stores an executable path and a workspace id. `new` takes both. `check` (`terminal.rs:119-122`) runs `tab list --workspace` and discards standard output. `call` (`terminal.rs:92-116`) retries operating-system error 26 up to 20 times, 5 milliseconds apart. A missing executable is `TermError::NotFound`. A non-zero status is parsed as JSON `error.code` and `error.message` (`terminal.rs:141-158`).

The twelve argument lists are all inside `HerdrCli`:

- `create_tab` runs `tab create` (`terminal.rs:162-178`).
- `split_pane` runs `pane split` (`terminal.rs:180-195`).
- `run` runs `pane run` with the text as one argument (`terminal.rs:197-198`).
- `send_text` runs `pane send-text` and does not press Enter (`terminal.rs:201-202`).
- `send_keys` passes each key as its own argument (`terminal.rs:205-208`).
- `read` passes `--source` and, when a count is given, `--lines` (`terminal.rs:211-223`).
- `foreground` reads `result.process_info.foreground_processes[].cmdline` (`terminal.rs:226-242`).
- `pane_get` reads `result.pane.agent` and `result.pane.agent_status` (`terminal.rs:246-251`).
- `tab_exists` runs `tab list` and looks for `result.tabs[].tab_id` (`terminal.rs:254-269`).
- `close_tab` runs `tab close` (`terminal.rs:271-272`).
- `version` runs `--version`, trims standard output, and returns an empty string on failure (`terminal.rs:275-279`).

`main.rs` builds one `HerdrCli` and passes it to every suite. The `--herdr` flag defaults to the string `herdr` (`main.rs:45-46`). Nothing in `main.rs` names `hc`.

Other `Terminal` implementations, which break if the trait gains a method, are `FakeTerminal` (`tests/support/mod.rs:1312`), `ScriptedReads` (`tests/screen.rs:699`), `ReadTerminal` (`tests/support/scripts4b.rs:339` and `tests/support/scripts5.rs:342`), and `SignalOnRead` in the cleanup tests.

The fake `herdr` is the bin `fake-herdr` (`tools/smoke/Cargo.toml:16-19`), implemented in `tests/support/fake_herdr.rs`. `FakeHerdr` (`tests/support/mod.rs:1538`) writes a wrapper script and `fake_herdr_exe` uses `env!("CARGO_BIN_EXE_fake-herdr")` (`mod.rs:1458-1459`). `FakeHerdr::log` stores one JSON array of arguments per line (`mod.rs:1591-1597`). `tools/smoke/tests/herdr_cli.rs` drives `HerdrCli` through that fake. REQ-13 and REQ-14 keep that file and that bin unchanged.

The approval step sends one digit with `send_text` and does not press Enter in that call (`tools/smoke/src/steps.rs:1047-1049`). A later submit helper sends a whole line with `send_text` and then `send_keys` of `enter` (`tools/smoke/src/steps5.rs:58-63`). REQ-5 is the digit. REQ-12 is the pair.

There is no `hc` type, no `fake-hc` bin, and no smoke test whose name contains `hc_cli`.

## The command map

The replies were measured on hc 0.1.20 on 2026-11-11. The reference is `docs/handoff/2026-11-11-v0-9/hc-facts.md`. The files beside it are what REQ-13 copies. Help text is not the source of the JSON fields.

- Piped `hc ls` is `ls.json`. `terminals` is an array. REQ-10 matches `address` and `state` `live`. REQ-10 does not pass `--json`.
- `hc run --bg --no-parent --name --cwd` with no program is `run_bg.json`. The string field `address` is the id. `split_pane` uses `run_split.json` and omits `--name`.
- `hc send` pastes its text and presses Enter. REQ-11 is one `send` and is not retried. Exit status 9 is `send_blocked.json` on standard error. `HcCli` maps any non-zero exit to `TermError::Io` and does not retry.
- `hc keys` accepts the names on the reference page, including `space` and `shift+` plus a lowercase letter. A refused key is exit status 6, `keys_unknown.json`.
- `hc read` with no `--lines` is `read_screen.txt`. `hc read --lines` is `read_lines.txt`. The default is 0. There is no `--source`. REQ-13 loads each file from the copied folder. `replies.json` holds an ordered list per first argument, and each call takes the next item.
- `hc kill` returns after the program has ended (`kill_live.json` has `state` `finished`). `hc rm` follows at once.
- `hc --version` is `version.txt`, the line `hc 0.1.20`. REQ-3 trims it. A failed `version` returns an empty string.

`SEAT-4B` does not add `hc wait`. The harness waits stay read-and-compare loops. `POLL_INTERVAL` is 250 milliseconds (`tools/smoke/src/screen.rs:18-19`). That is not this unit.

`create_tab` and `split_pane` start terminals with `hc run` and no program argument. `foreground` and `pane_get` read `hc get <address> --json`. REQ-8 is that map. `HerdrCli::pane_get` (`terminal.rs:246-251`) returns `format!("{agent}\n{status}")`, and both fields default to empty, so a pane with no agent is the string `"\n"`. REQ-8 uses that string when `status` is null and when the `status` key is absent. If `foreground` is present, is not null, and is not an object whose `argv` is an array of strings, REQ-8 returns `TermError::Parse`. An empty `argv` array is an empty vector.

## Where the name and the folder come from

The first suite builds the label `talaria-smoke-{run_id}` and the folder `cfg.temp_root.join(&label)` (`tools/smoke/src/steps.rs:216-218`) and passes both to `create_tab` (`steps.rs:227`). REQ-8 uses that label only as `--name` on `create_tab`. `run` (`terminal.rs:55`) sends its text into the pane id and does not start a terminal. Every `hc` child has `HC_TERMINAL_ID` and `HC_SCOPE` removed.
