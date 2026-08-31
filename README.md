# Talaria

Talaria is an experimental, Hermes-native terminal UI focused on a calmer, more capable workflow: less flicker, clearer run state, better sub-agent visibility, and an interface that can grow into an upstream contribution to Hermes.

The name comes from the _talaria_, Hermes's winged sandals. The project is intentionally named for the Hermes ecosystem rather than as a private product fork.

> **Status: v0.5.0 release candidate.** Talaria dials a fleet of configured Hermes gateways, tracks
> their sessions and waiting work, records every frame, and replays a recording with no socket open.
> The feature code is integrated. The [v0.5.0 acceptance run](docs/acceptance/2026-08-30-talaria-v0-5-0-live-acceptance-results.md)
> was executed on 2026-08-31 and returned **NOT SATISFIED**. The installed artifact was proven, but
> the approved live primary model route did not produce a passing receipt and `talaria-t1` item
> receipts and screenshots do not exist. Read the limits under "Quick start" before relying on it.
>
> **Talaria is written in Python** ([ADR-0004](platform-specs/04-architecture/adrs/0004-talaria-is-a-python-client.md)) with **Textual** as its terminal framework, which passed its validation gate on 2026-08-03 — see the [gate results](docs/analysis/2026-08-03-textual-validation-gate-results.md) and accepted [ADR-0005](platform-specs/04-architecture/adrs/0005-textual-is-talarias-presentation-layer.md). A small TypeScript tree remains under `src/`, reduced to the reference recorder the Python one is tested against; it is not the product and is described at the end of this page.

## Goals

- Provide a polished terminal workflow closer to the strengths of Claude Code-style interfaces.
- Preserve Hermes-native capabilities instead of reducing Hermes to generic chat.
- Use stable integration boundaries where possible: the Hermes API for core runs and sessions, the TUI gateway for richer Hermes control-plane behavior, and typed adapters for deterministic surfaces such as Kanban.
- Make changes small and reviewable. Contributing useful work upstream to Hermes is welcome when it fits, but it is not a constraint on Talaria's architecture — see [ADR-0001](platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md).
- Keep the local development installation separate from an official Hermes installation.

## Non-goals

- Talaria is not a replacement for Hermes core.
- Talaria does not import Hermes internals as its primary architecture.
- Talaria does not copy private Infiquetra operational context into this public repository.

## Current architecture direction

```text
                      Talaria
                         │
              transport and capability layer
                         │
       ┌─────────────────┼─────────────────┐
       │                 │                 │
  Hermes API        TUI gateway       Kanban adapter
  runs/sessions     rich events       typed operations
```

These are the product's integration boundaries, not a claim that every pictured adapter exists
today. The Hermes API and terminal gateway paths are implemented; a deterministic Kanban adapter
remains future work.

Across all of it, one rule is already settled: the domain core has no dependency on the terminal
framework. Frames become normalized events, normalized events become domain state, and only then does
anything render. See [ADR-0002](platform-specs/04-architecture/adrs/0002-the-domain-core-is-framework-independent.md).

## Quick start

The Python implementation is at the v0.5.0 release candidate. A bare `talaria` dials the configured
Hermes gateway fleet and opens a session; `talaria replay` drives the entire interface from a
recorded frame log with no socket open anywhere in the process.

**Read the limits before you rely on it.** The latest published statement is the
[v0.4.0 release note](docs/releases/v0.4.0.md): no person has driven Talaria on Linux; a credential
exported by the parent shell remains in the inherited process environment; and the framework
measurements used a bare pseudo-terminal rather than a real terminal emulator. That release also
shipped without its reserved human drive of a real foreign approval. The v0.5.0 acceptance run is
recorded as **[NOT SATISFIED](docs/acceptance/2026-08-30-talaria-v0-5-0-live-acceptance-results.md)**:
the installed artifact was proven, but the approved live primary model route failed to complete and
`talaria-t1` item receipts and screenshots do not exist. This page therefore does not claim that any
of the v0.4.0 limits has closed or that every new theme, status, inspector, diff, and polish surface
has terminal evidence. The historical
[v0.1 daily-driver verdict](docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md) remains useful for
the narrower evidence it recorded, but it is not the current release verdict.

