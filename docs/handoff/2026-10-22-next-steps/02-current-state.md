# 02 — Where Talaria stands on 2026-10-22

Status: `proposed`
Evidence: the research reports in [research/](research/), each of which names its commands and
revisions. Research reports are cited by number; for example, u11 is research/u11-language-framework.md,
the language and framework evaluation. Talaria was read at `cafc640` (the merge of pull request 187, 2026-09-25; the last content
commit is `101a951`, 2026-09-16). Hermes was read at the installed revision `22063fc09` (2026-10-17,
Hermes v0.22.2 plus canary commits). Measurements were taken on an Apple-silicon Mac (M2 Ultra) on
2026-10-22, with other agents running, so small differences are noise.

## The verdict in one paragraph

**Talaria 0.6.5 should be treated as an executable specification for a rewrite, not as a foundation
to keep building on.** Three things are broken at the center, not at the edges. First, it renders a
chat transcript as a tree of retained Textual widgets on the alternate screen, and that substrate is
where half the streaming time goes. Second, it decodes a fast-moving backend protocol by hand, attaches
through a path Hermes has since routed around, and as a result cannot answer approval prompts on the
current Hermes. Third, all orchestration lives in one application class of about 7,100 lines. Its best parts are
ideas and file formats, and every one of them survives a change of language.

## What was measured

| Dimension | Talaria 0.6.5 | Reference point | Source |
|---|---|---|---|
| Launch to first full frame, warm | 0.38 to 0.57 s | A minimal ratatui (Rust) app: 3.7 ms. A minimal Textual app: 127 ms | [u02](research/u02-talaria-performance.md) §1, [u11](research/u11-language-framework.md) §3.2 |
| Launch with no Python bytecode cache | 1.06 to 1.20 s | This is the real case in a shell that sets `PYTHONDONTWRITEBYTECODE=1` with a `uv tool` install, which compiles no bytecode by default | [u02](research/u02-talaria-performance.md) §1 |
| Launch to a usable composer against a live backend | 1.2 s | Hermes's own terminal UI: 3.0 to 3.7 s (it also makes sure its gateway is running) | [u13](research/u13-hands-on-talaria-drive.md) task 1 |
| CPU while a recorded reply streams at real speed | 28% of one core on average, 77.5% at peak | Target in this handoff: at most 5% average | [u02](research/u02-talaria-performance.md) §2 |
| Keystroke to screen while a reply streams | 21 ms median, 107 ms at the 95th percentile, 157 ms worst | One 60 Hz frame is 16 ms | [u02](research/u02-talaria-performance.md) §5 |
| Bytes written to the terminal per frame | grows from 1.2 KB to 12 KB as the message grows | Should be proportional to the cells that changed | [u02](research/u02-talaria-performance.md) §2 |
| Where streaming time goes | 48% in Textual's whole-screen layout pass; 17% in Talaria's own per-tick work, which re-parses the entire live message with a newly built Markdown parser several times per 50 ms tick | | [u02](research/u02-talaria-performance.md) §2 and §7 |
| The August framework gate | **fails today**: the growing-table workload's 99th-percentile apply time is 56.5 to 66.4 ms against a 50 ms ceiling, four runs out of four | It passed on 2026-08-10 at 44.0 ms | [u02](research/u02-talaria-performance.md) §2 |
| Idle | about 1% of a core and 2.2 repaints per second | Should be zero repaints when nothing changes | [u02](research/u02-talaria-performance.md) §4 |
| Resident memory | 67 to 72 MB at start, about 112 MB after one four-turn session | A minimal ratatui app: 2.8 MB | [u02](research/u02-talaria-performance.md) §3, [u11](research/u11-language-framework.md) §3.2 |

None of the targets the project set for itself in August covered launch time, keystroke latency,
streaming CPU, idle CPU or bytes per frame. Those are the dimensions the operator actually feels, so
the gate passing in August never meant the client felt fast.

## What a hands-on drive found

A research agent drove the installed Talaria and Hermes's own terminal UI side by side against the
live local backend on 2026-10-22, using a free model, on the same small coding tasks. Full notes are
in [u13](research/u13-hands-on-talaria-drive.md). Talaria's ten worst problems, most severe first:

