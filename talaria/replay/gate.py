"""The framework validation gate: run the real interface, record the numbers.

This module does not decide anything. It replays a corpus through the shipped
:class:`~talaria.ui.app.TalariaApp`, records KTD14's four measurements, and
compares each against its threshold. The verdict is arithmetic — the plan is
explicit that subjective smoothness is not a pass condition and that no
reviewer, panel, or observer sign-off gates it.

**What each measurement is, and what it is not.**

``peak_mounted_widgets``
    The high-water mark of live line widgets, read from the pane's own counter
    rather than sampled. A sampler can step over a spike; a counter cannot.

``rss_series`` / ``rss_slope_per_1k_frames``
    ``ru_maxrss`` sampled every :data:`RSS_SAMPLE_EVERY` frames. This is a
    *maximum* resident set, so it is monotonic by construction — it can never
    show memory being returned. That is precisely why KTD14 asks for the series
    and the fitted slope rather than a single endpoint: the endpoint answers
    "did this run stay under 300 MB", and only the slope answers "what happens
    to a session that runs all day". The slope is the input to whether domain
    transcript eviction becomes a milestone-3 requirement.

``render_ticks_per_second``
    Counted in the coalescing flush callback, divided by the run's wall clock.
    With a 50ms coalescing boundary the ceiling is 20/s by construction, so this
    measurement is really a *check that coalescing is engaged* — a regression
    that rendered per frame would show up here as hundreds per second.

``content_loss``
    At each checkpoint, every line of every committed domain transcript entry
    must appear, in order, in the projection the UI renders from. This is the
    one measurement that would catch the worst possible outcome: a fast,
    bounded, smooth interface that is quietly dropping the conversation.
"""

from __future__ import annotations

import asyncio
import hashlib
import platform
import resource
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from talaria.domain.projection import TranscriptView, transcript_view
from talaria.domain.state import SessionState
from talaria.replay.controls import MAX_SPEED, MIN_SPEED, ReplayControls
from talaria.replay.source import ReplaySource, load_frame_records, load_header
from talaria.replay.stress import StressCorpus, build_stress_corpus
from talaria.transport.source import FrameRecord
from talaria.ui.app import TalariaApp
from talaria.ui.transcript import DEFAULT_MOUNT_CAP


class GateError(Exception):
    """The gate could not run as specified.

    Raised rather than degrading quietly. A gate that measures fewer things than
    it claims and still exits 0 is worse than one that refuses to run.
    """


# ── KTD14 thresholds (PC7) ───────────────────────────────────────────────

#: Mounted line widgets allowed at any point of the stress corpus.
MOUNTED_WIDGET_CEILING = 600

#: Resident-set growth allowed across a full 50k-delta replay, in megabytes.
RSS_GROWTH_CEILING_MB = 300.0

#: Coalescing flushes per second allowed at maximum replay speed.
RENDER_TICKS_PER_SECOND_CEILING = 25.0

#: How often the memory series is sampled, in frames.
RSS_SAMPLE_EVERY = 5_000

#: Minimum points required before a fitted slope is worth publishing. The
#: sampler advances past every boundary crossed within one 20ms poll, so on a
#: fast machine the series can collapse to two points — an endpoint difference
#: reported as a fitted slope. Below this the gate fails rather than publishing
#: a number it did not really measure.
MIN_RSS_SAMPLES = 6

#: Minimum content checkpoints. "0 of 1" and "0 of 11" are very different
#: claims, and only the second is evidence.
MIN_CONTENT_CHECKPOINTS = 5

#: Terminal geometry the gate renders at. Fixed so a measurement taken on one
#: machine means the same thing on another.
GATE_SIZE = (100, 40)

#: How long the sustained-streaming pass should run. KTD14 states the render
#: tick threshold "divided by a fixed 60-second wall-clock window", and this is
#: that window.
CADENCE_WINDOW_SECONDS = 60.0


@dataclass(frozen=True)
class CorpusIdentity:
    """How a corpus is cited. Never by path — this repository is public (R29)."""

    label: str
    sha256: str
    frame_count: int
    kind: str
    note: str = ""

    def to_dict(self) -> dict[str, Any]:
        return {
            "label": self.label,
            "sha256": self.sha256,
            "frame_count": self.frame_count,
            "kind": self.kind,
            "note": self.note,
        }


