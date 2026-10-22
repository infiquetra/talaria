# u11: Language and terminal-UI framework for a rewritten Talaria

Research unit for the "Claude Code for Hermes" handoff. Talaria is the operator's experimental
third-party terminal client for Hermes Agent (repository `<talaria>`,
Python 3.12 + Textual 8.2.8, version 0.6.5, `main` at `cafc640`, which is current with `origin/main`
as of a `git fetch` on 2026-10-22). This unit asks which language and terminal-UI framework a rewrite
should use, given that AI coding agents of moderate capability will write most of the code.

Everything below was measured or read on 2026-10-22 on an Apple M2 Ultra running macOS 27.0.1. Hermes
is installed at `<hermes>`, revision `22063fc099` (committed 2026-10-18,
`v0.22.2+canary`). Scratch artifacts live under
`<scratch>/`
(paths in this report shorten that prefix to `$S`). Nothing in either repository, Hermes's
configuration, `~/.talaria`, or any Hermes process was touched. Rust, Go, npm and uv downloads went to
caches inside `$S`.

---

## 1. Bottom line

**Recommendation: Rust, with ratatui 0.30 + crossterm + tokio, using OpenAI's Codex command-line
tool (Codex CLI) terminal UI as the reference implementation.** The case rests on three measured facts
and one piece of prior art:

1. **Speed.** A minimal ratatui app draws its first frame in a pseudo-terminal in 3.7 ms with 2.8 MB
   resident memory. The same minimal app is 127 ms / 38 MB in Textual and 178 to 321 ms / 94 MB in
   OpenTUI. Talaria's own first frame (replay mode) is 347 ms / 64 MB.
2. **Streaming Markdown.** A naive Rust full re-render of a 16 KB message (parse, syntax-highlight,
   wrap, paint) takes 5.4 ms. Textual's Markdown widget needs about 37 ms per frame on its incremental
   `append()` path at 16 KB, and about 123 ms at 64 KB. That is past a 33 ms (30 frames per second)
   budget, which is the most likely mechanical source of "slow".
3. **Protocol code generation works in Rust.** It needed a patch: `cargo typify` 0.8.0 generated
   925 types from Hermes's 861 gateway schemas after defaults were stripped from 3 schemas, and the
   result type-checks.
4. **Prior art.** Codex is a production Claude-Code-class client in Rust under the Apache-2.0
   license. Its `codex-rs/tui` crate already solves the hard terminal problems: writing finished
   history into the terminal's native scrollback, a 170 KB multi-line text editor, streaming Markdown,
   diff rendering, keyboard protocol modes, images, and synchronized output. It also embeds V8 and
   Starlark (a Python-like configuration language), which is prior art for scripted workflows.

**The runner-up is close: TypeScript + OpenTUI, running on Bun (as OpenCode does).** Under my weights
it scores 78 against Rust's 81 (out of 100). It wins outright if you weight "reuse Hermes's own client"
and "agent familiarity" more and startup/memory less (section 6). Its decisive advantage is concrete.
Hermes ships a 8,672-line, MIT-licensed, zero-dependency TypeScript gateway client plus generated
types. I verified that it type-checks outside the Hermes repository and runs unmodified on Bun
against a mock gateway. That matters because the gateway contract grew from 215 to 257 methods and
from 605 to 861 schemas in the five weeks since Talaria's last Hermes pin.

**Rejected:** staying on Python + Textual (the slowest measured path, and the framework's vendor
wound down), stock Ink (every serious user has replaced or forked its renderer), and Go + Bubble Tea
v2 (fast and simple, but its code generators failed on or flattened Hermes's contract, and Go has no
sum types for the 79 notification kinds).

---

## 2. What August decided, and what has changed

Four analyses and one gate result were written on 2026-08-02 and 2026-08-03 under
`docs/analysis/` in the Talaria repository:

- The **final analysis** (`2026-08-02-language-and-tui-framework-analysis-final.md:15-17`) chose
  TypeScript + OpenTUI, with Go + Bubble Tea v2 as fallback, rejected stock Ink, and placed Rust +
  ratatui fourth because "ratatui intentionally leaves the async runtime, event architecture, command
  bus, and many compound widgets to the application" (lines 186-190).
- The **reconsideration** (`...-reconsideration.md:20-22`) superseded it and chose Python + Textual,
  for four stated reasons (lines 52-56): Talaria was greenfield; agents would write it; the
  surrounding Infiquetra repositories are Python; and Hermes core is Python. It ranked expected agent
  velocity as "Python/Textual → Go/Bubble Tea v2 → TypeScript/OpenTUI → Rust/ratatui", explicitly as
  "a project-specific hypothesis, not a model benchmark" (lines 253-257). It named the flip
  conditions, including "Textual cannot bound long-transcript cost" (line 391-393).
- The **Textual gate** (`2026-08-03-textual-validation-gate-results.md:61-63`) passed 13 threshold
  checks on the third attempt. The first two attempts were not real passes: the first measured itself,
  and the second failed on 3 real defects. The gate bounded mounted widgets at 501 by condensing
  older lines into one block. It said plainly that it did "not prove" real-terminal behaviour,
  bracketed paste, or IME (input method editor, the system that composes non-Latin text) through an
  emulator (lines 340-351). The gate ran headless only.

What is different now (each item is verified below):