1. **Talaria cannot answer Hermes's approval or question prompts, so dangerous commands are blocked.**
   Hermes now sends approvals only to clients that declare they can answer them
   (`client.capabilities {server_requests: true}`, added upstream on 2026-09-16). Talaria never sends
   that declaration, so Hermes withdraws the prompt and the command fails with "the attached client
   cannot answer approval requests". Hermes's own terminal UI showed a normal approval dialog for the
   same request.
2. **Background tasks started with `/bg` never report back.** The backend finished the task; Talaria
   showed nothing two minutes later. Hermes's terminal UI showed the result.
3. **The settings workspace is unusable as rendered.** It took 7 to 9 seconds to open, drew as
   run-on text with no visible fields, showed secrets masked to their first and last four characters,
   and left residue on screen after closing.
4. **Connecting to a specific backend is manual.** `refresh-credential` stores the token but not the
   address, so Talaria dials a default port the running dashboard did not use. The fix was an
   environment variable the user has to know about.
5. **Tool calls and diffs lose information.** Diffs printed raw colour escape codes as text; tool
   calls showed a name and a tick with no output or duration.
6. **Focus handling is fragile.** Typing was silently discarded while another region held focus;
   closed palettes left rows on screen; Page Up stopped following new output without saying so.
7. **Sending while busy is invisible.** A second prompt was injected into the running turn with no
   label; Hermes's terminal UI shows "queued (1)".
8. **Subagent control is thin.** Rows with timers, but no output tail, no detail, no steering. Hermes's
   own terminal UI has a spawn tree with tail, detail, steer and stop.
9. **Version-drift noise on every launch.** A compatibility warning on every start, "unknown event
   type" lines in the transcript, and "context ?%" because the model's window is unknown.
10. **Session discovery is slow and noisy.** The session picker took 1.5 to 2.1 s (Hermes: 0.07 s),
    labels every row "never observed", and `/resume` means "release the replay clock".

What worked and must not be lost: Talaria reached a usable composer three times faster than Hermes's
terminal UI, interrupt was fast and labelled, resize was instant, the model switch said plainly that it
applied to this session only, and the model picker listed only configured providers.

## Why it ended up here

Each mechanism is backed by the cited research report.

1. **The rendering substrate fights the product.** A chat transcript is mostly finished text that
   never changes again. Textual models it as a tree of retained widgets (one per line, one per
   Markdown block, one per table cell) on a full alternate screen, then relays out and repaints that
   screen on every tick. Architecture decision record (ADR) 0006 (bounded block rendering) was a
   25 KB effort to cap that cost, and the cap no longer holds. Clients that feel fast write finished
   history into the terminal's own scrollback once and redraw only a small live region. ([u02](research/u02-talaria-performance.md),
   [u08](research/u08-prior-art-coding-tuis.md) §5)
2. **The protocol was transcribed by hand from a backend that moves daily.** Talaria's compatibility
   baseline is pinned to Hermes `7f4d15515` (2026-08-01); Hermes is 36,115 commits past it. Since
   2026-09-14 Hermes publishes a generated, typed contract for every method and event, and since v0.22.3
   (2026-10-21) it runs one session authority per machine that clients attach to with a ticket. Talaria
   uses neither; it attaches to a standalone dashboard's legacy socket that has no session authority.
   ([u01](research/u01-talaria-architecture.md) §2, [u04](research/u04-hermes-client-surfaces.md) §1)
3. **There is no application layer.** The framework-independent domain core (ADR-0002) is real and
   cheap: about 14 µs per frame. But nothing sits between it and Textual, so every feature's
   orchestration went into `TalariaApp` in `talaria/ui/app.py`: a class of about 7,100 lines with 226 methods, in an
   8,486-line file, changed in 85 of the 504 non-merge commits. ([u01](research/u01-talaria-architecture.md) §4)