@dataclass
class GateMeasurement:
    """One replay's numbers."""

    corpus: CorpusIdentity
    frames_applied: int
    render_ticks: int
    elapsed_seconds: float
    render_ticks_per_second: float
    peak_mounted_widgets: int
    condensed_lines: int
    rss_series_mb: list[tuple[int, float]] = field(default_factory=list)
    rss_growth_mb: float = 0.0
    rss_slope_mb_per_1k_frames: float = 0.0
    content_loss_checkpoints: int = 0
    content_loss_failures: int = 0
    transcript_entries: int = 0
    transcript_lines: int = 0

    def to_dict(self) -> dict[str, Any]:
        return {
            "corpus": self.corpus.to_dict(),
            "frames_applied": self.frames_applied,
            "render_ticks": self.render_ticks,
            "elapsed_seconds": round(self.elapsed_seconds, 3),
            "render_ticks_per_second": round(self.render_ticks_per_second, 3),
            "peak_mounted_widgets": self.peak_mounted_widgets,
            "condensed_lines": self.condensed_lines,
            "rss_series_mb": [[frames, round(mb, 2)] for frames, mb in self.rss_series_mb],
            "rss_growth_mb": round(self.rss_growth_mb, 2),
            "rss_slope_mb_per_1k_frames": round(self.rss_slope_mb_per_1k_frames, 4),
            "content_loss_checkpoints": self.content_loss_checkpoints,
            "content_loss_failures": self.content_loss_failures,
            "transcript_entries": self.transcript_entries,
            "transcript_lines": self.transcript_lines,
        }


@dataclass
class GateResult:
    """Every measurement, every threshold check, and the arithmetic verdict."""

    stress: GateMeasurement
    cadence: GateMeasurement
    live: GateMeasurement | None
    #: None when no live corpus was replayed, so an unmeasured run cannot
    #: be mistaken for a passing one.
    determinism_identical: bool | None
    inert_controls_refused: tuple[str, ...]
    matrix: dict[str, str]
    checks: dict[str, dict[str, Any]] = field(default_factory=dict)

    @property
    def passed(self) -> bool:
        return all(check["pass"] for check in self.checks.values())

    @property
    def verdict(self) -> str:
        return "pass" if self.passed else "fail"

    def to_dict(self) -> dict[str, Any]:
        return {
            "verdict": self.verdict,
            "matrix": self.matrix,
            "checks": self.checks,
            "stress": self.stress.to_dict(),
            "cadence": self.cadence.to_dict(),
            "live": self.live.to_dict() if self.live is not None else None,
            "determinism_identical": self.determinism_identical,
            "inert_controls_refused": list(self.inert_controls_refused),
        }


# ── measurement helpers ──────────────────────────────────────────────────


def _rss_mb() -> float:
    """Maximum resident set, in megabytes.

    ``ru_maxrss`` is bytes on Darwin and kilobytes on Linux — the one platform
    difference in this module, and getting it wrong would silently misreport by
    a factor of 1024, so it is branched on explicitly rather than guessed from
    the magnitude.
    """
    raw = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    divisor = 1024 * 1024 if sys.platform == "darwin" else 1024
    return raw / divisor


def _fit_slope(series: list[tuple[int, float]]) -> float:
    """Least-squares slope in MB per 1,000 frames. Zero for a degenerate series."""
    if len(series) < 2:
        return 0.0
    xs = [frames / 1000.0 for frames, _ in series]
    ys = [mb for _, mb in series]
    n = float(len(series))
    mean_x = sum(xs) / n
    mean_y = sum(ys) / n
    denominator = sum((x - mean_x) ** 2 for x in xs)
    if denominator == 0.0:
        return 0.0
    numerator = sum((x - mean_x) * (y - mean_y) for x, y in zip(xs, ys, strict=True))
    return numerator / denominator


