# u02: Why Talaria feels slow, measured

Unit u02 of the "Talaria as Claude Code for Hermes" next-steps research. Talaria is the Python 3.12 + Textual 8.2.8 terminal client in `<talaria>`, measured at commit `cafc640` (main, version 0.6.5; `git fetch` showed no newer commits on `origin/main`). All numbers below were measured on this machine on 2026-10-22 unless a line says otherwise.

## The short answer

Talaria is not slow at rest. Typing into an idle composer reaches the screen in about 2 ms, and an idle app uses under 1% of one core. It is slow in two specific places, and both are measured:

1. **Launch.** A warm launch takes 0.38 to 0.57 seconds to the first full frame. With no compiled Python bytecode it takes 1.06 to 1.20 seconds, and the operator's environment was set up to produce exactly that case (details under Startup). About 85% of a warm launch is Python importing roughly 615 modules before any code runs.
2. **Streaming.** While a real recorded Hermes reply streams at its recorded speed, Talaria uses 28% of one core on average and up to 77.5% at peak. Keystrokes typed during the heavy part of the reply take 21 ms at the median, 107 ms at the 95th percentile, and up to 157 ms to appear. Terminal output reaches 277 KB per second, because the bytes written per frame grow with the length of the message on screen (from 1.2 KB to 12 KB per frame).

Profiling the streaming case shows that about half of the busy time is Textual relaying out and repainting the whole screen on every 50 ms tick (`Screen._refresh_layout`, 48% of busy time). About 17% is Talaria's own per-tick transcript work. That 17% includes re-parsing the entire live message several times on every tick, and building a fresh markdown parser for each parse.

The framework validation gate, which passed on 2026-08-10, **fails today** on one check. The one-column-table latency workload measures a p99 of 56.5 to 66.4 ms against a 50 ms ceiling, in four runs out of four.

## Environment and method

- Machine: Apple M2 Ultra, 24 cores, 128 GiB (`sysctl -n machdep.cpu.brand_string hw.memsize hw.ncpu` → `Apple M2 Ultra / 137438953472 / 24`). macOS 27.0.1, uv 0.13.4.
- Repository Python: 3.12.11 with Textual 8.2.8 (`uv run python -c 'import sys,textual;print(sys.version, textual.__version__)'`).
- The installed operator binary: `~/.local/bin/talaria` → `~/.local/share/uv/tools/talaria`. This is a `uv tool` environment on Python 3.13, installed from git tag v0.6.5 at commit `3664d25` (`direct_url.json`).
- **Load caveat.** Other agents were running on the machine during the measurements, with a load average of 11.4 to 13.6 on 24 cores (`sysctl -n vm.loadavg` at 01:12). Single-run timings have visible outliers, so every timing below is reported as a range across repeated runs. Measurements were taken through Claude Code's Bash tool. I did not check whether its sandbox adds file-open overhead (unverified).
- **Real terminal path, not headless.** Textual's headless driver skips turning frames into terminal bytes (`textual/app.py:3821-3840`, the `not self.is_headless` guard in `_display`). Startup, idle, keystroke and streaming numbers therefore come from a pseudo-terminal harness I wrote, `<scratch>/research/ptyharness.py`. It runs the real `talaria replay` in a 160×48 pseudo-terminal with no Herdr (a terminal multiplexer, like tmux) around it. It answers Textual's synchronized-output query (`ESC[?2026$p`, sent at `textual/drivers/linux_driver.py:323`), so every frame Textual writes is bracketed by `ESC[?2026h … ESC[?2026l`. A frame is counted at its closing `ESC[?2026l`. The harness also samples `ps -o rss=,time=` on the Python process.
- Keystroke latency is the time from writing one byte to the pseudo-terminal until the end of the next complete frame. I checked that the typed characters really render in the composer: the raw capture ends with frames showing `a`, `ab`, `abc` …
- Profiles: pyinstrument 5.1.3 via `uv run --with pyinstrument` (a throwaway overlay environment; the project environment is unchanged). Flattened with `<scratch>/research/flat.py`. "Busy" time means wall time minus time spent waiting in `KqueueSelector.select`.
- Recordings used:
  - `corpora/2026-08-02-u5-gate-corpus.jsonl`: 5,773 frames, 131.3 s recorded, 5,718 message deltas (about 44 deltas per second), four turns. This is the recording with the most frames.
  - `~/.talaria/recordings/2026-08-07T03-07-54-313Z.jsonl`: 1.0 MB, 1,779 frames over 898.7 s. This is the largest recording by size.
  - `corpora/2026-08-02-u2-live-capture.jsonl` is not a frame-log v1 file. `talaria replay` rejects it with `FrameLogError: first line is not a header record`.