4. **Process consumed more effort than product.** About 44% of all lines added were acceptance
   records, acceptance scripts, process documents and the journal; the product package was 20%. In
   the v0.5.0 and v0.6.1 release windows, product code was 8% and 9% of lines added. The tracked
   `docs/` tree holds 2,406 files, of which 2,234 are acceptance evidence. 37% of commit subjects are
   about acceptance, receipts, evidence, verdicts, gates or records; two are about performance.
   ([u03](research/u03-talaria-history.md) §1.2, §6)
5. **Architecture sized for a topology that changed.** Version 0.4 built a client that dials one
   gateway per Hermes profile (about 11,000 lines in one release). Hermes v0.22.0 replaced that
   topology with one backend serving every profile. ([u03](research/u03-talaria-history.md) §4.4)
6. **The code became the change log.** About a third of the package's lines are comments and
   docstrings, with 623 in-code references to "key technical decision" numbers and 617 to requirement
   numbers. Agents reading the code read history instead of intent. ([u01](research/u01-talaria-architecture.md) §4)

## What to carry into the rewrite

Ideas and artifacts, not modules:

- **The frame-log format** (`docs/formats/frame-log.md`). It is language-neutral and defines the replay
  test oracle. The old recorded corpora are not a usable starting point: `corpora/` is git-ignored and
  local to the operator's machine, and the only tracked corpora (`docs/acceptance/v0.5.0/**/corpora`)
  were recorded against the older Hermes protocol and are removed with `docs/acceptance/` in P0-02.
  Fresh fixtures recorded against the canonical gateway in P1-04 are the oracle.
- **A pure reducer and a projection, plus a real application layer between them and the screen.**
  `update(state, event) -> state` and `view(state) -> frame`, with services (connection, sessions,
  prompts, settings) as their own layer.
- **Redaction before anything touches disk**: the method deny-set for secret-bearing replies, URL
  credentials redacted by position, route-aware HTTP redaction, and recording what was redacted.
- **Honest outcomes**: a call whose socket dropped is "unknown", never "succeeded"; replies from a
  replaced connection never update liveness.
- **The fleet model**: fixed-size per-session summary rows with live, stale and never-observed states,
  and one "needs you" queue with stable item identity across sessions.
- **The status-line contract**: an external command fed a JSON payload on standard input, with an
  environment allow-list (`docs/formats/status-line.md`).
- **The terminal lessons** recorded in [u03](research/u03-talaria-history.md) §3.2: macOS takes F1 and
  F2 before any application sees them; Herdr (a terminal multiplexer, like tmux) takes Ctrl+B when nested; Ctrl+S collides with XON/XOFF
  flow control; people press Ctrl+C to copy; mouse capture breaks the terminal's own select-to-copy;
  Page Up must actually scroll; negotiate synchronized output rather than assuming it; render untrusted
  text literally; a new prompt takes the caret only when the composer is empty.
- **The protocol rules** in `docs/analysis/2026-08-02-hermes-reconciliation-rules.md` and
  [u03](research/u03-talaria-history.md) §3.1, re-checked against the current Hermes (several are now
  obsolete; for example approvals now carry request identifiers).
- **The operator's wishes that never shipped**: select-to-copy that works, a "notes" field on an
  approval answer, a jump-to-bottom affordance, Page Up that scrolls, and declining a blocking prompt
  without waiting for it to expire.

## What to leave behind

- The acceptance-receipt machinery (`scripts/acceptance/`, `docs/acceptance/`, release-record tests,
  per-release gate documents). Keep one thing that worked: a person or agent driving a real terminal
  before each release.
- The TypeScript reference recorder in `src/`, which exists only to prove the Python recorder's
  redaction matches it.
- The compatibility baseline pinned to `7f4d15515` and the startup seam probe built on the premise
  that the gateway publishes no capabilities.
- The per-profile gateway fleet and the `[profiles.endpoints]` map.
- Scraping the dashboard's session token out of its HTML page (`refresh-credential`).
- Replay pacing as a user feature (keep the recorder as a developer and fixture tool).
- Large peripheral surfaces built before the core was solid: the theme marketplace and Visual Studio
  Code theme importer (about 3,400 lines), and the untested, mostly unreachable settings surfaces
  module (972 lines).