```bash
uv sync --all-groups

# Attach to a running gateway. `--resume` returns to the most recent session;
# `--session <id>` opens an explicit one; the two are mutually exclusive.
uv run talaria
uv run talaria --resume

# Attach and record the session while you drive it. `--record` on its own writes
# a timestamped log under <config_dir>/recordings/; give it a path to choose one.
# This is what an R2/R3 first attach needs: a usable client and a recording.
uv run talaria --record
uv run talaria --record ./first-attach.jsonl

# Write the credential file from a running Hermes dashboard. The dashboard mints
# its session token at start-up and keeps it in memory, so this is needed again
# after every dashboard restart. The token is never printed.
uv run talaria refresh-credential
uv run talaria refresh-credential --from http://127.0.0.1:9119/

# Record with no interface, from a running Hermes gateway. The credential comes
# from the same chain the launcher uses -- the credential file written above,
# then the prompt -- and never from an endpoint: one carrying a credential in
# its query string, userinfo or fragment is refused, and exits 2. That holds
# whether the endpoint arrives on the command line or in TALARIA_GATEWAY_URL.
uv run talaria record
uv run talaria record ws://127.0.0.1:9119/api/ws   # endpoint override, no credential

# Replay that recording through the full interface. F8 pauses, F9/F10 change
# speed (replay only), ctrl+g/F2 folds sub-agent rows — F2
# is Mission Control on macOS — ctrl+c/F4 interrupts the turn when one is
# in flight, end/F5 re-follows the newest line, F3/ / shows commands, F6/models and F7/profiles
# open pickers; F1 is eaten on macOS and the approval card now owns focus so
# no jump key is needed (Tab or click reaches the card in the two residual
# cases). Controls that would change something on the gateway are visibly inert
# in replay.
uv run talaria replay <recording.jsonl>

# Re-run the framework validation gate. Exits non-zero on a fail verdict.
uv run talaria gate --corpus <recording.jsonl> --deltas 50000
```

After v0.5.0 is tagged, Talaria installs as an ordinary command from that release tag:

```bash
uv tool install git+https://github.com/infiquetra/talaria@v0.5.0
talaria --version
talaria
```

**Do not `uv tool install talaria`.** The name `talaria` on the Python Package Index belongs to an
unrelated content management system whose last upload was 2010-06-19, and installing it gets you that
project, not this one. Talaria is not published to PyPI and this is deliberate. The latest published
release still records no human Linux drive and bare-pseudo-terminal framework measurements, and the
[v0.5.0 acceptance run](docs/acceptance/2026-08-30-talaria-v0-5-0-live-acceptance-results.md)
returned **NOT SATISFIED**. Install from a release tag.

### Switching Hermes profile

`/profiles` opens a picker listing the profiles the connected Hermes knows about, with each one's
configured model and whether its gateway is running. Selecting a row **dials that profile's gateway**
and re-resolves the credential for it — Talaria never writes Hermes's own active-profile setting,
which by Hermes's own documentation "does not retarget the already-running dashboard process" and so
would change a machine-wide preference without moving the session in front of you.

The singular `/profile` is a Hermes command and still reaches Hermes; the plural is Talaria's. Both
Talaria-local names are marked `local` in the command listing.

Hermes publishes no address for a profile's gateway — measured on 2026-08-06, `GET /api/profiles`
returns names, paths, models and liveness, and no host or port — so Talaria needs to be told where
each one listens:

```toml
# ~/.talaria/config.toml
[profiles.endpoints]
work = "ws://127.0.0.1:9119/api/ws"
lab  = "ws://127.0.0.1:9219/api/ws"
```

A profile with no entry here is still listed, marked `[no endpoint configured]`, and selecting it is
refused before anything is dialled — as is a profile whose gateway is not running, marked
`[gateway not running]`. Those are two different problems with two different fixes, so they are two
different messages.