## 1. Startup

### `talaria --version`

| command | wall time, 3 to 4 runs |
|---|---|
| `uv run talaria --version` | 0.18, 0.20, 0.18, 0.21 s |
| `.venv/bin/talaria --version` | 0.14, 0.16, 0.17 s |
| `.venv/bin/python -c pass` (interpreter floor) | 0.03, 0.04, 0.04 s |

`uv run` adds about 40 ms. `--version` imports `talaria.cli`, which pulls in config, domain and theme modules (`talaria/cli.py:20-31`), but not the Textual app.

### Importing the app (`import talaria.ui.app`)

| condition | wall time |
|---|---|
| full bytecode cache (repository venv, cache redirected to a scratch folder) | 0.32, 0.32, 0.43 s; earlier 0.38, 0.38, 0.48 s |
| no bytecode at all (forced recompile of every module) | 0.94, 0.97, 1.11 s (an earlier run under heavier load measured 1.92 and 2.18 s) |
| installed tool, as found at 00:47 (0 `.pyc` files) | 0.75, 0.86, 0.91 s |
| installed tool, bytecode cached in a scratch folder | 0.39, 0.43, 0.57 s |

Measured with `/usr/bin/time -p` plus `PYTHONPYCACHEPREFIX` and `PYTHONDONTWRITEBYTECODE` set per row (commands are in the transcript of this unit).

**Why the no-bytecode case is the operator's real case.** Two facts combine:

1. The operator's shell profile has `export PYTHONDONTWRITEBYTECODE=1`, and the login shell is `/opt/homebrew/bin/bash` (`dscl . -read ~ UserShell`).
2. The installed tool environment held **0** `.pyc` files across 950 `.py` files when first checked (`find … -name '*.pyc' | wc -l` → `0`). uv does not compile bytecode at install unless asked to.

So every launch from a login shell recompiled about 600 modules from source. Later in the session the tool environment showed 370 `.pyc` files with modification times of 00:48:13 and 00:50:45 to 00:50:54. My own runs did not write them: those runs sent bytecode to a scratch folder, which I verified with `sys.pycache_prefix`, and they ran at about 00:55. A separate `talaria` process, started at 00:48:28 from a Herdr pane (pid <pid>, parent `-bash` under `herdr server`), was running at the time. **What wrote those files is unverified.** Either way, the 1-second launch comes back whenever that cache is missing.

### Where import time goes

