# Python fallback presentation layer: prompt_toolkit assessed against the gate criteria

Status: `draft`
Authority: `reference`
Date: 2026-08-02

## Purpose

This document discharges PC8 (Planning closure item 8) and closes the `docs/engineering-journal/QUEUED.md`
P0 titled "Identify and assess a Python fallback presentation layer." KTD12 (Key Technical Decision 12,
[`docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md`](../plans/2026-08-02-talaria-v0-1-prototype-plan.md))
names `prompt_toolkit` as the fallback presentation layer if U5's Textual validation gate fails, with `urwid`
recorded as the secondary candidate. Unit U4 requires this document to assess `prompt_toolkit` against the
same criteria the gate measures, **to plausibility depth** — the standing bar from QUEUED.md is "enough to
know it exists and is plausible, not a full comparative analysis." This is not a recommendation to switch;
Textual remains the primary candidate and U5 has not run. This document exists so that if U5 fails, the next
step is an evaluated one instead of a dead end.

## Method

The gate's criteria list (`docs/engineering-journal/QUEUED.md`, "Run the Textual validation gate") is:
coalesced streaming, bounded transcript mounting, scroll anchoring, deterministic `run_test()`/`Pilot`
behavior, selected pseudo-terminal behavior, framework-independent domain state, strict typing and linting,
and clean `uv tool install` launch. Two of those — framework-independent domain state and strict typing and
linting — are properties of `talaria/domain` and the project's own tooling, not of the UI framework choice;
ADR-0002 already guarantees them regardless of which presentation layer sits above the boundary. They are
not re-assessed here because they do not vary with this decision. "Selected pseudo-terminal behavior" is a
Textual-specific rendering-driver detail without a direct prompt_toolkit analogue and is folded into the
headless test story below, where the equivalent question is what a scripted, non-interactive test harness
looks like.

That leaves five criteria, matching U4's own approach text in the plan: bounded long-transcript strategy,
streaming coalescing, multi-line editing and bracketed paste, headless/deterministic test story, and install
cleanliness under `uv tool install`.

A minimal spike was run rather than relying on documentation alone, per U4's "a minimal spike is permitted
to settle a criterion a document cannot" allowance. `prompt_toolkit==3.0.53` and `urwid==4.0.8` were
installed into a scratch virtual environment with `uv venv` / `uv pip install` (no project files touched),
and specific claims below were checked by reading the installed source and exercising the API directly,
not from memory. Where a claim rests on prior knowledge rather than something exercised in the spike, it is
labeled as such.

## Candidates

**Primary: `prompt_toolkit` 3.0.53.** Pure-Python full-screen terminal toolkit with its own asyncio event
loop, used as the editing/rendering engine inside IPython and pgcli — both multi-line, paste-heavy,
long-running interactive tools, which is the adoption evidence KTD12 cites. Single runtime dependency:
`wcwidth`. Confirmed in the spike: `uv pip install prompt_toolkit urwid` into a clean venv resolved
`prompt_toolkit`, `wcwidth`, `urwid`, and `typing_extensions` — four packages total, installed in 10ms — and
no compiled (`.so`) artifacts were present in any of their installed site-packages directories on macOS
arm64/CPython 3.12, meaning no C-extension build step is in play for either candidate on this platform.

**Secondary: `urwid` 4.0.8.** Older (predates asyncio; asyncio support is an adapter, `urwid.aio`, over a
`urwid.MainLoop` model built around a synchronous event-loop abstraction) curses-style widget toolkit. It is
recorded here per KTD12 as the secondary candidate and is not assessed against all five criteria at the same
depth as `prompt_toolkit` — the plan's bar is naming and lightly checking a fallback for the fallback, not a
three-way comparison. Where it materially affects a criterion below, that is noted.

## Per-criterion assessment

### 1. Bounded long-transcript strategy

**Verdict: plausible, and structurally simpler than Textual's version of the same problem.**

`prompt_toolkit` has no persistent widget tree. A scrollable region is a `Window` wrapping a `UIControl`;
its `create_content(width, height)` method is called on each render and must return a `UIContent`. Read from
the installed 3.0.53 source in the spike:

