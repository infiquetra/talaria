# Talaria guide

## What Talaria is

Talaria is a terminal client for the Hermes agent gateway. It presents sessions and terminal input. Hermes owns the agent logic and session authority.

## Install

On macOS and Linux with Homebrew, install from the project tap with `brew install infiquetra/talaria/talaria`. See [install.md](install.md).

Or download the archive and checksums for your platform with `gh release download v<version> --repo infiquetra/talaria --pattern 'talaria-*-<target>.tar.gz' --pattern SHA256SUMS` and unpack `talaria-<version>-<target>/talaria` onto your path. See [install.md](install.md).

Install the Rust toolchain specified in `rust-toolchain.toml`, clone this repository, and build the client:

```bash
cargo build --release --locked
install -m 755 target/release/talaria ~/.local/bin/
```

Ensure that `~/.local/bin` is on your path. Alternatively, add the build directory to your path while working in the checkout.

Do not install a package named `talaria` from crates.io, npm or the Python Package Index: this project does not publish under that name there.

## Before you start

Talaria connects to a Hermes gateway. A profile selects the Hermes configuration and credentials for a session. Unless `--no-ensure` is given, Talaria may ask Hermes to ensure the gateway is running; it does not install Hermes. Gateway-backed examples below require a running gateway and may use a paid model.

## Your first session

Start a session and type a question:

```bash
talaria
```

Read the reply, then type `/quit`. To reopen the most recent session for the current folder, or select a session by its identifier:

```bash
talaria -c
talaria --resume <id>
```

## Working in a session

Use the [keybinding reference](keybindings.md) for composer editing, approvals and question cards. The transcript shows tool calls and diffs. Queue prompts behind a running turn, steer it when available, and use rewind to return to an earlier checkpoint. Use the model picker for the current session, `@` mentions for files, `!` for shell commands, and the transcript pager to search earlier output. Inspect commands before confirming an approval.

## Many sessions at once

Use `/fleet` to find, peek at, answer, or attach to sessions. You can launch a background session with `--bg`, and add `--worktree` to create a separate checkout. Files shared through `.worktreeinclude` may remain linked to the main checkout; review changes before using them. Notifications highlight sessions needing attention.

## Settings

Use `/config` for Hermes settings and `/configure` to prepare supported changes. `/doctor` checks the gateway and its capabilities. Hermes owns profile settings and credentials; Talaria reads its own `~/.talaria/config.toml` at startup. See [configuration](configuration.md).

## Workflows

Workflows are durable runs that can be detached and resumed. See the [workflow guide](workflows/runner.md) and inspect `talaria workflow --help` for run, resume, list and stop commands. Built-in workflow details are in [built-ins](workflows/builtins.md).

## Scripts and automation

Print mode runs a finite prompt without opening a screen:

```bash
talaria -p "Explain this project" --timeout 60
talaria -p "Explain this project" --output-format json --timeout 60
```

These calls use the gateway and may incur model costs. The print command exits 0 on completion, 1 on failure or unknown outcome, 2 for usage, 3 when input is required, 4 when the turn did not start, 124 on timeout, and 129, 130 or 143 on a signal. See [print results](formats/print-result-v1.md). A status-line command is described in [configuration](configuration.md).

## When something goes wrong

| What you see | Meaning | What to do |
|---|---|---|
| `talaria: no Hermes gateway is running for this home` | No gateway was found. | Start the gateway using Hermes, then retry. |
| talaria: the gateway never discovered a profile named <name> | The requested profile is not served. | Check its name and gateway configuration. |
| `talaria: no earlier session found for this folder.` | Continue found no session for this folder. | Start a session or choose one with resume. |
| `Talaria could not read ~/.talaria/config.toml (line <line>, column <column>): <message>. Using the default settings.` | The file has a syntax error; defaults are used. | Fix the line and restart. |
| `Hermes does not know /<name>. Type / to see the commands you can use.` | The slash command is not available. | Type `/` to see the palette. |
| FAIL  Profiles        the gateway never discovered a profile named <name> | The doctor found the gateway, but the profile is not served. | `Fix: Create the profile, or check the name with hermes profile list.` |

## Where things live

Talaria configuration is `~/.talaria/config.toml`. Hermes normally uses `~/.hermes`; `HERMES_HOME` selects a different Hermes home. Session state belongs to Hermes; see its documentation for the active profile's storage. The [migration guide](migrating-from-0.6.5.md) explains the former Python client.