Source: `uv run python -X importtime -c 'import talaria.ui.app'`, saved to `<scratch>/research/u02-importtime.txt` (as the task specified, with the shell's `PYTHONDONTWRITEBYTECODE=1`). A second file with a full bytecode cache is `<scratch>/research/u02-importtime-bytecode.txt`. `-X importtime` inflates absolute times, so read the numbers as shares.

The 15 largest cumulative import costs from `u02-importtime.txt` (cumulative µs, then module):

| cumulative | self | module |
|---|---|---|
| 735,651 | 48,639 | talaria.ui.app (total) |
| 113,872 | 7,395 | talaria.ui.transcript |
| 76,308 | 10,562 | talaria.config |
| 73,827 | 511 | textual |
| 71,129 | 9,469 | textual.widgets.markdown |
| 65,097 | 202 | textual._on |
| 62,664 | 3,241 | textual.css.model |
| 61,660 | 12,130 | textual.widgets._markdown |
| 58,830 | 1,621 | talaria.status.contract |
| 57,736 | 2,014 | textual.app |
| 55,361 | 30,557 | talaria.ui.diff_viewer |
| 53,908 | 9,096 | talaria.domain.projection |
| 51,225 | 515 | asyncio |
| 45,519 | 1,240 | asyncio.base_events |
| 43,819 | 239 | markdown_it |

Self time summed by top-level package, as the shell actually runs it (no Talaria bytecode written): talaria 425 ms, textual 128 ms, markdown_it 35 ms, rich 24 ms, asyncio 15 ms, out of 756 ms total. With a full bytecode cache: talaria 150 ms, textual 129 ms, rich 30 ms, markdown_it 27 ms, asyncio 24 ms, out of 504 ms. In-process, `sys.modules` holds **615 modules** after the import (146 from textual, 79 from talaria, 62 from markdown_it, 47 from rich).

Startup work that does not belong on the launch path:

- `talaria/transport/admin.py:490` builds `_PROBE_OPENER = urllib.request.build_opener(...)` at import time. That builds an SSL context and reads the macOS proxy settings. With bytecode this module still costs 20 ms of self time. In the import cProfile, `set_default_verify_paths` costs 13 ms and `_scproxy._get_proxies` 7 ms.
- Surfaces most sessions never open are imported eagerly: the diff viewer with pygments (20 ms cumulative), the admin transport (23 ms), the theme marketplace with urllib (9 ms), the settings workspace (6 ms) and the theme importer (3 ms). That is about 61 ms together, with bytecode.
- An import cProfile without bytecode (`u02-import.prof`) shows `builtins.compile` called 351 times for 141 ms, plus 580 `open_code` calls for 188 ms.

### Launch to first full frame (real pseudo-terminal, `talaria replay --paused corpora/2026-08-02-u5-gate-corpus.jsonl`)

| launch path | end of first frame, 3 runs | RSS at ~4 s |
|---|---|---|
| `uv run talaria` | 0.508, 0.442, 0.470 s | 68.0 to 68.5 MB |
| `.venv/bin/talaria` | 0.410, 0.383, 0.393 s | 68.1 to 68.6 MB |
| repository venv, full bytecode cache | 0.565, 0.410, 0.550 s | 67.0 to 71.9 MB |
| installed `~/.local/bin/talaria` (after the 370 `.pyc` appeared) | 0.521, 0.377, 0.391 s | 69.2 to 70.0 MB |
| repository venv, **no bytecode** | 1.197, 1.079, 1.056 s | 80.7 to 81.4 MB |

The first byte arrives about 45 to 50 ms before the first frame ends. An in-process breakdown with Textual's pilot (`<scratch>/research/pilot_startup.py`, full bytecode, 3 runs): import 255 to 364 ms; loading the 1.2 MB recording plus constructing `TalariaApp` 45 to 47 ms; mount 44 to 47 ms; first idle 84 ms.

**Conclusion:** import dominates launch. The interpreter floor is 30 to 40 ms. The app's own construction and mount add about 90 ms.

## 2. Streaming throughput and profile

### The framework gate today

Command: `/usr/bin/time -l uv run talaria gate --corpus corpora/2026-08-02-u5-gate-corpus.jsonl --deltas 50000 --json …/u02-gate.json`.
Result: **exit 1, verdict `fail`.** It took 300.3 s wall and 194.9 s user CPU, with a maximum resident set of 265 MB (`maximum resident set size 265060352`). Evidence files: `<scratch>/research/u02-gate.json` and `u02-gate.stderr`. My pseudo-terminal smoke tests overlapped this run and used under 2 s of CPU. The failing check reproduced on its own (below), so that overlap does not explain the failure.

The gate replays a synthetic 53,516-frame stress recording (50,000 deltas) twice through the real `TalariaApp`: once with no delay, and once paced to fit a 60-second window (`CADENCE_WINDOW_SECONDS = 60.0`, `talaria/replay/gate.py:189`). It also runs a feature recording, the recorded session, and the ADR-0006 growth workloads. The documents behind it are `docs/analysis/2026-08-03-textual-validation-gate-results.md` and `docs/analysis/2026-08-09-block-markdown-gate-results.md`.

| measurement | 2026-08-03 record | 2026-08-10 confirming run | today | ceiling |
|---|---|---|---|---|
| stress pass with no delay, elapsed | 2.29 s | — | **8.41 s** | none |
| stress pass, resident-set growth | 44.3 MB | 117 MB | 123.7 MB | < 300 MB |
| paced pass, elapsed (intended 60 s) | 190.2 s | — | **249.5 s** | none |
| paced pass, render ticks per second | 15.28 | — | 12.32 | ≤ 25 |
| recorded session, elapsed | 0.394 s | — | 0.949 s | none |
| peak mounted widgets, stress | 501 | 501 | 177 | ≤ 600 |
| peak descendants, stress | — | 295 (folded window) | 260 | ≤ 600 |
| growing-open-fence p99 | — | 19.5 ms | 22.6 ms | 50 ms |
| **growing-one-column-table p99** | — | 44.0 ms | **59.5 ms (fail)** | 50 ms |
| growing-unbroken-line p99 | — | 9.8 ms | 9.5 ms | 50 ms |
| table demotion apply (recorded, not enforced) | — | — | 441 ms | — |
| fence demotion apply (recorded, not enforced) | — | — | 208 ms | — |

Sources: the 2026-08-03 column is from `docs/analysis/evidence/2026-08-03-textual-validation-gate.json`. The 2026-08-10 column is from the table in `2026-08-09-block-markdown-gate-results.md`. Today's column is from `u02-gate.json`.

I reran the workloads alone three times with no gate around them (`workloads3.py`). The table p99 measured **63.9, 56.5 and 66.4 ms**, every run over the ceiling. Table demotion took 420.6, 471.9 and 465.6 ms. Fence demotion took 160.8, 140.0 and 163.9 ms. The 2026-08-10 results document already called the table check "the check most sensitive to machine load", with six confirming attempts between 44.0 and 59.1 ms. Today it is over the ceiling in four runs out of four.

Two notes on what the gate does and does not measure:

- **The paced pass running 4.2 times longer than its 60-second window** shows that the app cannot keep pace with an 890-frame-per-second paced stream while it renders. However, `ReplaySource` sleeps a relative delay per frame through a new task (`talaria/replay/source.py:366-369, 428-438`), so part of that overrun may be pacing overhead rather than render cost. **How the 189 s overrun splits between the two is unverified.**
- **The stress pass with no delay got 3.7 times slower between 2026-08-03 and today** (2.29 s to 8.41 s). The profile below attributes 18% of its busy time to `dataclasses.replace` in the domain reducer. This is a throughput regression as features landed. At real streaming rates it is not what the operator feels (see the reducer cost under "Causes").

### Real-time streaming in a real terminal

Command: `cpusampler.py … -- .venv/bin/talaria replay --speed 1 corpora/2026-08-02-u5-gate-corpus.jsonl`, sampling every 2 s over the full 131 s recording. Evidence: `<scratch>/research/E-cpu-x1.json`.

| turn (recorded time) | content | CPU of one core | terminal output |
|---|---|---|---|
| 1 (1 to 30 s) | 6,705 chars, 14 newlines | 7 to 12% | 16 to 56 KB/s |
| 2 (30 to 62 s) | 6,415 chars, 118 newlines | **54 to 77.5%** | 163 to 277 KB/s |
| 3 (62 to 92 s) | 7,130 chars, 99 newlines, a code fence | 20 to 26% | 66 to 99 KB/s |
| 4 (92 to 131 s) | 8,032 chars, 22 newlines | 14 to 25% | 51 to 99 KB/s |
| after the stream ends | — | 0.5% | 0.5 KB/s |

Across the 63 samples: mean 28.2%, median 22.0%, maximum 77.5% of one core. RSS rose from 78.8 MB to 112.0 MB.

Cost tracks the **number of markdown blocks** in the live message, not its size. Turn 2 is no longer than the others, but it has about eight times as many lines.

Bytes per frame during the same replay (`C-stream-x1.json`, 5-second buckets) grow with the message: 1,676, 1,249, 2,258, 2,913, 3,370, 5,000, 6,611, 8,593, 9,920, 11,329 and 12,073 bytes per frame, at 16 to 35 frames per second. The terminal is being sent most of the visible transcript again on each frame, rather than only the new text.

### Where the streaming time goes

Profile: pyinstrument over 45 s of `talaria replay --speed 1` in the pseudo-terminal, covering turns 1 and 2. Evidence: `<scratch>/research/u02-stream-x1.pyisession`, flattened in `u02-stream-x1-flat.txt`; the full tree is in `u02-pyinstrument-stream-x1.txt`. Busy time was 16.56 s of 45.18 s. Inclusive shares of busy time:

| share | function |
|---|---|
| 65.9% | Textual timers (`textual/timer.py:143 _run_timer`) |
| **48.6%** | `Screen._on_timer_update` → **48.0% `Screen._refresh_layout`** (`textual/screen.py:1236, 1316`) |
| 27.8% | `Compositor.render_update` (`textual/_compositor.py:1096`); 21.2% `_get_renders`; 20.5% `_styles_cache.render_widget` |
| 17.8% | `Compositor._arrange_root` / `add_widget` (`_compositor.py:525, 548`); 15.1% `reflow` |
| **17.0%** | Talaria `_render_tick` → `render_snapshot` → `TranscriptPane.apply` (16.4%) → `_reconcile_tail` (15.8%) (`talaria/ui/app.py:2457, 2501`; `talaria/ui/transcript.py:1027, 1547`) |
| 12.8% | `Stylesheet.apply` (CSS restyle, `textual/css/stylesheet.py:470`) |
| 11.0% | `Markdown.await_append` (`textual/widgets/_markdown.py:1465`) |
| 8.3% | widget mount and register (`textual/widget.py:1424`, `textual/app.py:3621`) |
| 7.7% | markdown-it `parse` |
| 7.3% | `EntryMarkdown._trim_trailing_block_margin` → `set_class` → CSS update (`talaria/ui/blocks.py:648`) |
| 6.4% | `trips_fallback_trigger` (`talaria/ui/transcript.py:493`), of which 5.8% is `descendant_estimate` re-parsing |

Hottest functions by self time are spread thin across Textual internals. The largest are `textual/cache.py:226 __init__` at 4.0%, `css/styles.py:1507 get_rule` at 2.5%, `strip.py:94 __init__` at 2.1% and `stylesheet.py:470 apply` at 1.8%. No single hot loop exists to fix.

Profile of the stress pass with no delay (`stress_unbounded.py` under pyinstrument, 17.8 s busy; `u02-stress-unbounded-flat.txt`):

| share | function |
|---|---|
| 22.5% | `TalariaApp.ingest` → 21.2% `route_frame` (`talaria/domain/state.py:3603`) |
| **18.3%** | **`dataclasses.replace`** (self time 9.0%, the largest single self-time entry) |
| 26.9% | Textual `_refresh_layout` |
| **11.5%** | **`parser_factory` → `MarkdownIt.__init__`** (`talaria/ui/blocks.py:164`), including 5.8% linkify regex compilation |
| 7.9% | `trips_fallback_trigger`; 7.2% `is_zero_block` |

## 3. Memory (resident set, `ps -o rss`)

| state | RSS |
|---|---|
| after startup, empty transcript (`--paused`) | 67.0 to 71.9 MB (no bytecode: 80.7 to 81.4 MB) |
| after replaying the 5,773-frame recording at full speed | 90.4 MB, then 91.5 MB after 30 keystrokes |
| after replaying the largest recording (1.0 MB, `2026-08-07T03-07-54-313Z.jsonl`) | 97.5 MB, then 100.5 MB after keystrokes |
| after streaming the 5,773-frame recording at real speed | 111 to 112 MB |
| gate process, peak (stress recording of 50,000 deltas) | 265 MB (`/usr/bin/time -l`) |

Evidence files: `B1-paused.json`, `B2-u5-full.json`, `B3-largest-bytes.json` and `E-cpu-x1.json` in the research scratch folder. Memory is bounded and reasonable for a Python app. It is not the cause of the slowness. The gate's `rss_growth_mb` is under its 300 MB ceiling, but it nearly tripled between 2026-08-03 and 2026-08-10 (44.3 to 117 MB) and is 123.7 MB today.

## 4. Idle CPU

Thirty seconds of idle in the pseudo-terminal harness, with the operator's real configuration (`~/.talaria/config.toml`: status command `~/.talaria/talaria-statusline.sh`, every 5 s):

| state | CPU in 30 s | share of one core | frames | bytes written |
|---|---|---|---|---|
| empty transcript (`--paused`) | 0.23 s | 0.77% | 66 | 17,154 |
| after the 5,773-frame recording | 0.26 s | 0.87% | 66 | 17,154 |
| after the largest recording | 0.30 s | 1.00% | 66 | 17,988 |

- **The app repaints about 2.2 times per second while idle.** In `B1-paused.json`, 52 of 65 gaps between frames are 0.5 s. That matches the default cursor blink of the composer, which is a Textual `TextArea` (`talaria/ui/composer.py:44,57`). The blink attribution is a hypothesis: the 0.5 s period fits it, but I did not disable the blink to confirm.
- **Three always-on timers** run from `on_mount` (`talaria/ui/app.py:2073-2087`): the 50 ms render tick (`COALESCE_INTERVAL = 0.05`, `app.py:338`), a 1 s connection poll (`POLL_TICK_S`, `app.py:730`) and a 300 s seam re-probe (`PROBE_REVALIDATION_S`, `talaria/domain/compat.py:623`). The render tick does work before it checks whether anything changed: it ages out approvals, refreshes seam ages, and re-applies the bottom status bar view (`app.py:2457-2469`). That happens 20 times a second whether or not anything changed.
- **The status-bar script** runs as a separate process every 5 s, not on the event loop (`asyncio.create_subprocess_exec`, `talaria/status/runner.py:197`). One run of `~/.talaria/talaria-statusline.sh` costs 0.05 s wall and about 0.04 s CPU (5 runs of `/usr/bin/time -p`). That is about 0.8% of a core in child processes, on top of the app's own 0.8 to 1.0%. The script starts bash, three `jq` calls and two `git` calls each time.

Idle cost is low in absolute terms. It is not zero, and it is not why the app feels slow.

## 5. Input latency

Pseudo-terminal harness: 30 to 60 single keystrokes 150 ms apart, timed from the write to the end of the next frame.

| state | median | p95 | max |
|---|---|---|---|
| empty transcript | 1.84 ms | 2.38 ms | 13.85 ms |
| after the 5,773-frame recording | 1.75 ms | 2.27 ms | 19.39 ms |
| after the largest recording (1.0 MB) | 6.87 ms | 9.17 ms | 40.75 ms |
| **during real-time streaming, heavy turn 2** (`C-stream-x1.json`) | **21.01 ms** | **107.37 ms** | **157.19 ms** |
| after an 8× replay had finished | 1.82 ms | 2.23 ms | 26.38 ms |

The keystroke handler itself is fast. Latency comes from the event loop being busy with render ticks. The single asyncio loop runs the domain reducer, Talaria's reconcile, Textual's layout and compositor, and input handling, so a 100 to 150 ms tick delays the keystroke behind it. Textual's pilot is not a usable instrument for this: `pilot.press()` + `pilot.pause()` measured a constant 85 to 87 ms in `pilot_startup.py` regardless of the work done, which is the pilot waiting for idle rather than render latency.

## 6. Comparison with the accepted targets

| target | source | today | verdict |
|---|---|---|---|
| p99 apply latency ≤ 50 ms on the three growth workloads (steady state) | ADR-0006 (d), `platform-specs/04-architecture/adrs/0006-block-rendering-is-bounded-by-work-and-height.md:164-178` | fence 22.6, table **56.5 to 66.4**, line 9.5 ms | **table fails** |
| demotion and block-phase cost "recorded, not enforced" | ADR-0006 (d) | table demotion 420 to 472 ms; fence 140 to 208 ms | a 0.4 s freeze the ADR exempts from enforcement |
| folded-window descendants ≤ 600 | ADR-0006 (a), `gate.py:69` | 260 to 263 | pass |
| mounted line widgets ≤ 600 | gate, 2026-08-03 | 177 to 181 | pass |
| render ticks ≤ 25 per second | KTD14 (the coalescing-tick decision) | 1.31 unpaced, 12.32 paced | pass |
| resident-set growth < 300 MB on the stress recording | KTD14 | 123.7 MB | pass |
| content loss 0, determinism identical | gate | 0, identical | pass |
| launch under 200 ms, keystroke echo under 16 ms, idle CPU near 0%, RSS under 100 MB after 2,000 messages | **draft only**: R1 in `docs/handoff/2026-10-22-next-steps/01-vision-and-requirements.md:27`, an untracked file another unit is writing | launch 0.38 to 1.20 s; keystroke 1.8 ms idle but 107 ms p95 while streaming; idle 0.8 to 1.0%; 90 to 112 MB after one 4-turn session | misses on launch and on keystrokes during streaming |

None of the accepted targets constrains launch time, keystroke latency, CPU during streaming, idle CPU, or bytes written to the terminal. Those are the dimensions the operator actually feels, so the gate passing (as it did in August) never meant the app felt fast.

## 7. Causes, each tied to a measurement

1. **Python start-up and import weight.** 615 modules load before the first frame. Import is 255 to 480 ms of a 380 to 570 ms warm launch, and 0.94 to 1.11 s when bytecode is missing.
   - Talaria's own modules are 30% of import self time even with bytecode (150 of 504 ms). The app module alone is 8,486 lines (`wc -l talaria/ui/app.py`), and the package is 61,002 lines.
   - Optional surfaces and an SSL and proxy opener load at import (about 61 ms plus 20 ms).
   - The operator's `PYTHONDONTWRITEBYTECODE=1` plus a uv tool install with no bytecode turned that into a full recompile on every launch.

2. **Textual's full-screen layout and compositor on every tick.** 48% of busy streaming time is `Screen._refresh_layout`: arrange (17.8%), reflow (15.1%) and render update (27.8%). CSS restyle adds 12.8%.
   - Bytes per frame grow from 1.2 KB to 12 KB as the message grows. Each tick that changes the live message's height relays out the screen and resends most of the visible transcript. Textual composites a full-screen buffer and does not use terminal scroll regions or scrollback, so the cost of a frame is proportional to what is visible, not to what changed. That second sentence is a hypothesis drawn from the byte growth. I did not read the compositor source to confirm it.

3. **Markdown work per tick that grows with the message.** On each 50 ms tick, `_reconcile_tail` (`talaria/ui/transcript.py:1597, 1629`) calls `is_zero_block` and `trips_fallback_trigger`. Each of them re-parses the **entire** live message with a **newly built** parser: `parser_factory()` constructs `MarkdownIt("gfm-like")` and checks its rules on every call (`talaria/ui/blocks.py:164-190`, `transcript.py:428, 470`).
   - Building a parser costs 0.214 ms. On the real turns, the checks cost 1.51 to 3.44 ms per tick by the end of the turn, growing linearly, so a message costs O(n²) in total.
   - `Markdown.append` itself is incremental (ADR-0006 (c)). `_trim_trailing_block_margin` then visits every block and calls `set_class` (`blocks.py:648-653`, 7.3%), and every new paragraph or list item mounts new widgets (8.3%).
   - Measured result: a 6.4 KB reply with 118 lines drives 54 to 77.5% of a core at 44 deltas per second.

4. **Synchronous work on the one event loop.**
   - Rendering, reconcile, the reducer and input share one asyncio loop, so keystrokes during streaming wait up to 157 ms.
   - Demotion to line rendering runs inside one apply, including an explicit `gc.collect()` (`transcript.py:1623`). That makes a 420 to 472 ms freeze for a table and 140 to 208 ms for a fence.
   - The domain reducer copies state with `dataclasses.replace` for each frame: 63 call sites in `talaria/domain/state.py` (`grep -c`), 18.3% of the stress profile. That is about 70 µs per frame, which matters for bursts and replays but not at 44 deltas per second.

5. **Polling and timers.** The 50 ms render tick runs three pieces of work even when idle (`app.py:2457-2469`). The connection poll runs at 1 s and the cursor blinks at 0.5 s. Measured cost: 0.8 to 1.0% of a core, 2.2 frames and about 570 bytes per second, plus about 0.8% of a core in status-script children. This is real but small, and it is not the cause of perceived slowness.

## 8. Numeric targets for a rewrite (or a heavy refactor)

Proposed engineering targets. Each is measured with the same pseudo-terminal method at 160×48 on this machine.

| dimension | today | target |
|---|---|---|
| launch to first usable frame, warm | 0.38 to 0.57 s | ≤ 100 ms (≤ 200 ms to a usable composer with a connected backend) |
| launch, worst realistic case (no caches) | 1.06 to 1.20 s | ≤ 150 ms; no compile or cache step on the launch path |
| keystroke to frame, idle | 1.8 ms median | ≤ 5 ms p95 |
| keystroke to frame **while streaming** | 21 ms median, 107 ms p95, 157 ms max | ≤ 16 ms p95, ≤ 33 ms max, at 1× and at 10× the recorded rate |
| CPU while streaming the 5,773-frame recording at 1× | mean 28%, max 77.5% of a core | mean ≤ 5%, max ≤ 15%, independent of block count |
| terminal bytes per streamed frame | 1.2 to 12 KB, growing with message length | proportional to changed cells: ≤ 2 KB typical, never growing with transcript length |
| worst single frame or apply, including demotion or representation switches | 420 to 472 ms | ≤ 50 ms; no stop-the-world collection on the UI thread |
| p99 apply on the ADR-0006 growth workloads | table 56.5 to 66.4 ms | ≤ 16 ms (one 60 Hz frame), with no warm-up or demotion exclusions |
| idle CPU | 0.8 to 1.0% plus status script | ≤ 0.2%; zero frames when nothing changes (blink optional) |
| RSS after startup | 67 to 72 MB | ≤ 40 MB |
| RSS after a 2,000-message session | not measured (largest local recording reached 97.5 to 112 MB) | ≤ 100 MB |
| reducer throughput | 53,516 frames in 8.41 s with the UI (about 6,400 per second) | ≥ 50,000 deltas per second with the UI attached, so replay and burst catch-up are instant |

What a rewrite must do differently, derived from the causes:

- **Do not reflow the whole screen per delta.** Render finished messages once into scrollback, or an equivalent cached region, and repaint only the live tail. Emit diffs proportional to the changed cells.
- **Parse markdown incrementally, once per delta, with a parser built once.** Never re-parse the whole message to answer a size question. Keep running counters instead.
- **Keep input and rendering off the network and reducer path.** Use a separate thread or task with a bounded frame budget, so a slow frame drops frames rather than delaying keys.
- **Use a compiled or lazily loaded start-up path** with no import or bytecode compile phase that grows with features.
- **Put launch, keystroke, streaming CPU, bytes per frame and idle CPU in the gate itself.** The current gate passed while all of these were unmeasured.

Prior-art claims (for example, that Claude Code renders finished output into scrollback, or that ratatui-based Rust clients start in tens of milliseconds) were **not measured in this unit and are unverified here**.

## 9. Slow paths that can be fixed without a rewrite

Rough expected gains, derived from the measurements above. None of these fixes was tested in this unit.

1. **Bytecode at install** (`uv tool install --compile-bytecode`, or `UV_COMPILE_BYTECODE=1`), plus reconsidering `PYTHONDONTWRITEBYTECODE=1` in the operator's shell profile. Measured difference: 1.06 to 1.20 s down to 0.38 to 0.41 s to first frame. This is the largest single launch fix and needs no code change.
2. **Lazy imports** for the diff viewer and pygments, the admin transport, the marketplace, the settings workspace and the theme importer. Build `_PROBE_OPENER` on first use (`talaria/transport/admin.py:490`). Expected gain: about 60 to 80 ms of import.
3. **Build the markdown parser once** and reuse it. `parser_factory` accounts for 11.5% of the stress profile at 0.214 ms per construction. Its rule-set assertion can run once at start-up.
4. **Stop re-parsing the whole live message per tick.** Cache `is_zero_block` and `descendant_estimate` per text length, or update them incrementally from the tokens `append` already produced. The checks cost 1.5 to 3.4 ms per tick at the end of a real turn (about 6 to 8% of busy time).
5. **Set the last-block class only on the blocks that changed** (two at most) instead of all of them (`blocks.py:648-653`). About 7% of busy streaming time.
6. **Move `gc.collect()` out of the demotion frame**, or demote before the block document grows large. This targets the 420 to 472 ms freeze.
7. **Skip idle work in the render tick** when nothing changed, and offer a no-blink composer option. Idle drops from about 1% toward 0%.
8. **Replace `dataclasses.replace`** in the reducer's per-delta path with a cheaper update, such as appending to a mutable buffer for the streaming text. This targets the 18% of the stress profile and the 3.7× slowdown of the unpaced stress pass since August.

What cannot be fixed from inside Textual without replacing its rendering model: the 48% of streaming time in `Screen._refresh_layout` and the compositor, the per-frame byte growth, and the 255 to 360 ms import floor of Python plus Textual plus rich plus markdown-it. Items 1 to 8 could plausibly bring a warm launch to about 0.3 s and streaming CPU down by roughly a third. That estimate is a hypothesis and was not measured. They would not reach the keystroke-during-streaming or bytes-per-frame targets in section 8.

## Side effects and anomalies

- No edits, commits or configuration changes were made in either repository. Measurement runs wrote bytecode only to scratch folders (`repopyc`, `toolpyc`, `nopyc`). The `uv sync --all-groups -q` at the start made no visible change: `git status` was clean apart from `docs/handoff/`, which another unit created.
- The installed tool environment gained 370 `.pyc` files at 00:48:13 and 00:50:45 to 00:50:54. I attribute them to another process, not to this unit (see Startup), but the attribution is unverified. I did not delete them.
- `corpora/2026-08-02-u2-live-capture.jsonl` cannot be replayed (`FrameLogError`).

## Evidence files

All are in `<scratch>/research/`:

- Import timings: `u02-importtime.txt`, `u02-importtime-bytecode.txt`, `u02-import.prof`
- Gate run: `u02-gate.json`, `u02-gate.stdout`, `u02-gate.stderr`
- Pseudo-terminal runs: `A-*.json` (startup), `B1-paused.json`, `B2-u5-full.json`, `B3-largest-bytes.json` (idle, keystrokes, memory), `C-stream-x1.json`, `C-stream-x8.json` (streaming keystrokes and bytes), `E-cpu-x1.json` (CPU over the full replay)
- Profiles: `u02-stream-x1.pyisession`, `u02-stream-x1-flat.txt`, `u02-pyinstrument-stream-x1.txt`, `u02-stress-unbounded.pyisession`, `u02-stress-unbounded-flat.txt`, `u02-idle.pyisession`
- Scripts: `ptyharness.py`, `cpusampler.py`, `flat.py`, `pilot_startup.py`, `stress_unbounded.py`, `workloads3.py`