| August assumption | Now |
|---|---|
| Rewriting is cheap because Talaria is greenfield | Talaria is 61,002 lines of Python plus 91,227 lines of tests (`find talaria -name '*.py' \| xargs wc -l`). A rewrite is now expensive, and the operator has accepted that cost explicitly. |
| Textual is a healthy, actively released framework | Textualize the company wound down in 2025 (Will McGugan, "The Future of Textual", 2025-12-07, per search snippet; the page returned 404 when fetched). There have been **0 Textual releases since 2026-08-02**; the last was v8.2.8 on 2026-06-30 (`gh api repos/Textualize/textual/releases`). Toad, the Textual showcase agent client, is AGPL-3.0 and was last pushed 2026-05-26. |
| OpenTUI is Bun-only, version 0.4.5 | OpenTUI is now 0.6.6. Its `package.json` declares `engines: {bun: ">=1.3.14", node: ">=26.4.0"}`, but Node needs `--experimental-ffi` per opentui.com docs. There have been **24 releases since 2026-08-02**. |
| The Hermes contract is small enough to hand-encode | Hermes now publishes a generated OpenRPC 1.3.2 contract (OpenRPC is a JSON description format for JSON-RPC services) with 257 methods, 861 schemas, 79 notifications and 16 server-to-client requests. Between Talaria's pin `3236f4405` (2026-09-14) and `22063fc099` it gained 43 methods, gained 262 schemas, changed 72, and 234 commits touched `tui_gateway/contracts`. |
| Claude-Code-style workflows were not a requirement | They are now. Claude Code workflow scripts are "plain JavaScript, NOT TypeScript", have "No filesystem or Node.js API access", and `Date.now()`/`Math.random()` throw so a run can resume deterministically (Claude Code `workflow-authoring` skill reference, read this session). |
| The operator accepts a Python runtime | The operator now finds Talaria "slow and not clean" and is open to Rust. |

---

## 3. Measurements taken on this machine

All scripts and raw JSON results are in `$S/bench/` and `$S/codegen/`. Other agents were probably
running on this machine at the same time, so treat differences under about 10% as noise.

### 3.1 Process start: `--version` loop (hyperfine is not installed)

`$S/bench/startup.py` runs 3 warm-up runs, then 20 timed runs, and reads peak RSS (resident set
size, the memory actually held) from `/usr/bin/time -l`. **`--version` measures process start to
exit, not UI readiness**, and several tools answer it on a fast path before loading their UI.

| Command (stack) | Median ms | Min ms | Peak RSS MB | Notes |
|---|---:|---:|---:|---|
| `codex --version` 0.163.0 (Rust, ratatui) | 18.0 | 15.1 | 19.6 | 238 MB binary |
| `claude --version` 2.1.308 (TypeScript, Bun-compiled, own Ink-derived renderer) | 16.2 | 14.5 | 47.4 | 262 MB binary; probably a fast path |
| `opencode --version` v2.1.2 (TypeScript, Bun, OpenTUI) | 65.8 | 60.7 | 94.4 | 180 MB binary |
| `gemini --version` 0.57.0 (TypeScript, Node, Ink) | 928.0 | 857.5 | 239.1 | loads the whole bundle |
| `talaria --version` 0.6.5 (Python, Textual) | 177.9 | 160.9 | 41.3 | |
| `hermes --version` (Python launcher) | 2,658.8 | 1,755.6 | 85.3 | includes a passive update check (`hermes_cli/_startup_fast.py:187-230`); not representative of `hermes --tui` |
| baseline `node -e 0` (v22.22.3) | 34.1 | 30.2 | 40.2 | |
| baseline `bun -e 0` (1.3.14) | 29.7 | 26.8 | 25.1 | |
| baseline `python -c pass` (Talaria venv, CPython 3.13.15) | 23.3 | 21.4 | 17.4 | |
| `python -c 'import textual.app'` | 228.8 | 185.9 | 39.0 | Textual's import floor |
| `python -c 'import talaria.ui.app'` | 396.9 | 344.1 | 58.9 | |
| `node --check ui-tui/dist/entry.js` (Hermes's own Ink TUI bundle, 3.9 MB; parse only) | 67.9 | 65.6 | 63.0 | I did not launch `hermes --tui`: it would start Hermes processes, which this unit's rules forbid |

I identified each tool's stack from strings embedded in its binary
(`strings -n 6 <binary> | grep ...`):

- **opencode** contains `@opentui/core`, `@opentui/solid`, and `Bun v1.4.2`.
- **claude** contains Ink component names (`ink-box`, `ink-text`), `yogaNode` (Yoga is the layout
  engine), `altScreen`, and `Bun v1.4.4`.
- **codex** contains `ratatui-core-0.1.2`, `ratatui-widgets-0.3.2`, a crossterm git fork,
  `pulldown-cmark-0.10.3`, `syntect-5.3.0`, `starlark-0.14.2`, and `tokio-1.52.3`.

`python -X importtime` splits Talaria's 0.6 s import: about 285 ms is framework (textual 175, rich 42,
markdown_it 50, pygments 18) and 174 ms is Talaria's own modules (`$S/bench/talaria-importtime.txt`).
"Slow" is therefore partly Textual's floor and partly Talaria.

### 3.2 First frame in a pseudo-terminal (minimal "hello" app per framework)

`$S/bench/ptyfirstframe.py` spawns each app in a 120×40 PTY (pseudo-terminal). It answers the
capability queries a modern terminal would answer: DA1, mode 2026/2027 queries, kitty keyboard, and
colour and cursor reports. It then times how long the marker text takes to appear, samples RSS with
`ps` 0.3 s later, and runs 15 times after one warm-up. A scripted responder is not a real terminal
emulator. The app sources are in `$S/bench/hello/`.

| App | First frame median ms (min to max) | RSS MB | Binary size | Queries it sent |
|---|---:|---:|---:|---|
| Rust ratatui 0.30.2, release build | **3.7** (3.5 to 4.2) | **2.8** | 0.6 MB | none |
| Go Bubble Tea v2.1.0 | 22.6 (21.7 to 23.4) | 8.6 | 3.6 MB | mode 2026, 2027, kitty keyboard |
| TypeScript Ink 8.0.0, `bun build --compile` | 68.5 (65.8 to 79.5) | 60.6 | 64 MB | none |
| TypeScript Ink 8.0.0, Node 22 bundle | 108.0 (106.5 to 113.0) | 70.2 | n/a | none |
| Python Textual 8.2.8 (CPython 3.13) | 127.4 (122.2 to 133.0) | 38.2 | n/a | mode 2026, 2048 |
| TypeScript OpenTUI 0.6.6, `bun run` | 178.0 (176.3 to 192.9) | 94.9 | n/a | 8 queries |
| TypeScript OpenTUI 0.6.6, `bun build --compile` | 320.6 (297.5 to 361.7) | 94.1 | 74 MB | 8 queries |
| **Talaria 0.6.5 full UI**, `talaria replay --paused` on a 2,141-frame synthetic log | 346.9 (333.0 to 385.1) | 64.0 | n/a | mode 2026, 2048 |