```python
class UIContent:
    def __init__(
        self,
        get_line: Callable[[int], StyleAndTextTuples] = ...,
        line_count: int = 0,
        ...
    ): ...
```

`get_line` is a lazy, index-addressed callable — the renderer asks for line *i* only when line *i* is inside
the visible viewport. There is no per-transcript-entry object that gets mounted, retained, and later evicted;
rendering is pull-based over whatever data structure the domain projection already holds. This means KTD14's
condensed-older-history strategy (cap the visible/expanded set, collapse the remainder into one block) maps
directly onto `get_line`'s index space, and the framework itself never accumulates mount overhead the way
Textual's one-widget-per-entry model does — the thing U5's gate measures (mounted widgets ≤ 600, RSS growth
< 300 MB) has no `prompt_toolkit`-side equivalent to measure, because there are no mounted per-entry widgets
to count. The domain-side transcript still grows without eviction exactly as KTD14 already accepts for
Textual (that half of the bound is a domain decision, not a UI-framework one, and does not change with this
choice). No built-in "virtual list" ships — the windowing/index logic must still be hand-written — but it is
the same amount of logic KTD14 already budgets for Textual's cap-and-collapse behavior, arguably less because
there is no widget lifecycle (mount/unmount/recycle) to manage alongside it.

### 2. Streaming coalescing

**Verdict: plausible, and closer to built-in than KTD14's hand-rolled Textual tick.**

KTD14 specifies a ~50ms coalescing boundary implemented as application logic sitting in front of Textual's
render path. Reading `Application.invalidate()` from the installed 3.0.53 source in the spike shows
`prompt_toolkit` already implements the coalescing half of that requirement natively:

```python
if self._invalidated:
    return          # never schedules a second redraw while one is pending
else:
    self._invalidated = True
...
if self.min_redraw_interval:
    # wait at minimum this amount of time between redraws
    ...
```

A burst of `invalidate()` calls collapses to a single pending redraw, and `Application(min_redraw_interval=...)`
/ `max_render_postpone_time` give an explicit minimum-gap knob — the ~50ms boundary becomes a constructor
argument rather than a coalescing scheduler Talaria has to write and test itself. The domain-side accumulation
(deltas land in the transcript immediately; the view flushes on the tick) is unchanged and portable across
either UI layer, consistent with ADR-0002.

### 3. Multi-line editing and bracketed paste

**Verdict: plausible; API surface confirmed present, end-to-end paste behavior not exercised.**

`prompt_toolkit.buffer.Buffer.__init__` takes a `multiline` parameter (confirmed by signature inspection in
the spike), and `prompt_toolkit.keys.Keys.BracketedPaste` exists as a named key in the installed 3.0.53 key
table (confirmed by direct import in the spike) — the library recognizes and dispatches on bracketed-paste
escape sequences rather than requiring Talaria to parse them. This is the criterion KTD12 leans on adoption
evidence for rather than a from-scratch capability claim: `prompt_toolkit` is the editing engine inside
IPython and pgcli, both of which handle large multi-line pastes into a terminal buffer as a core interaction,
which is prior knowledge cited here rather than something the spike exercised directly. The spike confirmed
the API exists and is reachable; it did not drive an actual paste event end to end, which is the honest edge
of "plausibility depth" for this criterion — a full check would replay a real bracketed-paste sequence
through a running `Application`, which is U5-gate-level effort, not U4-assessment-level effort.

### 4. Headless/deterministic test story

**Verdict: plausible, with a real ergonomics gap against Textual's `Pilot`.**

