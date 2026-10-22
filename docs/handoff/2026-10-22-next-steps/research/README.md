# Research reports behind the 2026-10-22 handoff

Fifteen research units ran in parallel on 2026-10-22, each with a written brief, a read-only rule,
and an instruction to give evidence (a `file:line` with a revision, a URL, or a command and its output)
for every claim and to label anything unverified. A further four-way build experiment (the stack
bake-off) is reported in [u16](u16-stack-bakeoff.md) and summarised in
[../05-architecture.md](../05-architecture.md) §Stack; its two Sonnet-built clients are kept in
[../reference-slices/](../reference-slices/).

The reports were copied here with private details replaced: `<talaria>` is this repository,
`<hermes>` is a Hermes Agent checkout at revision `22063fc09`, `<scratch>` is a session-local working
folder whose evidence files (captures, profiles, raw JSON) were **not** preserved, `~` is the home
directory, and private profile names are replaced with placeholders.

| Report | Question | Method | Model tier used |
|---|---|---|---|
| [u01](u01-talaria-architecture.md) | Is Talaria's Python codebase a foundation or a specification? | Code reading, line counts, a reducer benchmark | Opus 5.5 high |
| [u02](u02-talaria-performance.md) | Why does Talaria feel slow? | Pseudo-terminal harness, profiles, the framework gate | Opus 5.5 high |
| [u03](u03-talaria-history.md) | What does the project's history teach a rewrite? | ADRs, journal, analyses, issues, git churn | Opus 5.5 high |
| [u04](u04-hermes-client-surfaces.md) | Which Hermes surfaces should a client build on? | Hermes source at `22063fc09`, the OpenRPC contract, one live `identify` call | Opus 5.5 high |
| [u05](u05-hermes-release-delta.md) | What changed in Hermes since Talaria's pin? | Release notes v2026.9.14 to v0.22.4, upstream commits | Haiku 5.5 max |
| [u06](u06-hermes-desktop-catalog.md) | What does Hermes Desktop offer, and what fits a terminal? | Desktop source scan | Haiku 5.5 max |
| [u07](u07-hermes-ink-tui.md) | How does Hermes's own terminal UI work? | `ui-tui` source and docs | Haiku 5.5 max |
| [u08](u08-prior-art-coding-tuis.md) | Does a coding-agent terminal UI already meet the need? | Web, GitHub API, installed binaries | Opus 5.5 high |
| [u09](u09-prior-art-multiagent-acp.md) | Prior art for background agents, workflows, and ACP | Web, GitHub API, ACP schema | Opus 5.5 high |
| [u10](u10-prior-art-hermes-ecosystem.md) | Which third-party Hermes clients exist? | 30+ GitHub searches, plugin catalog, curated lists | Haiku 5.5 max |
| [u11](u11-language-framework.md) | Which language and terminal framework? | Startup, first-frame and Markdown benchmarks; code generation trials | Opus 5.5 xhigh |
| [u12](u12-claude-code-parity.json) | What does "functionally equivalent to Claude Code" cover? | Claude Code documentation and changelog | Haiku 5.5 max |
| [u13](u13-hands-on-talaria-drive.md) | What is it like to use Talaria today, beside Hermes's own UI? | Hands-on drive in a multiplexer against a live backend | Sonnet 5.5 high |
| [u14](u14-baseline-timing.md) | How fast do installed agent terminal UIs start? | Timed launches in multiplexer panes | Haiku 5.5 xhigh |
| [u15](u15-hermes-orchestration.md) | How could Claude-Code-style workflows run on Hermes? | Hermes source and docs | Opus 5.5 high |
| [u16](u16-stack-bakeoff.md) | Can moderate-tier agents build the client in Rust, and at what cost against TypeScript? | Four agents built the same client live; fixes logged, runtime measured | Sonnet 5.5 high and Haiku 5.5 xhigh, two of each |

## Corrections between reports

Where two reports disagree, the handoff follows the one with stronger evidence:

- **u08 said no third-party Hermes terminal client is close; u10 found 29**, including Herm
  (`liftaris/herm`, 301 stars). u10's finding was re-checked with the GitHub API and is used in
  [../04-prior-art.md](../04-prior-art.md).
- **u09 said Hermes's ACP adapter dials "the same socket Talaria dials".** The path name is the same
  (`/api/ws`), but the process is not: Talaria attaches to a standalone dashboard's legacy socket, while
  the ACP adapter and Hermes's own clients attach to the gateway's ticketed `hermes-gateway-v1` socket
  (u04 §1.2–1.3).
- **u05 lists "per-child model and provider settings" as a new capability.** That is one child model per
  profile (the `delegation` config block), not a model per child; u15 §1.1 checked the tool schema.
- **u07 and the Hermes README disagree** about how Hermes's terminal UI starts; u07 follows the code.
- **When the session authority shipped.** The first drafts of the handoff dated it to v0.22.0.
  v0.22.0 (2026-10-13) made one gateway process serve every profile. The session authority came
  later: the gateway owning every session, clients attaching as viewers with a ticket, and the
  canonical methods. Its first stable release was v0.22.3 (2026-10-21). v0.22.1 shipped it by
  mistake and v0.22.2 withdrew it. A reviewer checked this with the GitHub contents API, and the lead
  re-checked it on 2026-10-22: `gateway/session_authority.py` is absent at v0.22.0 and v0.22.2 and
  present at v0.22.3. The handoff's minimum supported Hermes is therefore v0.22.3.
- **u15's workflow sketch sends `unattended: true` with each prompt.** In Hermes that flag
  auto-approves every dangerous command in the turn (`tools/approval.py`, `_yolo_active`), and it
  does not mean "no viewer". The handoff's runner never sends it
  ([../06-workflows.md](../06-workflows.md)).
- **u05 counts the release notes that announce v0.23.0 as two in one place and three in another.**
  Two is right: the notes for v0.22.2 and v0.22.3.
- **u05, u06 and u12 were not allowed to write Markdown files** by their sub-agent rules; u05 was
  converted from structured output, u06 from the unit's final message, and u12 is the unit's JSON with
  per-row evidence paths removed.