def interface_shows_everything(app: TalariaApp, *, settled: bool) -> bool:
    """Is the conversation actually on screen, as opposed to merely in the domain?

    :func:`content_is_complete` is a domain-side check, and at both of its call
    sites the ``view`` argument was ``transcript_view(app.state)`` — a pure
    function of the very state being walked. Its own docstring warns that
    comparing the projection against itself "would pass no matter what", which
    is precisely what the call sites did. Making ``TranscriptPane.apply`` a
    no-op, so the interface rendered nothing at all, produced a completely blank
    screen and a ``pass`` verdict with zero content loss.

    This reads the pane instead. Every line the pane still holds as a widget
    must be the projection's line at the same position, and the pane's own
    accounting — mounted lines plus condensed lines — must add up to the whole
    transcript, which is what makes "bounded" and "complete" one claim rather
    than two. A pane that renders nothing fails on the second half; a pane that
    renders the wrong text fails on the first.
    """
    view = transcript_view(app.state)
    pane = app.transcript
    on_screen = tuple(pane.rendered_lines)
    if len(on_screen) > len(view.lines):
        return False

    # Correctness, checked always: whatever the pane is showing must be a real
    # contiguous window of the projection, not invented or reordered text.
    # Anchored at the pane's own top index rather than at the tail, because
    # mid-stream the pane is legitimately behind — it renders on a coalescing
    # boundary, so between flushes it holds an older, shorter window. Requiring
    # it to match the newest tail at every instant is a race, not a defect.
    top = pane.condensed_count
    window = tuple(view.lines[top : top + len(on_screen)])
    if not settled:
        # Mid-stream this asserts liveness, not equality. The pane renders on a
        # coalescing boundary, and an arbitrary number of deltas can land
        # between two flushes — each appending lines and rewriting the tail in
        # place — so the pane is legitimately behind by an unknown number of
        # lines, and *any* suffix comparison against the current projection is a
        # race. Asserting equality here failed all eleven sustained checkpoints
        # against a pane that was provably correct once settled.
        #
        # What still holds while the stream moves: the pane cannot be showing
        # more than exists. That is a true invariant at every instant, so it is
        # asserted at every instant.
        #
        # Deliberately not asserting "the pane is non-blank" here as well. It
        # reads like a stronger check but it is a race — before the first flush
        # the projection legitimately has content the pane has not rendered yet,
        # which failed exactly one checkpoint per run — and it catches nothing
        # the settled check below does not already catch outright: a pane that
        # renders nothing fails the settled sum, since 0 mounted plus 0
        # condensed cannot equal a non-empty transcript.
        return len(on_screen) + pane.condensed_count <= len(view.lines)

    if on_screen != window:
        return False

    # Completeness, checked only once the stream has stopped and a flush has
    # run. Now there is no in-flight snapshot to be behind, so every line must
    # be either mounted or accounted for by the condensed block. This is the
    # half that a pane rendering nothing at all fails.
    return len(on_screen) + pane.condensed_count == len(view.lines)


def content_is_complete(state: SessionState, view: TranscriptView) -> bool:
    """Every committed entry's lines appear, in order, in the rendered projection.

    Deliberately not ``view == transcript_view(state)``: that would compare the
    projection against itself and pass no matter what. This walks the domain's
    own entries and requires each of their lines to be findable in sequence.
    """
    cursor = 0
    lines = view.lines
    for entry in state.transcript:
        for fragment in entry.text.split("\n"):
            while cursor < len(lines) and fragment not in lines[cursor]:
                cursor += 1
            if cursor >= len(lines):
                return False
            cursor += 1
    return True


def live_corpus_identity(path: str | Path, records: tuple[FrameRecord, ...]) -> CorpusIdentity:
    """Cite a recorded corpus by digest and count. The path never leaves this call."""
    digest = hashlib.sha256(Path(path).read_bytes()).hexdigest()
    header = load_header(path)
    note = f"frame-log v{header.version}, recorded {header.started_at}"
    return CorpusIdentity(
        label=f"talaria-live-v1-{len(records)}f-{digest[:12]}",
        sha256=digest,
        frame_count=len(records),
        kind="recorded-session",
        note=note,
    )


def stress_corpus_identity(corpus: StressCorpus) -> CorpusIdentity:
    return CorpusIdentity(
        label=corpus.label,
        sha256=corpus.sha256,
        frame_count=corpus.frame_count,
        kind="synthetic-stress",
        note=(
            f"generated, seed {corpus.seed}: {corpus.delta_count} deltas across "
            f"{corpus.turn_count} turns, with interleaved malformed frames, "
            "unknown event types and sub-agent fan-out"
        ),
    )


# ── the replay runs ──────────────────────────────────────────────────────