Each profile's dashboard mints its own session token, so the credential file holds one entry per
profile. Pair each one against **its own** dashboard:

```bash
uv run talaria refresh-credential                    # the default profile: the top-level token
uv run talaria refresh-credential --profile work     # writes [profiles.work] in the same file
```

`--profile` derives the dashboard from that profile's `[profiles.endpoints]` entry above, so a
profile with no configured endpoint is refused by name rather than paired against whichever gateway
happens to be the default. A profile whose token you do not currently hold reports
`credential_unavailable` with the reason on screen — nothing was dialled, so no gateway refused
anything, and every other connection stays up. A named profile never falls back to another
profile's token: that would present one gateway's credential to another, where the refusal reads as
an authentication problem rather than as "that credential was never for this gateway".

### Supplying the gateway credential

The credential is acquired once per dial and rides the WebSocket URL's `?token=` query parameter,
never the command line. **Two routes supply it**, in precedence order, highest first:

1. A `token` key in `<config_dir>/credentials`, refused unless the file's mode is `0600` or
   stricter. `talaria refresh-credential` writes this file for you.
2. An interactive hidden prompt, asked once before the interface starts.

**No environment variable is one of them.** Two used to be, and both were removed:

- `HERMES_DASHBOARD_SESSION_TOKEN`, the variable Hermes's own dashboard publishes, was the
  highest-precedence source until 2026-08-06. If you have it exported, Talaria ignores it.
- A `token` query parameter on `TALARIA_GATEWAY_URL` was the highest-precedence source until
  2026-08-07. Talaria now **refuses** to start with one rather than ignoring it, and tells you to
  rotate the value: it was readable to anyone who could read the process, and your shell wrote it to
  history. `TALARIA_GATEWAY_URL` names the endpoint and nothing else.

If you were using that second route, run `talaria refresh-credential` once and unset the credential
from your exported endpoint. Nothing else changes.

**You can now configure Talaria with nothing exported at all.** The credential file holds both
halves — `token` for the credential and `url` for the endpoint:

```toml
# <config_dir>/credentials, mode 0600
token = "..."                              # written by `talaria refresh-credential`
url = "ws://127.0.0.1:9119/api/ws"         # optional; the default is this value

[profiles.work]                            # written by `--profile work`
token = "..."
```

The top-level pair is the **default profile's** entry and stays valid forever — a file holding only
those two keys needs no migration. A `[profiles.<name>]` table holds one `token` and nothing else:
an endpoint there is refused, because addresses live in `[profiles.endpoints]` in `config.toml` and
having two places one could come from is a precedence nobody has defined. A syntax error fails the
whole file loud (one TOML document has one parse); after a successful parse, a bad table refuses
that profile by name and leaves the rest usable.

Stated narrowly, because the narrow claim is the true one: **no route Talaria supports puts a
credential in its environment, its command line, or your shell history.** What that does _not_ buy
is a clean process environment on its own — see the caveat below.

`talaria refresh-credential` writes that file for you, reading the session token from the page a
running dashboard already serves to its own web UI, preserving any other keys in the file, and
writing at `0600` through a temporary file so the value never exists at a looser mode. It refuses to
read a credential from another machine over plain HTTP. You will need it again after each dashboard
restart: Hermes mints the token with `secrets.token_urlsafe(32)` at server start and holds it only in
memory, so a restart invalidates whatever the file holds. A stale token is reported as a named
authentication failure, not a hang.

**The prompt only happens before the interface starts.** A credential cannot be requested once the
terminal belongs to the full-screen interface — the question would be painted where nothing can show
it. If no non-interactive source can supply one and there is no terminal to ask on, Talaria prints
what to do and exits `2` rather than opening a client that cannot dial.