Caveats:

- My first Ink build failed because Ink 8 resolves `react-devtools-core` at runtime. The rows above
  come from the rebuild with that package installed (`$S/bench/hello-results2.jsonl`); the failed
  rows in `hello-results.jsonl` are superseded.
- Talaria ran with `TALARIA_CONFIG_DIR=$S/bench/talaria-cfg`, so `~/.talaria` was not read or written.
- **Hypothesis, not verified:** OpenTUI's compiled binary is slower than `bun run` because the
  `--compile` binary has to unpack its embedded native library before it can start.

### 3.3 Markdown cost

Inputs: deterministic 16 KB and 64 KB assistant-style Markdown documents with headings, lists,
Python code fences, and tables (`$S/bench/md/doc16k.md`, `doc64k.md`).

**Parse only** (median of repeated full parses; "300 frames" means re-parsing a growing prefix
300 times, as a naive streaming client would):

| Parser | 16 KB full parse ms | 64 KB full parse ms | 300-frame naive stream, 64 KB, total ms |
|---|---:|---:|---:|
| Rust pulldown-cmark 0.13.5 | 0.029 | 0.114 | 17 |
| Go goldmark v1.8.6 + GFM (GitHub-flavoured Markdown) | 0.196 | 0.818 | 131 |
| JS markdown-it 15.0.3 on Bun 1.3.14 | 0.388 | 1.125 | 180 |
| JS marked 18.1.1 on Node 22 | 0.573 | 1.875 | 283 |
| Python markdown-it-py 4.2.0 (Textual's parser) | 7.239 | 29.259 | 4,462 |
| **JS marked 18.1.1 on Bun 1.3.14** | **9.47** | **99.5** | **10,912** |

The marked-on-Bun result was reproduced 3 times. It is about 50 times slower than the same library
on Node and grows faster than the input does (16 KB to 64 KB is about 10.5 times slower). **Hypothesis:**
a regular-expression backtracking cliff in Bun's JavaScriptCore engine. This is a real hazard for any
Bun-based client. OpenTUI itself depends on `marked` 17.0.1 (not the 18.1.1 measured here), and its
own streaming path measured fast (next table), probably because it re-lexes only the trailing block.
That reason is inferred and was not read from OpenTUI's source.

**Full render per frame**:

| Stack (method) | 16 KB per frame ms | 64 KB per frame ms |
|---|---:|---:|
| OpenTUI 0.6.6 `MarkdownRenderable` in a sticky-bottom ScrollBox, streaming mode, `TestRenderer` | **1.1** (avg) | **1.6** (avg) |
| Rust ratatui 0.30.2 + tui-markdown 0.3.12 (syntect highlighting), naive full re-render scrolled to bottom | 5.4 (worst) / 2.65 (avg) | 21.4 (worst) / 10.7 (avg) |
| Go glamour v2.0.1 (charm.land), naive full render to an ANSI string | 23.8 (worst) / 12.4 (avg) | 89.9 (worst) / 47.6 (avg) |
| Textual 8.2.8 `Markdown.append()` (its incremental path), headless `run_test`, minus a 21.4 ms per-frame `pilot.pause()` baseline | ≈37 (avg) | ≈123 (avg) |
| Textual 8.2.8 `Markdown.update()` (full replace), same method | ≈209 (avg) | not run |

Caveats:

- These rows are not identical work. OpenTUI's streaming mode re-parses only the trailing block;
  I confirmed that the final captured frame shows the document's tail. Its tree-sitter highlighting
  runs asynchronously and is not included in its numbers.
- The Rust and Go rows deliberately use the worst strategy (re-render everything every frame). A
  real client caches finished blocks, as Codex's `markdown_stream.rs` and Toad do.
- The Textual rows use Textual's own incremental API and are still over budget at 16 KB.
- Current Talaria does not use the stock widget path. It mounts one widget per line under a 500-line
  cap and imports from the private module `textual.widgets._markdown` (`talaria/ui/blocks.py:95`).
  This benchmark measures the framework, not Talaria's workaround.

### 3.4 Code generation from Hermes's contract (`$S/codegen/`)

Input: `apps/shared/src/gateway-contract.openrpc.json` at `22063fc099`, sha256 `11644a40…d52`.
`mkschema.py` lifts `components.schemas` into a JSON Schema 2020-12 document with `$defs`, and a
second variant whose root references every definition. The schemas are Pydantic-generated and use
`anyOf` 1,964 times, `$ref` 1,119, `additionalProperties` 902, `enum` 91, `const` 26, and OpenAPI-style
`discriminator` 10 times. 22 of the `anyOf` sites include `{}` ("any value").

| Generator (language) | Result | Output | Compiles? | Fidelity notes |
|---|---|---|---|---|
| Hermes's own generator, `scripts/gen_gateway_contracts.py` (TypeScript) | upstream artifact | 7,132 lines | `tsc --strict --noEmit` passes on the file alone (1.7 s) | Hermes regenerates it, and `tests/tui_gateway/contracts/test_generated.py` fails CI when it is stale (file header lines 1-3) |
| quicktype 26.0.0 (TypeScript) | success, 3.8 s | 8,907 lines, 908 types | `tsc --strict` passes | 6 placeholder names such as `Purple…`/`Fluffy…` |
| **cargo-typify 0.8.0 (Rust)** | **failed, then success with a patch** | 101,956 lines, 925 types (with builders) | `cargo check` passes; incremental re-check 4.8 s | See the two problems below |
| quicktype 26.0.0 (Rust) | success | 14,090 lines, 916 types | `cargo check` passes; incremental 3.4 s | The discriminated union `Owner` is flattened into one struct with optional fields, so the type no longer enforces it; 15 placeholder names |
| go-jsonschema v0.24.1 (Go) | **failed** | — | — | Fails on `anyOf: [{}, null]` (`SessionLiveInfo.update_behind`). After normalising those 22 sites it fails on a nested `anyOf: [oneOf{…}, null]` (`GoalSnapshot.wait_barrier`) |
| quicktype 26.0.0 (Go) | success | 9,194 lines, 918 types | `go vet` passes | Unions flattened; 31 placeholder names |
| datamodel-code-generator 0.86.0 (Python, Pydantic v2) | success, 4.7 s | 9,698 lines, 899 classes | imports; discriminated union validates to `SessionOwner` | Best fidelity: `Field(discriminator='type')`. But importing all 899 models takes **≈310 ms** (5 runs, Pydantic 2.14.1), and `extra='forbid'` appears 746 times |
| quicktype 26.0.0 (Python) | success | 29,883 lines | not run | — |

The two typify problems:

- **Defaults.** typify rejected the document with "value does not conform to the given schema"
  (`typify-impl-0.8.0/src/convert.rs:1097,1132`; the message itself is at `lib.rs:49`). Bisecting
  over the 548 schemas that carry defaults pinned it to 3: `DisplayLease` (a `type: null` field with a
  default), `MessageCompletePayload` (`anyOf: [string, {}]` with a default), and `ProviderUsageWindow`.
  For `ProviderUsageWindow`, the field passes in isolation, and **likely cause (inferred, not
  confirmed in typify's source):** five inline enums are all titled `Scope` with different value
  sets, so typify's title-based naming collides.
- **Unknown fields.** typify emits `#[serde(deny_unknown_fields)]` 746 times, from
  `additionalProperties: false`. A client built that way rejects any frame where Hermes has added a
  field. Datamodel-codegen's `extra='forbid'` has the same problem. Whatever the stack, the client
  generator must strip `additionalProperties: false` first.

A further typify note: `rustfmt` is found through `CARGO_HOME`, so in a non-standard `CARGO_HOME` it
needs `RUSTFMT=/opt/homebrew/bin/rustfmt`. Generated discriminated unions become
`#[serde(untagged)]` enums (for example `Owner` at `$S/codegen/rs-typify/src/lib.rs:10617-10624`).

**Reusing Hermes's TypeScript client, verified.** I copied 4 files from `apps/shared/src`:

| File | Lines |
|---|---:|
| `gateway-contract.generated.ts` | 7,132 |
| `gateway-events.ts` | 85 |
| `json-rpc-channel.ts` | 613 |
| `json-rpc-gateway.ts` | 842 |

- They type-check standalone with `tsc --strict --module nodenext`.
- `$S/codegen/smoke.ts` runs `JsonRpcGatewayClient` unmodified on Bun 1.3.14 against a `Bun.serve`
  mock. It printed
  `{"state":"open","result":{"agents":[{"id":"a1"}]},"events":["gateway.ready","message.delta"]}`.
- The package is `"private": true` (`apps/shared/package.json`) and is not published to npm. Reuse
  therefore means vendoring it with a sync script. The license is MIT
  (`<hermes>/LICENSE`).
- The client already implements reconnect replay (`session.events.since`), sequence watermarks,
  heartbeats, `client.capabilities` advertisement on `gateway.ready`, and server-to-client request
  handling (`json-rpc-channel.ts:444-540`). Upstream tests it with `json-rpc-gateway-replay.test.ts`
  (33.7 KB).

### 3.5 Distribution checks

- **Go:** `GOOS=linux|windows GOARCH=amd64|arm64 CGO_ENABLED=0 go build` produced Linux x86-64,
  Linux ARM64, and Windows PE binaries from macOS with no extra setup.
- **Bun:** `bun build --compile --target=bun-linux-x64` produced a Linux ELF for Ink with no extra
  setup. For **OpenTUI** it failed (`Could not resolve: "@opentui/core-linux-x64"`) until I ran
  `bun install --os='*' --cpu='*'` to fetch every native package. It then produced a Linux ELF. I did
  not execute that binary.
- **Rust:** Homebrew Rust 1.98.1 has only the `aarch64-apple-darwin` standard library installed, so
  local cross-compiling needs rustup targets or `cargo-zigbuild`. Release builds would normally run
  per operating system in CI.

---

## 4. Evidence by criterion

### Startup time and memory
Measured in sections 3.1 and 3.2. Ranking: Rust (3.7 ms / 2.8 MB) ≫ Go (22.6 ms / 8.6 MB) >
Ink-on-Bun (68.5 ms / 60.6 MB) > Textual (127 ms / 38 MB) > OpenTUI (178 to 321 ms / 94 MB). Codex's
rewrite announcement gave "no runtime garbage collection, resulting in lower memory consumption" as a
reason ([InfoQ](https://www.infoq.com/news/2025/06/codex-cli-rust-native-rewrite),
[devclass](https://devclass.com/2025/06/02/node-js-frustrating-and-inefficient-openai-rewrites-ai-coding-tool-in-rust/)).

### Cost of rendering streaming Markdown
Measured in section 3.3. Rust and OpenTUI have the most headroom. Go needs block caching, and
Textual's stock widget exceeds a 33 ms frame at 16 KB. Will McGugan describes Toad's approach as
re-parsing only the last block ([mathspp summary of McGugan's post, 2025-07-24](https://mathspp.com/link-blog/willmcgugan-github-io-streaming-markdown)).
Codex has `markdown_stream.rs`, `markdown_render.rs` and a `streaming/` module under `codex-rs/tui/src`
(`gh api repos/openai/codex/contents/codex-rs/tui/src`, commit `b4ad33d1a3`, 2026-10-22).

### Rendering model: inline with native scrollback, or alternate screen

There are two ways to run a terminal app. An inline app draws below the prompt and lets finished
output scroll into the terminal's own history. An alternate-screen app takes over the whole window
and manages its own scrolling.

- **Codex (Rust)** writes finished history into native scrollback with escape sequences. The module
  header says "Codex uses the terminal scrollback itself for finalized chat history, so inserting a
  history cell is an escape-sequence operation rather than a normal ratatui render"
  (`codex-rs/tui/src/insert_history.rs:1-5`, via `SetScrollRegion` at lines 175 and 205). It wraps
  frames in synchronized updates (`SynchronizedUpdate` in `codex-rs/tui/src/tui.rs`, from GitHub code
  search). ratatui itself provides `Viewport::Inline` and `insert_before`
  (`ratatui-core/src/terminal/inline.rs`).
- **Claude Code** rewrote its renderer to diff cells while keeping React. It stayed on the main
  screen because, in Thariq Shihipar's words, "We value this native experience a lot" and "our bar is
  quite high" for alternate screen
  ([steipete, Claude Code 2.0.72](https://steipete.me/posts/2025/signature-flicker);
  [letsdatascience](https://letsdatascience.com/news/anthropic-rewrites-claude-code-terminal-rendering-76034b52)).
  An opt-in fullscreen mode exists behind `CLAUDE_CODE_NO_FLICKER=1` ([vanja.io](https://vanja.io/claude-code-no-flicker/)).
- **Gemini CLI** moved to the alternate screen buffer by default in v0.15.0 (Google blog,
  2025-11-13, [developers.googleblog.com](https://developers.googleblog.com/en/making-the-terminal-beautiful-one-pixel-at-a-time/)).
  Copying text then requires Ctrl-S to leave mouse mode.
- **OpenTUI** offers `ScreenMode = "alternate-screen" | "main-screen" | "split-footer"`
  (`@opentui/core/renderer.d.ts:66`). Steipete reports it "doesn't work in the standard macOS Terminal
  below macOS 26 or in GNOME Terminal", with scroll and search problems. That is unverified against
  0.6.6.
- **Bubble Tea v2** has `Println` for scrollback and negotiates mode 2026, which it did query in my
  PTY run.
- **Textual** defaults to the alternate screen and has `App.run(inline=True)`
  (`textual/app.py:2224`). I did not verify whether inline mode commits content to native scrollback.
- **Hermes's own TUI** maintains a private Ink fork, `ui-tui/packages/hermes-ink`, at 27,451 lines.

### Text input quality

Terms used here: IME is the input method editor that composes non-Latin text; the kitty keyboard
protocol is an extension that reports keys unambiguously; bracketed paste marks pasted text so it
arrives as one event.

- **Rust:** crossterm has `PushKeyboardEnhancementFlags` (kitty protocol) and `EnableBracketedPaste`.
  Codex wrote its own editor, `bottom_pane/textarea.rs` at 173,600 bytes, plus `paste_burst.rs` (paste
  detection where bracketed paste is unavailable) and `tui/keyboard_modes.rs`. Off-the-shelf options
  are `ratatui-textarea` 0.9.3 (2026-10-05) and the unmaintained `tui-textarea` 0.7.0 (2024-10-22).
  Images: `ratatui-image` 11.1.0 handles sixel, kitty, iTerm2 and half-block fallbacks. Codex has
  `transcript_image.rs`.
- **Go:** Bubble Tea v2 queried the kitty keyboard protocol and mode 2027 in my PTY run. Bubbles v2
  has textarea and textinput. Images are low-level only, via `github.com/charmbracelet/x/ansi/kitty`.
- **TypeScript/OpenTUI:** it has `Textarea`, `Input`, `Select`, `TabSelect`, and `Image` renderables
  (`node_modules/@opentui/core/renderables/`, `image.d.ts`) and sent 8 capability queries including
  kitty. **Ink:** its reconsideration-era features include `use-paste` and IME/cursor handling
  (reconsideration doc, lines 312-319).
- **Python/Textual:** it enables the kitty protocol unconditionally (`textual/drivers/linux_driver.py:285-292`)
  and bracketed paste via mode 2048. Images come from the third-party `textual-image` 0.14.2. The gate
  did not exercise IME through a real emulator.
- **No stack has a verified IME advantage.** IME composition happens in the terminal emulator; the
  app's job is to place the cursor correctly, which all four frameworks expose.

### Syntax highlighting and diffs
- Codex uses `syntect` 5 + `two-face` 0.5 and has its own `diff_render.rs` / `diff_model.rs`.
- OpenTUI ships `Code` and `Diff` renderables backed by `web-tree-sitter` and the `diff` 9.0.0 package.
- Go has chroma v2.27.0; diff views would be hand-built.
- Python has pygments, and Talaria already has `talaria/ui/diff_viewer.py`.

### Async JSON-RPC over WebSocket and standard input/output
- Rust: tokio + tokio-tungstenite 0.30.0. Codex patches forks of `tokio-tungstenite` and `tungstenite`
  (`codex-rs/Cargo.toml:665-667`). JSON-RPC framing with serde is small, but the Hermes channel
  semantics (replay barrier, heartbeats, server requests) would have to be ported and kept in sync.
- Go: goroutines plus `coder/websocket` v1.8.15.
- TypeScript: global `WebSocket` on Bun and Node 22+. Hermes's client takes a pluggable
  `JsonRpcTransport`, and its comment says the event hub is "Shared by the WebSocket client below and
  the Ink TUI's stdio" (`json-rpc-gateway.ts:77`).
- Python: asyncio + `websockets` 15.0.1 (Talaria's current transport).

### Code generation from Hermes's OpenRPC
Measured in section 3.4. Ranking: TypeScript (reuse upstream's output; nothing to generate) >
Python (best generator fidelity, slow import) > Rust (works after a small preprocessing step) > Go
(the generator failed; quicktype lost the union types).

### Embedded scripting runtime for Claude-Code-style workflows
Claude Code workflow scripts are sandboxed plain JavaScript using `agent()`, `parallel()` and
`pipeline()`, with deterministic replay. So the natural target is an embedded JavaScript engine with
no ambient filesystem access.

- **Rust:** `rquickjs` 0.14.0 has a `futures` feature: "Rust futures can be passed to JS as ES6
  Promises" ([docs.rs](https://docs.rs/rquickjs/latest/rquickjs/)). Other options are `boa_engine`
  0.22.0 (pure Rust), `deno_core` 0.413.0 (V8, heavy), `mlua` 0.12.2, and `starlark` 0.14.2. Codex
  embeds `v8 = "=152.2.0"` in `codex-rs/code-mode-runtime` and Starlark in `codex-rs/execpolicy`.
- **Go:** `goja` added async/await in PR #464 (merged 2022-12-24) and `for await` in PR #740
  (2026-09-30), though the README still calls it "ECMAScript 5.1(+)". `go.starlark.net` and
  `gopher-lua` also exist.
- **TypeScript:** the scripts' own language. A real sandbox still needs an isolate or a WebAssembly
  QuickJS (for example `quickjs-emscripten`), because `node:vm` is not a security boundary.
- **Python:** Python as the script language has no sandbox; embedding JavaScript would mean V8 or
  QuickJS bindings.
- **Open architectural point:** if workflows must keep running after the client exits (the operator
  wants agents that keep working in the background), the runtime lives in a daemon or in Hermes, not
  in the terminal UI. Hermes exposes `subagent.*`, `spawn_tree.*` and `delegation.*` but no workflow
  runtime (method survey of the contract).

### Distribution
- Rust: one small binary per target (0.6 MB hello app). Codex is 238 MB, and it links V8 for
  `codex-rs/code-mode-runtime`; that V8 explains most of the size is a hypothesis. Homebrew is easy.
  Cross-compiling needs CI runners or zig.
- Go: the best story; cross-compiling is trivial (verified).
- TypeScript on Bun: single-file binaries of 61 to 74 MB for hello apps (Claude Code is 262 MB,
  OpenCode 180 MB). OpenTUI adds per-platform native packages (cross-compile verified after
  `--os/--cpu` install). On Node it needs `--experimental-ffi`.
- Python: `uv tool install`; no single binary without PyInstaller or Nuitka.

### How reliably coding agents write each stack
- **Compile-time guidance:** Rust (exhaustive `match` over enums, ownership checks) > TypeScript
  `--strict` > Go (no sum types; nil and zero values) > Python with mypy.
- **Benchmarks disagree.** On SWE-bench Multilingual (Claude 3.7 Sonnet + SWE-agent, about 43 tasks
  per language), Rust resolved **58.14%** (highest), JS/TS 34.88%, and Go 30.95%
  ([swebench.com/multilingual](https://www.swebench.com/multilingual.html)). ByteDance's
  Multi-SWE-bench found TypeScript and JavaScript lowest and Go and Rust far below Python
  ([arXiv 2504.02605](https://arxiv.org/pdf/2504.02605)). Rust translation studies find compile
  errors cause 92.3% of failures ([RustRepoTrans, arXiv 2411.13990](https://arxiv.org/pdf/2411.13990)).
  That is consistent with Rust surfacing errors at compile time, where an agent can loop on them.
  **No controlled study compares agent velocity on terminal-UI work across these four stacks.**
- **Existence proofs, both with frontier models:** Codex's team estimates Codex "writes more than
  90% of the app's code" and chose Rust "to eliminate a class of errors with things like strong
  typing" ([Pragmatic Engineer, 2026-02-17](https://newsletter.pragmaticengineer.com/p/how-codex-is-built)).
  Claude Code's lead has said 80 to 90% of Claude Code is written by Claude (secondary reports, for
  example [Ernest Chiang](https://www.ernestchiang.com/en/posts/2026/the-man-who-built-claude-code-was-changed-by-it/)).
  Neither proof involves moderate-capability agents.
- **Ecosystem churn since 2026-08-02** (release counts from `gh api .../releases`): ratatui 0, Bubble
  Tea 3 (v2.0.9 to v2.1.0), Ink 1 (major v8.0.0), Textual 0, OpenTUI **24**. Churn makes an agent's
  training-time API knowledge stale. OpenTUI is the most exposed: it is pre-1.0 and its
  `SyntaxStyle.create()` now demands an explicit resource owner, which my first benchmark attempt hit.
  Bubble Tea v2 moved to the `charm.land/...` import path.
- **Test tooling:**
  - Codex uses `insta` snapshots plus a `vt100` terminal emulator in tests (`codex-rs/tui/Cargo.toml`).
  - ratatui has `TestBackend`.
  - OpenTUI has `createTestRenderer` with `captureCharFrame` (`@opentui/core/testing`).
  - Bubble Tea has golden terminal-byte tests.
  - Textual has `run_test`/`Pilot`, but the gate recorded two traps that no lint catches: `App`
    namespace collisions and `Paste` events inserting twice (gate doc, lines 353-372).

### TypeScript's reuse advantage, quantified
- `apps/shared/src` has 12,030 non-test lines, 18 exported modules (slash-command parsing, fuzzy
  matching, reasoning effort, reconnect backoff, skin and theme presets), and the client from 3.4.
- Hermes Desktop (Electron + React) has `store/` at 47,895 lines, `api/` at 4,488, and `lib/` at
  37,604. But 74 of 193 store files and 9 of 14 api files touch `window`, `document`, `localStorage`,
  or the desktop bridge (grep count), so they are reference material, not drop-in code.
- ADR-0003 (Talaria's architecture decision record that Hermes's TUI documents behaviour rather than
  being a source tree to translate) still applies to `ui-tui`. It does not forbid vendoring a
  versioned client library.

---

## 5. Scored comparison

Scores run 1 (poor) to 5 (best) and are my judgment from the evidence above; weights total 100. The
table's purpose is to make the tradeoffs explicit, not to stand in for the evidence.

| Criterion (weight) | Rust + ratatui | TS + OpenTUI (Bun) | Go + Bubble Tea v2 | TS + Ink | Python + Textual |
|---|:-:|:-:|:-:|:-:|:-:|
| Streaming render headroom (15) | 5 | 5 | 3 | 2 | 1 |
| Startup and memory (10) | 5 | 2 | 5 | 3 | 2 |
| Inline scrollback, flicker control (10) | 5 (Codex proves it) | 3 | 4 | 3 | 2 |
| Text input: editor, kitty, paste, images (10) | 4 | 4 | 4 | 3 | 4 |
| Highlighting and diffs (5) | 5 | 4 | 3 | 3 | 3 |
| Async JSON-RPC over WS and stdio (5) | 4 | 5 | 4 | 5 | 4 |
| Contract codegen / Hermes reuse (15) | 3 | 5 | 2 | 5 | 4 |
| Embedded workflow scripting (5) | 4 | 5 | 4 | 5 | 2 |
| Distribution (5) | 5 | 3 | 5 | 3 | 2 |
| Agent authoring reliability (15) | 3 | 3 | 4 | 4 | 3 |
| Widget breadth for config and fleet screens (5) | 2 | 4 | 4 | 2 | 5 |
| **Weighted total (out of 100)** | **81** | **78** | **73** | **69** | **56** |

**Sensitivity.** The top two are within noise of my own judgment:

- If startup/memory drops from 10 to 5 and contract codegen/reuse rises from 15 to 20, TypeScript +
  OpenTUI wins 81 to 79.
- If agent authoring reliability rises from 15 to 25, taken from streaming render (15 to 5), Rust
  still leads at 77, Go rises to 75, and TypeScript + OpenTUI falls to 74.

Python + Textual loses under every weighting I tried, because it is last on the two
performance criteria the operator complained about.

---

## 6. Recommendation in detail

**Stack:**

| Layer | Choice |
|---|---|
| Language | Rust (stable; edition 2024 when the toolchain allows) |
| Renderer | `ratatui` 0.30.2 + `crossterm` 0.29, inline viewport with history inserted into native scrollback (port Codex's `insert_history.rs`) and full-screen overlays only for transcript, pager, and fleet views |
| Async runtime | `tokio` with a single application event channel (Codex's `AppEvent` pattern, `codex-rs/tui/src/app_event.rs:297`), so every agent writes state changes one way |
| Transport | `tokio-tungstenite` (WebSocket) and tokio child-process pipes (standard input/output), behind one transport trait |
| Markdown | `pulldown-cmark` 0.13 with block-level caching; finished blocks are rendered once and committed to scrollback |
| Highlighting | `syntect` + `two-face` (Codex's choice) at first; `tree-sitter-highlight` 0.27 only if grammar accuracy demands it |
| Editor | start from Codex's `textarea.rs` and `chat_composer.rs` (Apache-2.0: keep the NOTICE) rather than `ratatui-textarea` |
| Images | `ratatui-image` 11 (kitty, iTerm2, sixel, half-block fallback) |
| Protocol types | a separate `hermes-contract` crate generated by `cargo typify` from the OpenRPC, after a checked-in preprocessing script that (a) strips `additionalProperties: false`, (b) drops the 3 failing defaults, and (c) emits one Rust enum over the 79 `x-notifications` and 16 `x-server-requests`. CI regenerates it and fails on drift, mirroring Hermes's own `test_generated.py` |
| Workflow scripts | `rquickjs` with the `futures` feature, exposing only `agent()`, `parallel()`, `pipeline()`, `log()`, `phase()`, `args`, with no filesystem access and no `Date.now`/`Math.random`, matching Claude Code's workflow contract |
| Tests | `insta` snapshots, `ratatui::backend::TestBackend`, and a `vt100` emulator for scrollback and resize, as Codex does; plus replay from Talaria's existing frame-log format, which the August analyses already made language-neutral |
| Release | `cargo-dist` or GitHub Actions per operating system → Homebrew tap |

**Mitigations for moderate-capability agents.** These make Rust the safer bet rather than the harder
one:

1. Check in a `docs/agents/` guide with the 5 or 6 patterns every change uses: event enum,
   reducer, render function, snapshot test, transport message. Pin ratatui and crossterm versions in
   it so agents do not reach for removed APIs.
2. Keep the generated contract (≈102k lines) in its own crate so the edit-check loop stays small;
   whole-crate incremental `cargo check` was 3.4 to 4.8 s.
3. Forbid `unsafe`, `Rc<RefCell<…>>` webs, and lifetimes in application state; everything is owned
   data passed through channels. Clippy runs with `-D warnings`.
4. Prove the agent-velocity assumption before committing (section 8).

---

## 7. The strongest counter-argument

**"Build it in TypeScript on OpenTUI, because the protocol is the product and Hermes already wrote
the client."**

- The gateway contract is the fastest-moving surface in this system: +43 methods, +262 schemas, and
  72 changed schemas in five weeks.
- In TypeScript, tracking Hermes is a file copy. The upstream client already encodes the subtle parts
  (replay after reconnect, sequence watermarks, heartbeat liveness, capability advertisement,
  server-request deadlines such as `APPROVAL_RESPOND_TIMEOUT_MS = 300_000` at
  `json-rpc-gateway.ts:49`). Upstream tests it, and I verified it runs unmodified on Bun.
- In Rust, every one of those behaviours must be re-implemented and then kept in sync by agents. That
  is exactly the class of quiet protocol bug the August gate caught three times in Talaria's own
  reconciliation code.
- TypeScript is the language moderate agents have seen most. The workflow scripts are JavaScript
  already. OpenTUI's measured streaming Markdown (1.1 to 1.6 ms per frame) is faster than my naive
  Rust path. Its widget set (Markdown, Code, Diff, Textarea, Select, TabSelect, ScrollBox, Image,
  EmbeddedTerminal) covers a Hermes-Desktop-like configuration UI better than ratatui's.
- OpenCode, with 219,813 GitHub stars, proves the stack at scale.
- Against all that, Rust's advantage is mostly startup (about 175 to 315 ms) and memory (about
  90 MB). Users already accept those numbers from Claude Code and OpenCode.

Why I still rank Rust first, narrowly:

- OpenTUI is pre-1.0 with 24 releases in 11 weeks; that churn will repeatedly break agent-written
  code.
- It ties the client to Bun, which showed a measured 50× slowdown against Node on `marked` 18.1.1.
  OpenTUI depends on `marked` 17, though its own streaming path measured fast.
- Its terminal-compatibility problems were reported but are unresolved in what I could verify.
- Its first frame is slower than Textual's, so it would not fix the "slow to start" half of the
  operator's complaint.
- Codex provides a complete, permissively licensed Rust reference for nearly every terminal problem
  on the Claude-Code-parity list.

That ranking is a judgment call, not a measured conclusion.

---

## 8. Evidence that would change the recommendation

1. **A head-to-head agent spike (the decisive test).** Give the same moderate-tier agent (for example
   Sonnet- or Haiku-class) the same vertical slice in Rust/ratatui and in TypeScript/OpenTUI, with
   identical acceptance tests:
   - connect, stream a turn, show a tool call with a diff, answer an approval, reconnect with replay;
   - replay a recorded frame log.

   Count compile-fix iterations, human interventions, stale-API errors, and wall-clock time to green.
   **If Rust needs more than about twice the interventions, switch to TypeScript + OpenTUI.**
2. **Hermes publishes `@hermes/shared` as a supported, versioned npm package** (it is `private: true`
   today). That would turn vendoring into a supported dependency and favour TypeScript.
3. **OpenTUI reaches 1.0** with a stable API, Node support without `--experimental-ffi`, and verified
   behaviour in macOS Terminal and GNOME Terminal. That favours TypeScript.
4. **Hermes adds a server-side workflow runtime** (its backend is Python). Client-side scripting then
   stops being a criterion, and the decision becomes purely about the terminal UI.
5. **A real-terminal first-frame and streaming test of Codex versus OpenCode versus Claude Code**
   (not `--version`) shows the startup gap is invisible in practice. Removing the startup criterion
   flips the ranking toward TypeScript.
6. **Typify cannot keep up with contract churn** (repeated new failure classes after preprocessing).
   That favours TypeScript, or Python's datamodel-code-generator.

---

## 9. Open questions

- Where do long-running workflows execute: a Talaria daemon, a Hermes plugin, or the terminal UI?
  This decides whether the embedded JavaScript engine belongs in the client at all.
- Is the operator's "slow" about start-up, streaming, scrolling long transcripts, or reconnects? Only
  streaming and start-up were measured here.
- How much of Codex's `codex-rs/tui` (1,139 `.rs` files, 17.2 MB of source at `b4ad33d1a3`) is
  separable from Codex's own protocol? I read its structure, not its coupling.
- Does inline mode stay usable inside tmux and Zellij for the Hermes-Desktop-like multi-agent views?
  Codex's `insert_history.rs:51-54` has a separate strategy for Zellij.

## 10. Reproduce

Startup timing:
- `python3 -I $S/bench/startup.py`; results in `$S/bench/startup-results.jsonl`; formatter
  `$S/bench/fmt.py`.

First frame in a pseudo-terminal:
- `RUNS=15 python3 -I $S/bench/ptyfirstframe.py $S/bench/cases.json` (and `cases2.json`); results in
  `$S/bench/hello-results*.jsonl`; formatter `$S/bench/fmt2.py`.

Markdown:
- Parse: `$S/bench/md/rs/target/release/mdbench $S/bench/md`, `$S/bench/md/go/mdbench $S/bench/md`,
  `python $S/bench/md/py_bench.py $S/bench/md`, `bun|node $S/bench/md/js/bench.mjs $S/bench/md`.
- Render: `$S/bench/md/rsr/target/release/mdrender`, `$S/bench/md/gor/gor`,
  `bun run $S/bench/hello/otui/mdbench2.ts $S/bench/md`,
  `<talaria venv python> $S/bench/md/textual_md_bench2.py <doc> append|update|none`.

Code generation:
- Schema prep: `$S/codegen/mkschema.py`.
- typify bisection: `$S/codegen/bisect_typify.py`.
- Generated crates: `$S/codegen/rs-typify`, `$S/codegen/rs-quicktype`.
- Go: `$S/codegen/go-qt`, `$S/codegen/go-js`.
- Python: `$S/codegen/py/contract_models.py`.
- Client smoke test: `bun run $S/codegen/smoke.ts`.

## Sources (web)

- https://steipete.me/posts/2025/signature-flicker
- https://letsdatascience.com/news/anthropic-rewrites-claude-code-terminal-rendering-76034b52
- https://vanja.io/claude-code-no-flicker/
- https://developers.googleblog.com/en/making-the-terminal-beautiful-one-pixel-at-a-time/
- https://www.swebench.com/multilingual.html
- https://arxiv.org/pdf/2504.02605
- https://arxiv.org/pdf/2411.13990
- https://newsletter.pragmaticengineer.com/p/how-codex-is-built
- https://www.infoq.com/news/2025/06/codex-cli-rust-native-rewrite
- https://devclass.com/2025/06/02/node-js-frustrating-and-inefficient-openai-rewrites-ai-coding-tool-in-rust/
- https://www.ernestchiang.com/en/posts/2026/the-man-who-built-claude-code-was-changed-by-it/
- https://mathspp.com/link-blog/willmcgugan-github-io-streaming-markdown
- https://www.textualize.io/blog/the-future-of-textual (search snippet only; 404 on fetch)
- https://opentui.com/docs/getting-started/
- https://docs.rs/rquickjs/latest/rquickjs/
- https://github.com/dop251/goja/pull/464 and https://github.com/dop251/goja/pull/740
- GitHub API reads of openai/codex (`b4ad33d1a3`), charmbracelet/crush (`LICENSE.md`: FSL-1.1-MIT),
  Textualize/toad (AGPL-3.0), and release lists for ratatui, bubbletea, ink, textual, opentui