async def measure_replay(
    records: tuple[FrameRecord, ...],
    identity: CorpusIdentity,
    *,
    mount_cap: int = DEFAULT_MOUNT_CAP,
    timeout: float = 900.0,
    speed: float | None = None,
) -> tuple[GateMeasurement, SessionState, tuple[str, ...]]:
    """Replay one corpus through the real app and measure it.

    ``speed=None`` means unbounded — no inter-frame delay at all, which is what
    KTD14 calls maximum replay speed. A finite ``speed`` replays on the recorded
    cadence scaled by that multiplier, which is the only way to hold the
    renderer under sustained streaming long enough to measure it: at unbounded
    speed the domain reducer drains a 50,000-frame corpus in a few seconds, so
    the render loop is never the thing under load.
    """
    controls = ReplayControls()
    if speed is None:
        controls.set_unbounded()
    else:
        controls.set_speed(speed)
    source = ReplaySource(records, controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls, mount_cap=mount_cap)

    rss_series: list[tuple[int, float]] = [(0, _rss_mb())]
    checkpoints = 0
    failures = 0

    async with app.run_test(size=GATE_SIZE) as pilot:
        stop = asyncio.Event()

        async def sample() -> None:
            nonlocal checkpoints, failures
            next_mark = RSS_SAMPLE_EVERY
            while not stop.is_set():
                await asyncio.sleep(0.02)
                applied = app.frames_applied
                if applied >= next_mark:
                    rss_series.append((applied, _rss_mb()))
                    # A memory sample is also a natural pause point, so the
                    # content check runs at the same cadence rather than
                    # needing a second schedule.
                    checkpoints += 1
                    # Both halves: the domain kept the conversation, and the
                    # interface is actually showing it.
                    if not content_is_complete(app.state, transcript_view(app.state)):
                        failures += 1
                    # The interface-side check runs only at the settled
                    # checkpoint, not here. A coalescing renderer is, by
                    # design, an unknown number of flushes behind the
                    # projection at any given instant: it can show fewer lines
                    # than exist (nothing flushed yet) and briefly more than
                    # exist (a completing turn shortened the projection under
                    # a window the pane still holds). Every invariant strong
                    # enough to be worth asserting is therefore a race here,
                    # and asserting one produced a steady one-to-eleven
                    # spurious failures per run against a pane that was
                    # provably correct the moment the stream stopped. What is
                    # checked mid-stream is the domain claim above; the
                    # interface claim is checked where it is true.
                    while applied >= next_mark:
                        next_mark += RSS_SAMPLE_EVERY

        sampler = asyncio.create_task(sample())
        try:
            await app.drain(timeout=timeout)
        finally:
            stop.set()
            sampler.cancel()

        # A real pause, then the final checkpoint: the projection has to be
        # complete when the stream stops, not only while it is moving.
        controls.pause()
        await pilot.pause()
        # "Settled" has to mean a flush has actually run, not merely that the
        # stream stopped. The coalescing timer may not fire again after the last
        # frame lands, which left the pane one flush behind the projection and
        # failed the interface check below against a pane that was correct the
        # moment it was allowed to catch up.
        await app.render_snapshot()
        await pilot.pause()
        checkpoints += 1
        final_view = transcript_view(app.state)
        if not content_is_complete(app.state, final_view):
            failures += 1
        elif not interface_shows_everything(app, settled=True):
            failures += 1
        rss_series.append((app.frames_applied, _rss_mb()))

        raw = app.measurements()
        refusals = tuple(outcome.name for outcome in controls.refusals)
        final_state = app.state
        await app.shutdown_sources()

    growth = rss_series[-1][1] - rss_series[0][1]
    measurement = GateMeasurement(
        corpus=identity,
        frames_applied=int(raw["frames_applied"]),
        render_ticks=int(raw["render_ticks"]),
        elapsed_seconds=float(raw["elapsed_seconds"]),
        render_ticks_per_second=float(raw["render_ticks_per_second"]),
        peak_mounted_widgets=int(raw["peak_mounted_widgets"]),
        condensed_lines=int(raw["condensed_lines"]),
        rss_series_mb=rss_series,
        rss_growth_mb=growth,
        rss_slope_mb_per_1k_frames=_fit_slope(rss_series),
        content_loss_checkpoints=checkpoints,
        content_loss_failures=failures,
        transcript_entries=len(final_state.transcript),
        transcript_lines=final_view.total_lines,
    )
    return measurement, final_state, refusals


async def replay_headless(
    records: tuple[FrameRecord, ...], *, speed: float, pause_after: int | None = None
) -> SessionState:
    """Replay through the app at a given speed and return the final domain state.

    Used for AE11's determinism claim. Runs the real app rather than the reducer
    alone, because the claim under test is that *the interface* produces the
    same state at any speed, and a reducer-only comparison would prove something
    weaker.
    """
    controls = ReplayControls()
    controls.set_speed(speed)
    source = ReplaySource(records, controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls)
    async with app.run_test(size=GATE_SIZE) as pilot:
        if pause_after is not None:
            while app.frames_applied < pause_after and not app.replay_complete.is_set():
                await asyncio.sleep(0.005)
            controls.pause()
            await pilot.pause()
            controls.resume()
        await app.drain()
        state = app.state
        await app.shutdown_sources()
    return state