`prompt_toolkit.input.create_pipe_input()` and `prompt_toolkit.output.base.DummyOutput` exist and are
documented in-source (read in the spike) as "mostly useful for unit testing." A `PipeInput` supports
`send_text()` and `send_bytes()` (confirmed by inspecting `PipeInput`'s method list in the spike), so
keystrokes — including raw escape sequences, which is how a bracketed-paste test would be driven — can be
injected deterministically into a running `Application(input=pipe_input, output=DummyOutput())` without a
real terminal. This is a genuine headless, deterministic story, structurally analogous to what Textual's
`run_test()`/`Pilot` provide.

The honest gap: `prompt_toolkit` has no `Pilot`-equivalent convenience layer. Textual's `Pilot` gives
`pilot.press("ctrl+c")`-style scripted actions and built-in snapshot assertions; `prompt_toolkit`'s pipe-input
story is lower-level — a Talaria-side helper (send bytes, pump the event loop, read screen/state) would need
to be written by hand to get the same ergonomics KTD14's replay-under-test pattern assumes. That is added
work in a fallback, not a blocker — it is the kind of cost a plausibility-depth assessment is supposed to
surface rather than paper over.

### 5. Install cleanliness under `uv tool install`

**Verdict: plausible, favorable.**

Confirmed directly in the spike: `uv venv` + `uv pip install --python .venv/bin/python prompt_toolkit urwid`
resolved four packages (`prompt-toolkit==3.0.53`, `wcwidth==0.8.2`, `urwid==4.0.8`, `typing-extensions==4.16.0`)
and installed them in 10ms with no build step. Inspecting every installed package's directory for compiled
`.so` files found none — both candidates are pure Python on macOS arm64/CPython 3.12 in this environment.
`prompt_toolkit`'s own dependency footprint is exactly one package (`wcwidth`); `urwid` pulls in
`typing_extensions` as well. Either is a smaller and simpler dependency graph than Textual's own install,
which is favorable evidence for a clean `uv tool install` launch, though this was checked by installing the
libraries in isolation, not by building and installing an actual Talaria console-script distribution against
either — that end-to-end packaging check does not exist for Textual yet either (it is U1/U5 work) and is out
of scope for a fallback-plausibility assessment.

## Summary table

| Gate criterion (U4 scope) | prompt_toolkit verdict | Basis |
| --- | --- | --- |
| Bounded long-transcript strategy | Plausible — structurally simpler than Textual's version | Spike: `UIContent.get_line` is lazy/index-based; no per-entry widget mounting exists to bound |
| Streaming coalescing | Plausible — largely built in | Spike: read `Application.invalidate()` source; single-pending-redraw + `min_redraw_interval` |
| Multi-line editing and bracketed paste | Plausible — API confirmed, behavior not exercised | Spike: `Buffer(multiline=...)`, `Keys.BracketedPaste` present; IPython/pgcli adoption (prior knowledge) |
| Headless/deterministic test story | Plausible — real capability, weaker ergonomics than `Pilot` | Spike: `create_pipe_input()`, `DummyOutput`, `PipeInput.send_text/send_bytes` confirmed; no scripted-action helper ships |
| Install cleanliness under `uv tool install` | Plausible — favorable | Spike: 4-package resolve, 10ms install, zero compiled artifacts found |

`urwid` (secondary candidate): recorded per KTD12, not assessed at the same depth. Its asyncio integration is
an adapter over a synchronous `MainLoop` model rather than asyncio-native, which is a weaker fit for KTD13's
asyncio-first transport than `prompt_toolkit`; it remains a plausible second fallback rather than the named
one.

## Conclusion

`prompt_toolkit` clears plausibility on all five criteria assessed. It is not a full comparative analysis
against Textual — that is explicitly not the bar U4 sets — but it is more than a name on a list: every
verdict above rests on either a source read or a direct API/install check performed in this session's spike,
not on recollection. If U5's Textual validation gate fails, `prompt_toolkit` is a real next step: the
biggest known cost of switching is rebuilding the `Pilot`-equivalent test ergonomics by hand, not any of the
five gate criteria themselves. That cost, and the deferred end-to-end bracketed-paste and packaging checks,
are the first things a follow-on spike should close if this fallback is ever actually invoked.

## Refs

- [KTD12](../plans/2026-08-02-talaria-v0-1-prototype-plan.md), the plan's naming of `prompt_toolkit` as
  fallback and `urwid` as secondary candidate
- [PC8](../plans/2026-08-02-talaria-v0-1-prototype-plan.md#planning-closure-pc1pc10), the planning-closure
  row this document discharges
- [U4](../plans/2026-08-02-talaria-v0-1-prototype-plan.md#u4-fallback-presentation-layer-assessment), the
  unit spec this document satisfies
- [QUEUED.md](../engineering-journal/QUEUED.md), the P0 this document closes
