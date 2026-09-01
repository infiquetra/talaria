# Install Talaria

This guide is for a macOS user who already runs Hermes Agent and wants Talaria as a separate
terminal client. Before the first command, you should have an interactive terminal, Git, `uv`,
Python 3.12 or 3.13, and a running Hermes dashboard and terminal gateway that Talaria can reach.
Talaria connects to that gateway; it does not install or start Hermes for you.

## Platform support

macOS is the supported platform for Talaria v0.5.0. The release has been exercised there with its
real terminal interface and with Python 3.12 and 3.13.

Linux is not supported for interactive use in this release. The test suite runs on Linux in
continuous integration, but no person has driven Talaria on Linux. That hands-on path was parked
during v0.3 and was not revisited for v0.5.0. Windows is not supported and has no release acceptance
path. Passing automated tests on either platform would not establish terminal behavior that nobody
has driven.

## Install a tagged release

Talaria is not published to the Python Package Index. Install the repository's release tag into an
isolated `uv` tool environment:

```bash
uv tool install git+https://github.com/infiquetra/talaria@v0.5.0
```

Do not run `uv tool install talaria`. The `talaria` name on the Python Package Index belongs to an
unrelated project, so that command installs the wrong software.

The tag command applies after v0.5.0 is published. To run the untagged release candidate from an
existing checkout instead, install its locked development environment and keep every invocation in
that environment:

```bash
uv sync --all-groups
uv run talaria --version
uv run talaria
```

## Verify the installation

For a tagged tool install, bypass the shell's command search during the first check:

```bash
"$(uv tool dir --bin)/talaria" --version
"$(uv tool dir --bin)/talaria" --help
```

For a source checkout, use the project environment:

```bash
uv run talaria --version
uv run talaria --help
```

A successful v0.5.0 install prints this version line:

```text
talaria 0.5.0
```

The help output identifies Talaria as a `Hermes-native terminal UI client` and lists `record`,
`replay`, `refresh-credential`, `theme`, and `gate`. After the gateway credential is ready, launching
`talaria` for a tool install or `uv run talaria` from the checkout opens the terminal interface and
starts connecting to the configured Hermes gateway.

## Prepare the gateway credential

Hermes keeps the dashboard session token in memory and creates a new one when its dashboard starts.
Write the current token to Talaria's credential file without printing it **before the first
launch**:

```bash
talaria refresh-credential
```

From a source checkout, use `uv run talaria refresh-credential`. By default the command derives the
dashboard address from the configured gateway and creates or updates `~/.talaria/credentials` at
mode `0600`. Use `--from http://127.0.0.1:9119/` when you need to name the default dashboard
explicitly. A different `TALARIA_CONFIG_DIR` relocates the credential file along with the rest of
Talaria's configuration.

## Troubleshooting

### A bare command exits without output

A stale global `talaria` on `PATH` can shadow the repository build or the executable installed by
`uv`. This has been observed with an older frozen scaffold: the real checkout worked through
`uv run talaria`, while the bare command reached the stale copy instead.

Compare the selected executable with `uv`'s tool directory:

```bash
command -v talaria
uv tool dir --bin
uv run talaria --version
"$(uv tool dir --bin)/talaria" --version
```

First compare the directory containing the path from `command -v talaria` with the directory printed
by `uv tool dir --bin`:

- If they are different directories, another executable is shadowing the uv tool. Remove that stale
  executable from the command search or put the uv tool directory earlier on `PATH`. Start a fresh
  shell and repeat `command -v talaria` before launching.
- If they are the same directory, the uv tool environment itself is stale. Replace it with the tagged
  release:

  ```bash
  uv tool install --force git+https://github.com/infiquetra/talaria@v0.5.0
  ```

  Alternatively, remove the tool and install the tag again:

  ```bash
  uv tool uninstall talaria
  uv tool install git+https://github.com/infiquetra/talaria@v0.5.0
  ```

In either case, repeat the version checks after the repair. Do not install the unrelated Python
Package Index package.

### First launch stops at the credential prompt

Launching before preparing the credential can stop at the hidden prompt
`Hermes gateway session token:`. Run `talaria refresh-credential` first to avoid this prompt. If it
is already waiting and Ctrl+C does not clear it, send end-of-file with Ctrl+D or close the terminal,
then refresh the credential and launch again.

This wait is specific to an interactive launch. With no controlling terminal, Talaria refuses the
prompt, exits cleanly, and prints an error that names `talaria refresh-credential` as the fix.

### Authentication fails after Hermes restarts

A Hermes restart that restarts the dashboard invalidates the token already stored in
`~/.talaria/credentials`. Talaria then reports `authentication failed`; that message distinguishes a
rejected credential from an unreachable gateway, but it does not say that a restart made the token
stale.

Refresh the credential from the running dashboard and launch Talaria again:

```bash
talaria refresh-credential
talaria
```

Use the `uv run` forms when working from a source checkout. For a named Hermes profile, run
`talaria refresh-credential --profile NAME`; that profile must have its own endpoint under
`[profiles.endpoints]` in `config.toml`.

Configuration locations, gateway precedence, and profile endpoints are documented in
[Configuration](configuration.md).