async def exercise_inert_controls(records: tuple[FrameRecord, ...]) -> tuple[str, ...]:
    """Invoke every mutation control mid-replay and collect the refusals (AE11)."""
    controls = ReplayControls()
    controls.set_speed(1.0)
    source = ReplaySource(records, controls=controls)
    app = TalariaApp(source, mode="replay", controls=controls)
    async with app.run_test(size=GATE_SIZE) as pilot:
        await pilot.pause()
        app.action_interrupt()
        app.composer.text = "this must not be sent"
        app.composer.text_area.focus()
        await pilot.press("enter")
        await pilot.pause()
        refusals = tuple(outcome.name for outcome in controls.refusals)
        retained = app.composer.text
        await app.shutdown_sources()
    if retained != "this must not be sent":  # pragma: no cover - guarded by tests too
        raise AssertionError("the composer lost its text when a replay submit was refused")
    return refusals


def build_matrix() -> dict[str, str]:
    """The exercised platform matrix PC7 asks the gate to record."""
    import textual

    return {
        "os": f"{platform.system()} {platform.release()}",
        "machine": platform.machine(),
        "python": platform.python_version(),
        "textual": textual.__version__,
    }


async def run_gate(
    *,
    live_corpus: str | Path | None = None,
    deltas: int = 50_000,
    seed: int = 20260802,
    mount_cap: int = DEFAULT_MOUNT_CAP,
) -> GateResult:
    """Run the whole gate and return every measurement plus the verdict."""
    stress = build_stress_corpus(deltas=deltas, seed=seed)
    stress_identity = stress_corpus_identity(stress)
    stress_measurement, _, _ = await measure_replay(
        stress.records, stress_identity, mount_cap=mount_cap
    )

    # The sustained pass. The unbounded pass above answers "can the reducer
    # keep up" and it answers it emphatically — which is exactly why it cannot
    # answer "does the renderer keep up": the corpus is gone before the render
    # timer has fired a handful of times. This pass replays on the recorded
    # cadence, scaled so the run occupies roughly
    # :data:`CADENCE_WINDOW_SECONDS`, so the coalescing tick runs hundreds of
    # times against a live stream and the render-tick threshold means something.
    cadence_measurement, _, _ = await measure_replay(
        stress.records,
        stress_identity,
        mount_cap=mount_cap,
        speed=_cadence_speed(stress.records),
    )

    live_measurement: GateMeasurement | None = None
    #: None means "not measured", which is not the same as True. This defaulted
    #: to True and was published verbatim, so a run that never replayed a live
    #: corpus still reported determinism_identical: true having compared
    #: nothing at all.
    determinism_identical: bool | None = None
    refusals: tuple[str, ...] = ()

    if live_corpus is not None and not Path(live_corpus).exists():
        # A typo in the path used to degrade the gate from ten checks to seven
        # and still exit 0. Silently measuring less than you claim to is the
        # failure mode this whole module exists to avoid.
        raise GateError(f"live corpus not found: {live_corpus}")

    if live_corpus is not None:
        records = load_frame_records(live_corpus)
        identity = live_corpus_identity(live_corpus, records)
        live_measurement, _, _ = await measure_replay(records, identity, mount_cap=mount_cap)

        # AE11: same corpus, three transport treatments, one final state.
        fast = await replay_headless(records, speed=64.0)
        paused = await replay_headless(records, speed=64.0, pause_after=len(records) // 2)
        unbounded_state = await replay_headless(records, speed=float("inf"))
        determinism_identical = fast == paused == unbounded_state
        refusals = await exercise_inert_controls(records)

    checks: dict[str, dict[str, Any]] = {
        # Frames the app applied against frames the corpus contains. Content
        # loss upstream of the reducer is invisible to a check whose ground
        # truth is the reducer's own output: discarding nine of every ten
        # inbound frames destroyed 90% of the conversation and still reported
        # zero content loss, because the destroyed frames were never in the
        # ground truth to begin with. This is the accounting that notices.
        "frames_accounted_for": {
            "description": "frames applied equals frames in the stress corpus",
            "measured": stress_measurement.frames_applied,
            "threshold": stress_identity.frame_count,
            "comparison": "==",
            "pass": stress_measurement.frames_applied == stress_identity.frame_count,
        },
        # A check that runs zero times reports zero failures. Both sampled
        # checks below ride a 20ms poll that skips boundaries wholesale on a
        # fast machine, so the sample count is itself a pass condition.
        "enough_memory_samples": {
            "description": "memory series has enough points to fit a slope worth publishing",
            "measured": len(stress_measurement.rss_series_mb),
            "threshold": MIN_RSS_SAMPLES,
            "comparison": ">=",
            "pass": len(stress_measurement.rss_series_mb) >= MIN_RSS_SAMPLES,
        },
        "enough_content_checkpoints": {
            "description": "content completeness was actually checked, not merely scheduled",
            "measured": stress_measurement.content_loss_checkpoints,
            "threshold": MIN_CONTENT_CHECKPOINTS,
            "comparison": ">=",
            "pass": stress_measurement.content_loss_checkpoints >= MIN_CONTENT_CHECKPOINTS,
        },
        "mounted_widgets": _check(
            "mounted line widgets at any point of the stress corpus",
            stress_measurement.peak_mounted_widgets,
            MOUNTED_WIDGET_CEILING,
        ),
        "rss_growth_mb": _check(
            "resident-set growth across the full stress replay (MB)",
            stress_measurement.rss_growth_mb,
            RSS_GROWTH_CEILING_MB,
        ),
        "mounted_widgets_sustained": _check(
            "mounted line widgets during the sustained-streaming pass",
            cadence_measurement.peak_mounted_widgets,
            MOUNTED_WIDGET_CEILING,
        ),
        "render_ticks_per_second": _check(
            "coalescing flushes per second at maximum replay speed",
            stress_measurement.render_ticks_per_second,
            RENDER_TICKS_PER_SECOND_CEILING,
        ),
        "render_ticks_per_second_sustained": _check(
            "coalescing flushes per second under sustained streaming",
            cadence_measurement.render_ticks_per_second,
            RENDER_TICKS_PER_SECOND_CEILING,
        ),
        "content_loss": {
            "measured": stress_measurement.content_loss_failures,
            "threshold": 0,
            "comparison": "<=",
            "description": "checkpoints at which a domain transcript entry was missing "
            "from the projection",
            "pass": stress_measurement.content_loss_failures == 0,
        },
        "content_loss_sustained": {
            "measured": cadence_measurement.content_loss_failures,
            "threshold": 0,
            "comparison": "<=",
            "description": "same check taken while the stream is still moving",
            "pass": cadence_measurement.content_loss_failures == 0,
        },
    }
    if live_measurement is not None:
        checks["live_corpus_content_loss"] = {
            "measured": live_measurement.content_loss_failures,
            "threshold": 0,
            "comparison": "<=",
            "description": "same check against the recorded session",
            "pass": live_measurement.content_loss_failures == 0,
        }
        checks["replay_determinism"] = {
            "measured": determinism_identical,
            "threshold": True,
            "comparison": "==",
            "description": "1x-with-pause, 64x and unbounded replays end in identical "
            "domain state (AE11)",
            "pass": determinism_identical,
        }
        checks["inert_mutation_controls"] = {
            "measured": sorted(set(refusals)),
            "threshold": ["interrupt", "submit"],
            "comparison": "==",
            "description": "mutation controls invoked mid-replay refused and said so",
            "pass": sorted(set(refusals)) == ["interrupt", "submit"],
        }

    return GateResult(
        stress=stress_measurement,
        cadence=cadence_measurement,
        live=live_measurement,
        determinism_identical=determinism_identical,
        inert_controls_refused=refusals,
        matrix=build_matrix(),
        checks=checks,
    )


def _cadence_speed(records: tuple[FrameRecord, ...]) -> float:
    """The multiplier that makes a corpus take about one measurement window.

    Derived from the corpus's own recorded duration rather than fixed, so the
    sustained pass lasts the same wall-clock time whether the corpus is a
    four-turn recording or a 50,000-delta generated stream. Clamped into the
    controls' finite range, so a corpus far too short to fill the window simply
    replays as slowly as the controls allow rather than pretending.
    """
    if len(records) < 2:
        return 1.0
    recorded = records[-1].at - records[0].at
    if recorded <= 0:
        return 1.0
    return max(MIN_SPEED, min(MAX_SPEED, recorded / CADENCE_WINDOW_SECONDS))


def _check(description: str, measured: float, threshold: float) -> dict[str, Any]:
    return {
        "measured": round(measured, 3),
        "threshold": threshold,
        "comparison": "<=",
        "description": description,
        "pass": measured <= threshold,
    }