**Talaria refusing to read a credential from the environment does not empty your environment.** Any
credential a shell exported before launching Talaria is inherited and stays visible in the process
environment for the life of the process, on Linux through `/proc/<pid>/environ` and on macOS through
`ps -E` to the owning user. No client can remove what the kernel captured at `exec` — a process
that scrubs its own `os.environ` changes nothing a reader sees, and the reader who can see Talaria's
environment can equally see the shell's that launched it. This applies squarely to
`HERMES_DASHBOARD_SESSION_TOKEN`: it is still visible to anyone who can read that process's
environment, whether or not Talaria consults it.

So the mitigation is yours, and it is one line: **do not export a gateway credential at all.** Both
routes above make that possible with nothing given up.

Talaria itself never adds the credential to its own command line or environment; that half is
asserted against a running process in `tests/transport/test_process_surface.py`, and the same file
asserts the _failure_ of the inherited half rather than defining it away.

### The TypeScript reference recorder

`src/` is down to three files, and they are not leftovers. `record/recorder.ts` and `record/redact.ts`
are the reference implementation the Python recorder is asserted _equivalent to_, across the
credential redaction boundary: `tests/recorder/test_equivalence.py` feeds the same frames through both
and compares the results, running the real TypeScript in a `tsx` subprocess rather than a
reimplementation of it. Deleting the tree would delete that guarantee. That is why these three
survived the cut on 2026-08-07 that removed the Ink prototype, the command-line entry point, the
recording command and the transport shim — five files nothing depended on any more.

```bash
npm install
npm run check
```

`npm run check` runs TypeScript compilation, the redaction unit tests, and a Prettier formatting
check. The equivalence assertion itself runs from the Python suite. The continuous-integration leg
that sets `TALARIA_REQUIRE_TS_BRIDGE=1` turns a missing Node toolchain into a failure rather than a
skip, because this harness once skipped invisibly inside a green run and nobody noticed that the
parity proof had stopped running.

## Documentation

- [Documentation index](docs/00-index.md)
- [Themes](docs/themes.md)
- [Configuration](docs/configuration.md)
- [Terminal UI](docs/terminal-ui.md)
- [Architecture decisions](platform-specs/04-architecture/adrs/) — what is settled and why
- [Project direction and conversation analysis](docs/analysis/2026-08-01-hermes-tui-project-direction.md)
- [Engineering journal](docs/engineering-journal/README.md)
- [Public-safe project context](docs/public-safe-summary.md)

### Settled decisions

| ADR                                                                                          | Decision                                                                    |
| -------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| [0001](platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md)           | Talaria is a standalone process, not a TUI bundle Hermes launches           |
| [0002](platform-specs/04-architecture/adrs/0002-the-domain-core-is-framework-independent.md) | The domain core is framework-independent; the terminal UI is a projection   |
| [0003](platform-specs/04-architecture/adrs/0003-talaria-re-encodes-hermes-tui-behavior.md)   | Talaria re-encodes the Hermes terminal UI's behavior rather than porting it |
| [0004](platform-specs/04-architecture/adrs/0004-talaria-is-a-python-client.md)               | Talaria is a Python client; the terminal framework is decided by a gate     |
| [0005](platform-specs/04-architecture/adrs/0005-textual-is-talarias-presentation-layer.md)   | Textual is the accepted presentation layer                                 |
| [0006](platform-specs/04-architecture/adrs/0006-block-rendering-is-bounded-by-work-and-height.md) | Block rendering is bounded by work and rendered height                 |

## Contributing

Small, focused pull requests are preferred. Before opening a pull request, run the repository's check:

```bash
uv sync --all-groups
uv run ruff check .
uv run mypy
uv run pytest
uv run bandit -r talaria -q
git diff --check
```

If the change touches `src/`, run `npm run check` as well.

Changes that create durable project knowledge should update the relevant engineering-journal file in the same change. See [AGENTS.md](AGENTS.md) for repository-local guidance.

## Relationship to Hermes

Talaria is an independent public project built against [Hermes Agent](https://github.com/NousResearch/hermes-agent)'s public interfaces. It is not an official Hermes distribution and does not imply upstream endorsement.

## License

Talaria is released under the [MIT License](LICENSE).
