# TRU-7: Help text for --resume

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the code that exists today and for why the long-help assertion is shaped the way REQ-3 describes. The issue states what to change. This document does not restate those requirements.

## The flag today

`crates/talaria-cli/src/args.rs` defines `Cli` with `color = ColorChoice::Never` and an `after_long_help` string (`args.rs:32-37`). That string describes print mode, then `--bg` and `--worktree`. It does not mention `--resume`.

`continue_session` is `-c` / `--continue` and conflicts with `resume` (`args.rs:40-41`). It has no documentation comment. `resume` is the next field (`args.rs:42-43`):

```rust
#[arg(long, num_args = 0..=1, value_name = "ID", value_parser = required_text)]
pub resume: Option<Option<String>>,
```

`required_text` rejects an empty or all-whitespace value (`args.rs:10-16`). Neighbouring flags that do have documentation comments are `ultracode`, `bg`, `worktree`, and `name` (`args.rs:60-70`). The `name` comment uses backtick characters around the flag name (`args.rs:69`).

`Cli::into_launch` maps the fields (`args.rs:93-99`). `-c` becomes `StartMode::Continue`. A present `resume` becomes `StartMode::Resume`, with `Some(None)` staying `None` and an identifier trimmed. Otherwise the mode is `StartMode::New`. `StartMode` is `New`, `Continue`, and `Resume(Option<String>)` (`crates/talaria-app/src/launch.rs:14-18`).

When the mode is not `New` and `new_session_options` is non-empty, `into_launch` returns `UsageError::NewSessionOptionsWithResume` (`args.rs:114-116`). `new_session_options` pushes only `--model`, `--provider`, and `--effort` (`launch.rs:101-112`). `--profile` is not in that list. The error variant is `UsageError::NewSessionOptionsWithResume { given: Vec<&'static str> }` (`launch.rs:52-54`).

`LaunchFailure::from` renders that variant as `--model only apply to a new session; a resumed session keeps the model it was created with.` when `given` is `["--model"]`, and the next step `Remove those options, or start a new session with talaria.` (`crates/talaria-app/src/failure.rs:90-97`). `render` prefixes `talaria: ` and `Next step: ` and ends each line with a newline (`failure.rs:20-21`). REQ-4 leaves this text alone. UAT-2 quotes it.

## What the binary does before a gateway

`main` parses `Cli`, runs `validate_print_options`, and only then calls `into_launch` (`crates/talaria-cli/src/main.rs:69-90`). For a command that is not print mode and not background, `validate_print_options` returns `Ok(())` when there is no prompt, output format, timeout, or input format (`args.rs:265-287`). `talaria --resume local-session-1 --model minimax-m3.1-flash` takes that path, then fails in `into_launch`, and `main` prints `failure.render()` and exits with `failure.exit_code` (`main.rs:85-90`). It does not call `run_with_ultracode`. REQ-6 is this order. The help path is the clap error `DisplayHelp` in `render_usage` (`args.rs:129-136`), which also finishes before a launch.

## The tests that already exist

`forms_defaults_and_refusals` (`crates/talaria-cli/tests/cli_args.rs:12`) rejects `--resume` with an empty value (`cli_args.rs:20-21`) and rejects `-c` combined with `--resume` (`cli_args.rs:23-24`). Bare `--resume` becomes `StartMode::Resume(None)` (`cli_args.rs:59-62`). An identifier with surrounding spaces becomes `StartMode::Resume(Some("local-session-1"))` and allows `--profile fixture` (`cli_args.rs:64-77`). The new-session refusal in that test is `-c` with `--model minimax-m3.1-flash` and `--effort high`, matching `given == ["--model", "--effort"]` (`cli_args.rs:42-57`). It does not pass `--resume` together with `--model`. REQ-4 says not to edit this test. UAT-2 is the `--resume` plus `--model` case, through the same `into_launch` branch.

`flags_help_and_neutralised_errors` (`cli_args.rs:102`) sets `help` from `Cli::command().render_long_help().to_string()` (`cli_args.rs:103`) and asserts that the string contains `--record`, `--bench-replay`, `--bench-speed`, `--bench-report`, `--no-ensure`, and `--effort` (`cli_args.rs:104-113`). It does not assert `--resume`. The two `Command` spawns after that loop clear the environment and set `PATH` (`cli_args.rs:115-147`). REQ-3 adds assertions to the `help` string only and leaves those spawns in place.

`commands_in_docs_exist_in_help` (`crates/talaria-cli/tests/docs.rs:146`) scans fenced bash blocks. For a word that starts with `--`, it asserts the help contains that flag spelling (`docs.rs:983-996`). `docs/guide.md:44-45` is a bash block containing `talaria --resume <id>`, so the test already requires the spelling `--resume`. REQ-5 says not to edit the test or the guide.

## Why the assertion splits the two copies

The workspace pins `clap = { version = "=4.6.7", features = ["derive"] }` (`Cargo.toml`). That feature set does not turn on terminal-width wrapping, so `render_long_help` wraps at 100 columns. Text beside a flag wraps in the columns left after the flag column, which is narrower than the sentence. A paragraph of the sentence alone fits on one line at 100 columns.

The trailing copy is one line at that width, so REQ-3 can match it at the end of the string. The copy beside the flag is wrapped, so REQ-3 collapses whitespace before counting it. The command attribute `color = ColorChoice::Never` is why the existing test can already find plain flag spellings in `to_string()`.

## Callers this issue does not move

`validate_print_options` refuses `--bg` combined with `-c` or `--resume` (`args.rs:244-247`) and refuses bare `--resume` under print mode (`args.rs:316-320`). Those strings stay. `crates/talaria-cli/src/run.rs` owns the resume list. `crates/talaria-cli/src/picker.rs` owns the list rows. Neither is a file this issue edits.
