"""``TalariaApp`` — the Textual shell, and the thing the gate measures.

The prototype and the framework validation gate are the same build. That is the
plan's central bet: a gate that measures a purpose-built harness proves the
harness, so this app is the real interface and the gate simply runs it against a
bigger corpus with counters attached.

**Two clocks, deliberately separated.** Frames arrive from the source as fast as
the replay speed allows, and each one is folded into :class:`SessionState`
immediately — that path is pure and never touches a widget. Rendering happens on
a fixed coalescing tick (KTD14, ~50ms), which projects the state once and hands
the snapshot to the regions. A 50,000-token turn therefore costs 50,000 cheap
reducer calls and about twenty renders a second, instead of 50,000 renders.

**The app owns no domain logic.** It decodes (via the domain's own seam), folds,
projects, and distributes. Every question of meaning — is this turn cancelled,
which sub-agent status wins, what counts as waiting — was answered in
``talaria.domain`` where it is testable without a screen (ADR-0002).

**In replay nothing here can send.** The mutation controls route through
:meth:`ReplayControls.attempt`, which refuses and returns a notice. The gate
asserts the refusal; R30 asserts that no socket exists to refuse *to*. In live
mode the same two controls route to a :class:`LiveDispatcher` — U7's
``LiveSource`` in production, a double in tests — and the app's whole
contribution is deciding what the transcript is allowed to claim about each
outcome (R3, R4, AE8).
"""

from __future__ import annotations

import asyncio
import json
import time
from collections.abc import AsyncIterator, Callable, Iterator, Mapping
from contextlib import contextmanager
from dataclasses import dataclass, replace
from pathlib import Path
from time import time as _wall_clock
from typing import Any, ClassVar, Final, Literal, Protocol, runtime_checkable

from textual import events
from textual.app import App, ComposeResult, ScreenStackError
from textual.binding import Binding, BindingType
from textual.containers import Horizontal, Vertical
from textual.css.query import NoMatches
from textual.timer import Timer
from textual.widgets import Static

from talaria import __version__
from talaria.config import ConfigError, ThemeSaveScope, save_theme
from talaria.domain.changes import DiffSelection, InspectorView, inspector_view
from talaria.domain.commands import (
    CATALOG_METHOD,
    DISPATCH_METHOD,
    LIVE_HAS_NO_REPLAY_CLOCK,
    PASTE_COLLAPSE_METHOD,
    SLASH_EXEC_METHOD,
    SUBAGENT_INTERRUPT_METHOD,
    CommandCatalog,
    GatewayInvocation,
    LocalInvocation,
    PasteThreshold,
    SlashOutput,
    UnsupportedInvocation,
    decode_catalog,
    decode_collapsed_paste,
    decode_slash_exec,
    parse_speed,
    render_dispatch,
    render_slash_output,
    resolve_command,
    slash_exec_command,
    unavailable_catalog,
)
from talaria.domain.compat import (
    PROBE_REVALIDATION_S,
    ProbeTrigger,
    SeamBoard,
    apply_probe_round,
    board_lines,
    empty_board,
    seam_probe_due,
)
from talaria.domain.composer_history import ComposerHistory
from talaria.domain.composer_history import push as history_push
from talaria.domain.decode import (
    DispatchResult,
    UnknownDispatchResult,
    decode_dispatch_result,
)
from talaria.domain.models import (
    ConnectionStatus,
    PendingPrompt,
    PromptKind,
    QueueItem,
    RunMode,
    TerminalCause,
)
from talaria.domain.models_catalog import (
    ModelAssignmentResult,
    ProfileDirectory,
    ProviderCatalog,
)
from talaria.domain.normalize import clip_detail_line, normalize_frame
from talaria.domain.projection import (
    DEFAULT_VIEWPORT_ROWS,
    ProjectionUnavailableError,
    PromptRow,
    Snapshot,
    TranscriptView,
    entry_scoped_view,
    project,
    status_payload,
    terminal_read,
)
from talaria.domain.queue import NeedsYouQueue, decode_pending_approvals
from talaria.domain.registry import (
    RegistryRow,
    attach_displaces_client,
    next_poll_due_at,
)
from talaria.domain.selection import PickerSource
from talaria.domain.session_list import (
    SessionDirectory,
    decode_active_list,
    decode_session_list,
)
from talaria.domain.startup import StartupSelection
from talaria.domain.state import (
    APPROVAL_COMMAND_LABEL,
    DELIVERY_NOTES,
    PROMPT_EVENT_KINDS,
    REFUSED_NOT_OUTSTANDING,
    DeliveryState,
    FleetState,
    SessionState,
    activation_hydration_events,
    age_out_approvals,
    apply_active_list,
    apply_approval_pending,
    apply_frame,
    approval_detail_targets,
    attach_confirm_row,
    begin_fleet_answer,
    cancel_turn,
    end_fleet_answer,
    fleet_connection_lost,
    fleet_connection_restored,
    fleet_queue,
    fleet_row,
    fleet_seam_board,
    fleet_switch_refusal,
    land_session,
    latch_attach_residue,
    latch_resolved_prompts,
    latch_unservable_prompt,
    listing_failed,
    mark_we_drive,
    record_command_result,
    record_local_note,
    record_replayed_submission,
    record_seam_board,
    record_submission,
    replayed_submission_text,
    respond_to_all_approvals,
    respond_to_prompt,
    restore_prompt,
    roster_staleness,
    route_frame,
    seed_from_listing,
    seed_history,
    set_connection,
    settle_prompt,
    settle_queue_item,
)
from talaria.domain.state import (
    SUBMIT_METHOD as SUBMIT_METHOD,
)
from talaria.replay.controls import INERT_NOTICE, ReplayControls
from talaria.replay.source import REPLAY_EPOCH
from talaria.status.contract import StatusBarSettings
from talaria.status.local import capture_local_status
from talaria.status.runner import StatusRunner, StatusTickResult
from talaria.transport.admin import AdminError
from talaria.transport.attach import scrub_urls
from talaria.transport.compat_check import CompatReport, HealthProbe, probe_seams
from talaria.transport.connection_set import (
    # Aliased: ``talaria.domain.models`` already exports a ``ConnectionStatus``
    # and they are different things — the domain's is what the focused engine
    # believes about its own link, this one is what the transport knows about
    # one member of the fleet. Importing both under one name would make the
    # protocol below silently claim the wrong contract.
    ConnectionStatus as FleetConnectionStatus,
)
from talaria.transport.connection_set import (
    EnsureReport,
    TaggedFrame,
)
from talaria.transport.rpc import (
    LOST_WITH_TRANSPORT,
    NEVER_SENT,
    NO_REPLY_IN_TIME,
    NOT_CONNECTED,
    RpcOutcome,
)
from talaria.transport.source import FrameRecord, FrameSource, SwitchReport
from talaria.ui.agents import AgentRow, AgentRows
from talaria.ui.composer import ChatTextArea, Composer
from talaria.ui.dialog import ConfirmDialog, PickerDialog
from talaria.ui.diff_viewer import DiffViewer, adapt_diff_document
from talaria.ui.focus import CaretReleased, focused_region
from talaria.ui.inspector import Inspector
from talaria.ui.motion import MotionPolicy
from talaria.ui.needs_you import (
    ITEM_NO_LONGER_WAITING,
    NeedsSelection,
    NeedsYouPickerSource,
    decode_selection,
)
from talaria.ui.palette import PaletteRegion
from talaria.ui.picker import (
    NO_PROFILES,
    NO_PROVIDERS,
    SESSION_ALREADY_FOCUSED,
    ModelPickerSource,
    PickerMode,
    ProfilePickerSource,
    SelectableRow,
    SessionModel,
    SessionPickerSource,
    flatten_profiles,
    flatten_registry_sessions,
    flatten_selectable,
)
from talaria.ui.prompts import (
    DENY_ALL_CHOICE,
    RESPOND_METHODS,
    UNATTENDED_KINDS,
    PromptCard,
    PromptRegion,
    decline_value,
    echoable_answer,
    gateway_refusal,
    respond_params,
)
from talaria.ui.status_bar import (
    BottomStatusBar,
    BottomStatusBarView,
    build_status_bar_view,
)
from talaria.ui.status_region import StatusRegion
from talaria.ui.theme import BUILTIN_THEME_REGISTRY, DEFAULT_THEME_SLUG, ThemeRegistry
from talaria.ui.transcript import DEFAULT_MOUNT_CAP, TranscriptAnchor, TranscriptPane

#: KTD14's coalescing boundary. Deltas accumulate in the domain transcript and
#: the UI flushes on this tick rather than per token.
COALESCE_INTERVAL: Final[float] = 0.05

#: ``SUBMIT_METHOD`` — the gateway method a composed message is sent with — is
#: re-exported from :mod:`talaria.domain.state` in the import block above rather
#: than spelled again here. Both ends of it matter now: the composer writes the
#: frame, and :func:`~talaria.domain.state.replayed_submission_text` reads the
#: operator's words back out of a recorded one. Two string literals that have to
#: agree is one literal too many.

#: The gateway method that stops the in-flight turn (R4;
#: ``tui_gateway/methods_session.py:2706``). Distinct from ``subagent.interrupt``
#: (``:2806``), which stops one delegated child and belongs to U9's control.
INTERRUPT_METHOD: Final[str] = "session.interrupt"

#: The name this control is registered under in
#: :data:`~talaria.replay.controls.MUTATION_CONTROLS`, so replay refuses it
#: visibly rather than letting a prompt answer quietly go nowhere (AE11).
PROMPT_RESPOND_CONTROL: Final[str] = "prompt-respond"

#: The two U9 controls that need a gateway, under the names
#: :data:`~talaria.replay.controls.MUTATION_CONTROLS` already reserved for them.
COMMAND_DISPATCH_CONTROL: Final[str] = "command-dispatch"
PASTE_COLLAPSE_CONTROL: Final[str] = "paste-collapse"

#: Said when a command Talaria could not collapse a paste for is still editable.
#: The wording names the capability rather than the call, because "paste.collapse
#: failed" tells the operator nothing they can act on and "the paste is still
#: here, uncollapsed" tells them everything (AE13).
#:
#: **It is only said once the composer has been read**, because it is a claim
#: about the composer. The operator can submit or delete the paste while the
#: round trip is in flight, and a refusal that then announces "the paste was
#: left in full" over an empty composer is the U8 failure family: a sentence
#: asserting a state nothing checked. :data:`PASTE_COLLAPSE_REFUSED` is what is
#: said when the text is gone — the same news about the gateway, with no claim
#: about the editor attached.
PASTE_NOT_COLLAPSED: Final[str] = (
    "the paste was left in full — the gateway did not collapse it"
)
PASTE_COLLAPSE_REFUSED: Final[str] = "the gateway did not collapse the paste"

#: Said when Enter arrives while a paste is still out at the gateway.
#:
#: Paste-then-Enter is ordinary muscle memory and it is exactly what KTD16
#: exists to stop: the literal insert is the floor for every paste, so at that
#: instant the composer holds the whole several-hundred-line body and submitting
#: puts it into the turn. Talaria's insert-literal-first ordering is what opens
#: the window (Hermes's client computes its placeholder locally and never has
#: one), so Talaria closes it here rather than pretending the window is too
#: small to matter.
PASTE_COLLAPSE_IN_FLIGHT: Final[str] = (
    "a large paste is still being collapsed — nothing was sent; press Enter "
    "again in a moment"
)

#: Said when the gateway collapsed a paste that the operator has since edited
#: away. Nothing is inserted in that case; the file the gateway wrote is named
#: so the operator can reach it if they wanted it after all.
PASTE_NO_LONGER_PRESENT: Final[str] = (
    "the pasted text is no longer in the composer, so nothing was replaced; "
    "the gateway saved it at"
)

#: How the transcript names an interrupt the gateway accepted, and one it says
#: it could not find. ``found: false`` is a real answer, not a failure — the
#: child had already finished — and reporting it as an interrupt would claim an
#: act that did not happen.
SUBAGENT_INTERRUPTED: Final[str] = "interrupted sub-agent"
SUBAGENT_NOT_FOUND: Final[str] = "the gateway has no such running sub-agent:"

#: How far an ``alias`` chain is followed before Talaria stops and says so.
#:
#: Hermes's client re-dispatches an alias target through its own handler with no
#: guard at all (``createSlashHandler.ts:100-102``), which a quick command
#: aliased to itself turns into an unbounded loop. Three is well past any real
#: chain and short enough that the refusal arrives while the operator is still
#: looking at the command they typed.
ALIAS_FOLLOW_LIMIT: Final[int] = 3

#: Said when an alias points back into a chain already being followed.
ALIAS_CIRCULAR: Final[str] = "the alias chain does not end; stopped at"


#: Shown when an answer arrives for a prompt the registry no longer holds.
#:
#: Named rather than silent. The two ways to get here — the prompt expired
#: between the keystroke and the dispatch, or it belongs to a session that is no
#: longer focused — are both races the operator did nothing wrong to cause, and
#: a control that swallows a keystroke and does nothing looks exactly like one
#: that answered.
#:
#: Re-exported from the domain, where the registry that does the refusing
#: chooses the wording for each of its three refusals.
PROMPT_NO_LONGER_LIVE: Final[str] = REFUSED_NOT_OUTSTANDING

#: Shown when F5 (or ``end``) is pressed while the transcript already follows
#: the newest line (B3, KTD3).
#:
#: Both keys reach the same rule (KTD2): re-following at the bottom of a
#: paused replay is a legitimate no-op, and silence there is ambiguous —
#: exactly charter E2's observation. The ``nothing changed`` tail is the
#: pacing register's own ("this session is live — nothing changed").
ALREADY_FOLLOWING_BOTTOM: Final[str] = "the newest line is already followed — nothing changed"

#: Shown when ctrl+c is pressed with no turn in flight (A4 P1-A).
#:
#: ``ctrl+c`` is a system quit binding in Textual; this unit reclaims it for
#: the destructive interrupt, so a guard and visible feedback are load-bearing.
#: The first press when nothing is in flight is a no-op that says so, matching
#: :data:`ALREADY_FOLLOWING_BOTTOM` — a signal whose failure is
#: indistinguishable from success is worse than no signal.
NOTHING_TO_INTERRUPT: Final[str] = "nothing to interrupt — no turn is in flight"

#: Shown when F2 is pressed while the sub-agent region has no rows to show or
#: hide (B3, KTD3).
#:
#: The toggle still flips its flag on an empty press — it decides how the
#: next fan-out arrives — but the flag is invisible, so the keypress is
#: confirmed by saying there is nothing on screen for it to act on. The
#: ``-populated`` class decides visibility from the same predicate
#: (``talaria/ui/agents.py``), so the notice and the rendered region cannot
#: disagree about whether a toggle would have been seen.
AGENTS_NOTHING_TO_TOGGLE: Final[str] = "no sub-agents to show or hide — the region stays hidden"

#: Shown when a landing retains the already-focused session (B3, KTD4).
#:
#: The picker's marked row already refuses itself in the dialog with
#: ``SESSION_ALREADY_FOCUSED``; this is the same fact on the composer surface
#: for the unmarked row the picker did not recognize, so one fact keeps one
#: voice on both surfaces.
SESSION_ALREADY_FOCUSED_NOTICE: Final[str] = f"{SESSION_ALREADY_FOCUSED} — nothing changed"

#: Shown when a decline's wire value and the registry's current kind for the
#: same id no longer agree (CR4 finding 5).
#:
#: :func:`~talaria.ui.prompts.decline_value` computes the wire *value* from
#: the kind the card carried at the moment ``escape`` was pressed;
#: :meth:`~talaria.ui.app.TalariaApp.respond_live` picks the wire *method*
#: from a later, independent read of the registry's kind for that id. Nothing
#: used to check the two still named the same prompt — a registry id that
#: expired and was reused under a different kind in between would pair one
#: kind's value with another kind's method, and for approval that is the one
#: case that matters: an empty value read against the approval method is
#: *approved*, not declined (``tools/approval.py:3679``). Refused instead.
PROMPT_KIND_CHANGED: Final[str] = "the prompt changed before this could be sent — nothing was sent"

#: Shown when escape is pressed on the unanswerable (deny-all-only) card
#: (CR4 finding 6b). Named rather than silent, for the same AE11 reason
#: every other refusal in this module is: the card's own hint line
#: (:data:`~talaria.ui.prompts.DENY_ALL_HINT`) names one key, and pressing a
#: different one used to look exactly like a control that worked by doing
#: nothing visible at all.
DECLINE_NOT_OFFERED_HERE: Final[str] = (
    "escape does nothing on this card — it only offers deny all"
)

#: How a successful whole-queue denial opens. A constant so the transcript and
#: the operator's notice cannot come to say different things about the one
#: action the interface offers when nothing can be aimed.
DENIED_EVERY_APPROVAL: Final[str] = "denied every waiting approval"

#: How the deny-all line names approvals it reached but cannot speak for.
#:
#: ``all: true`` resolves every entry in the gateway's queue, including one
#: whose own ``approval.respond`` is still travelling — and that respond may
#: carry the affirmative the operator pressed a moment earlier. Which of the
#: two the gateway applies is decided by arrival order there. So they are
#: counted separately from the ones this call actually denied and the count is
#: labelled as undecided, rather than folded into a "denied" total that would
#: put two different fates for one command into the same transcript.
ANSWER_ALREADY_TRAVELLING: Final[str] = "already answered, outcome unknown"

#: What the deny-all line says when the reply carried no usable ``resolved``
#: count.
#:
#: Formatting the raw value put Python's ``None`` in front of the operator, and
#: "None resolved" reads in English as "none resolved" — the exact opposite of
#: what it meant. The gateway not answering a question and the gateway answering
#: zero are different facts about a safety action, and one of them was being
#: rendered as the other.
UNCOUNTED_RESOLUTION: Final[str] = "the gateway did not say how many it resolved"

#: Prefix for the line shown when terminal-read cannot be served. Nothing goes
#: to the gateway in this case: its bridge expires on its own after 30 seconds,
#: and silence is a supported outcome while a fabricated screen is not (KTD10).
#:
#: A *prefix* rather than the whole sentence, because the reason comes from the
#: projection's own exception and the two must not say the same thing twice. The
#: combined line is clipped at
#: :data:`~talaria.domain.normalize.TRANSCRIPT_LINE_CLIP`, which no longer sits
#: anywhere near this line's length — but the reason to keep the halves saying
#: different things is that a reader learns nothing from the repeat, which holds
#: at any bound.
TERMINAL_READ_UNAVAILABLE: Final[str] = "terminal read not answered —"


#: KTD7's three startup methods, and the read that resolves ``--resume``.
#: ``session.most_recent`` is read-only and is the one of the four a startup
#: probe is allowed to invoke; the other three are evidence-only in U3's
#: classification and are called here as the operator's *action*, not as a
#: capability probe. That distinction is the whole of R34: this code opens the
#: session the operator asked for, and never calls a method to find out whether
#: it could have.
MOST_RECENT_METHOD: Final[str] = "session.most_recent"
CREATE_METHOD: Final[str] = "session.create"
RESUME_METHOD: Final[str] = "session.resume"

#: The terminal width reported to ``session.create``/``session.resume``. Hermes
#: re-wraps stored history to this (``tui_gateway/methods_session.py:14``), and a
#: fixed 80 would re-wrap a wide terminal's history to something narrower than
#: the screen it is about to be drawn on.
DEFAULT_SESSION_COLS: Final[int] = 80

#: Said when ``--resume`` found no session to resume.
#:
#: Talaria stops rather than creating one. Silently starting a new conversation
#: for an operator who asked to return to their last one is the kind of
#: substitution that is only noticed several turns later.
NO_SESSION_TO_RESUME: Final[str] = (
    "no previous session to resume — the gateway reports none. "
    "Start a new one with a bare `talaria`."
)

#: Prefix for the line that names a session startup the gateway refused or never
#: answered. The outcome's own notice supplies the rest.
SESSION_START_FAILED: Final[str] = "could not open a session:"

#: What a ``session.resume`` landing writes to say which session arrived (B5).
#: The durable id is the one the picker names a session by and a later resume
#: asks for (KTD2); the line precedes the seeded history it introduces (KTD3a).
#: Only the real-switch branch announces — the retain branch (landing the
#: already-focused row) stays silent deliberately (KTD3b).
RESUMED_SESSION_ANNOUNCEMENT: Final[str] = (
    "resumed session {session_key} — now showing this session's conversation"
)

#: The same announcement when the reply carries no durable id at all (AE4).
#: The runtime id is named and labelled as such, because a bare string would be
#: indistinguishable from the durable id it is not.
RESUMED_SESSION_ANNOUNCEMENT_RUNTIME: Final[str] = (
    "resumed session {session_id} (runtime session id) — "
    "the gateway reported no durable session id"
)

#: Said when ``/models <n>`` is typed before anything has been fetched.
MODELS_NOT_FETCHED: Final[str] = (
    "no model list fetched yet — open the picker with a bare /models first"
)

#: Said when a selection is made after a reconnect invalidated the list it was
#: read from (KTD4, 2026-08-06 model-picker plan). Refused rather than sent: the
#: reconnect may have landed on a different gateway, and the row the operator
#: is looking at could name a provider or model that gateway does not have.
MODELS_STALE_EPOCH: Final[str] = (
    "the connection changed since this list was fetched — reopen /models to refresh"
)

#: Said when ``/profiles <n>`` is typed before anything has been fetched (U4).
PROFILES_NOT_FETCHED: Final[str] = (
    "no profile list fetched yet — open the picker with a bare /profiles first"
)

#: The profile listing is epoch-scoped for the same reason the model list is
#: (KTD4): a switch or a reconnect may have landed on a different gateway, whose
#: profile inventory is its own.
PROFILES_STALE_EPOCH: Final[str] = (
    "the connection changed since this list was fetched — reopen /profiles to refresh"
)

#: Said when the assembled session has no way to list profiles at all — a
#: replay, or a gateway whose admin surface could not be reached. Distinct from
#: "the fetch failed", which names a gateway that was asked.
PROFILES_UNAVAILABLE: Final[str] = (
    "this session cannot list profiles: no admin connection to the gateway"
)

#: Said when a switch is asked of a session that cannot make one — no
#: per-endpoint credential resolver, so KTD6 cannot be honoured. Nothing is
#: dropped, and saying so is the point: the operator is still connected.
PROFILE_SWITCH_UNAVAILABLE: Final[str] = (
    "this session cannot switch gateways — still connected to the current one"
)

#: How the transcript names a switch that closed the old connection without
#: making the new one. The report's own reason and detail follow.
PROFILE_SWITCH_FAILED: Final[str] = "profile switch failed:"

#: How the transcript names an *ensure* that did not bring a connection up
#: (v0.4 KTD1). Deliberately a different sentence from
#: :data:`PROFILE_SWITCH_FAILED`: nothing was dropped to attempt it, so the
#: operator needs to read "that profile is not connected", not "you have been
#: disconnected from something".
PROFILE_CONNECT_FAILED: Final[str] = "could not connect"

# ── U7: the session picker (KTD3, KTD6) ─────────────────────────────────────

#: The gateway method the picker's listing comes from (``tui_gateway/methods_session.py:162``,
#: pinned read-only in ``talaria/domain/compat.py`` — R10).
LIST_SESSIONS_METHOD: Final[str] = "session.list"

#: Sent explicitly on every ``session.list`` call rather than omitted — it is
#: the handler's own default (``methods_session.py:181``), and the compat
#: baseline's request fixture pins the same value, so a startup probe and an
#: operator's ``/sessions`` ask the gateway the identical question.
SESSIONS_LIST_LIMIT: Final[int] = 200

#: Said when ``/sessions`` is typed with no gateway attached at all — a
#: replay, or a live session that has not (yet, or any longer) connected.
SESSIONS_UNAVAILABLE: Final[str] = (
    "this session cannot list sessions: no gateway connection"
)

#: Prefix for the line naming a ``session.list`` call the gateway refused or
#: never answered. The outcome's own notice supplies the rest.
SESSIONS_LIST_FAILED: Final[str] = "could not list sessions:"

#: Said when the connection changed between issuing ``session.list`` and its
#: reply landing. Unlike ``MODELS_STALE_EPOCH``/``PROFILES_STALE_EPOCH`` this
#: is not about a *cached* listing going stale — the picker never caches one
#: (``talaria/ui/picker.py:SessionPickerSource``) — it is about the reply
#: itself having answered for a gateway Talaria is no longer talking to by
#: the time it lands.
SESSIONS_STALE_EPOCH: Final[str] = (
    "the connection changed while sessions were being listed — try /sessions again"
)

#: Said for an empty, but successfully fetched, listing.
NO_SESSIONS: Final[str] = "the gateway reports no sessions to switch to"

#: Said when a switch is chosen while a previous ``session.create``/
#: ``session.resume`` is still on the wire (C1, U7 round three). Refuses the
#: *send*, not just the reply — see :attr:`TalariaApp._resume_in_flight`.
SWITCH_ALREADY_IN_FLIGHT: Final[str] = (
    "a session switch is already on the wire — wait for it to land, then try again"
)

# ── v0.4 U4: the registry-backed picker, the steal confirm, the handover ────

#: The connection name a single-gateway run's registry rows are keyed under.
#: The registry key is ``(profile, durable_id)``, so this half has to be stable
#: for the whole run or one gateway's sessions become two sets of rows.
DEFAULT_PROFILE: Final[str] = "default"

#: The gateway's live roster (``tui_gateway/methods_session.py:986``). It is
#: **not** gated behind a version check: U1's enumeration found it registered
#: at the old pin, at the checkout, and on the wire this machine serves — the
#: plan's KTD4 sentence calling it new was wrong, and building a version gate
#: on it would refuse a roster from gateways that have always had one. It is
#: probed the only honest way instead: it is called, and a call that fails
#: leaves the rows saying what they actually know (R10/R24).
LIST_ACTIVE_METHOD: Final[str] = "session.active_list"

#: Feed B's wire call (KTD11). Distinct from the bare presence probe of the same
#: name that ``compat_check`` issues: that one carries no parameters at all, and
#: MUST NOT, because a bare call cannot resolve a session and so cannot warm one.
#: This one names a session, which is the whole difference — see
#: :meth:`TalariaApp.poll_approval_detail` for what makes that acceptable.
APPROVAL_PENDING_METHOD: Final[str] = "approval.pending"

#: How often the poll loop asks whether any connection is due. A floor on the
#: cadence rather than the cadence itself: :func:`~talaria.domain.registry.
#: next_poll_due_at` is re-checked on every tick, so a faster timer cannot make
#: the fleet poll faster than KTD2's 2-second coalesce allows — the same
#: relationship ``PROBE_REVALIDATION_S`` has with ``seam_probe_due``.
POLL_TICK_S: Final[float] = 1.0

#: Said when the roster call failed and the picker fell back to identity-only
#: rows. Names the absent capability rather than showing lifecycle zeroes.
ROSTER_UNAVAILABLE: Final[str] = (
    "roster unavailable: session.active_list did not answer — rows show identity "
    "only, not what each session is doing"
)

#: Said when the roster failed *this* time but answered before. The rows do
#: show lifecycle, so the sentence above would be false; what they cannot say is
#: how old it is. Two sentences because there are two situations, and reporting
#: the second as the first told the operator the rows showed identity only while
#: displaying a lifecycle for every one of them.
ROSTER_STALE: Final[str] = (
    "roster stale: session.active_list did not answer — lifecycle below is from "
    "an earlier sweep and may have changed since"
)

#: Added to OP2's dialog when the roster could not confirm the session is still
#: live. The confirmation still fires: not asking would risk a silent steal,
#: which is the worse error. What changes is that it stops implying a sighting
#: it does not have (R24).
ATTACH_CONFIRM_STALE_LIVENESS: Final[str] = (
    "the roster has not confirmed this session since an earlier sweep, so it may "
    "already have ended"
)

#: The title of OP2's confirmation. Names the session and the consequence in
#: one line, because that line is what an operator reads before pressing enter.
ATTACH_CONFIRM_TITLE: Final[str] = "this session is live and Talaria did not open it"

#: The consequence, stated as both possibilities. Rows carry no transport
#: identity field, so a detached orphan and a session another client is
#: actively driving are indistinguishable from here (KTD8) — the copy says so
#: rather than asserting the one that would sound more decisive.
ATTACH_CONFIRM_BODY: Final[str] = (
    "this session may be attached to another client; focusing it here detaches "
    "that client — it stops receiving this session's events and is told nothing"
)

#: The extra sentence for a queue item whose kind the gateway never named
#: (KTD8's ``unobserved`` clause). A polled row reports only the flattened
#: ``waiting``, and the activate reply hydrates approvals and clarifications
#: alone, so the operator is confirming a steal that may recover nothing.
ATTACH_CONFIRM_UNKNOWN_KIND: Final[str] = (
    "the gateway reports only that it is waiting: the kind of prompt is unknown, "
    "and it may not be recoverable after the attach"
)

#: How much of a gateway-supplied session title the confirmation's own title
#: line carries. The modal is fixed-size and the title's length is the
#: gateway's choice, not Talaria's.
CONFIRM_TITLE_CLIP: Final[int] = 48

#: Said when the operator backed out of that dialog. Fire-and-observe: nothing
#: was sent, and nothing on screen moved optimistically either.
ATTACH_DECLINED: Final[str] = "nothing was sent — the session was left where it is"

# ── U5: the default-model write and its two-act confirmation (KTD7) ────────

#: Said when ``/models <n> default`` is asked before Talaria knows which
#: profile it is connected to — no ``switcher.switch_to_endpoint`` has ever
#: run, or the session was started without one named. There is no profile to
#: scope the write to, so nothing is sent.
MODEL_DEFAULT_NO_PROFILE: Final[str] = (
    "no profile is selected for this session — the default model has nowhere to write to"
)

#: Said when the assembled session has no way to write a default at all — a
#: replay, or an admin client that predates U5's write. Distinct from a write
#: that was attempted and failed, the same distinction ``PROFILES_UNAVAILABLE``
#: draws for the read side.
MODEL_DEFAULT_UNAVAILABLE: Final[str] = (
    "this session cannot set a default model: no admin connection to the gateway"
)

#: The suffix every successful or confirm-required default-model notice
#: carries. R4: this affects **new sessions only**, not the one on screen —
#: stated here once so the two call sites that need it cannot say it
#: differently. Hermes's own docstring for ``POST /api/model/set`` names this
#: as the mistake operators make ("the currently running chat PTY … is not
#: affected"), which is exactly why it is said rather than assumed obvious.
MODEL_DEFAULT_NEW_SESSIONS_ONLY: Final[str] = (
    "this changes new sessions only — the running session keeps its current model"
)

#: How the transcript names a default-model write the gateway refused for a
#: reason other than the expensive-model guard (a 400, an unreachable
#: profile, or any other :class:`~talaria.transport.admin.AdminError`).
MODEL_DEFAULT_FAILED: Final[str] = "could not set the default model:"

#: How the transcript names the second act ``/models <n> default confirm``
#: takes once KTD7's guard has already been shown once.
MODEL_DEFAULT_CONFIRM_HINT: Final[str] = "confirm"

#: How the transcript names the compatibility check's blocking rows. The verdict
#: document is where "not ready" is decided; this is the operator's copy of the
#: same evidence, at the moment it is discovered.
COMPAT_BLOCKED: Final[str] = "gateway compatibility check found a gap:"

#: How the transcript names a frame source that failed rather than ended.
STREAM_FAILED: Final[str] = "the frame stream failed —"

#: How the transcript names a background task that died of an exception. The
#: label that follows says which one, because "the startup sequence never ran"
#: and "the catalogue never arrived" leave very different clients behind.
BACKGROUND_FAILED: Final[str] = "a background task failed —"

#: Process exit code for that failure. Distinct from 0 so a supervisor can tell
#: a stream that ended from one that broke, and distinct from 1 so it is not
#: confused with a usage error.
STREAM_FAILURE_EXIT_CODE: Final[int] = 70


#: What the composer says for each transport state that is not ``connected``.
#: R35 asks these be *distinct and visible*; a single "not connected" line for
#: all four would satisfy the letter and lose the only information the operator
#: needs to know what to do next.
_CONNECTION_NOTICE: Final[Mapping[str, str]] = {
    "disconnected": "disconnected from the gateway",
    "connecting": "connecting to the gateway…",
    "connected": "",
    "reconnecting": "connection lost — reconnecting…",
    "auth_failed": "authentication failed — the gateway rejected the credential",
}


#: Which claim the transcript is allowed to make about a submitted message, for
#: each reason the correlator can resolve a call with (``talaria.transport.rpc``).
#:
#: The correlator already knows why a call ended without an answer; this table is
#: only the translation from its reason to what the transcript may say. Without
#: it the UI collapsed all four onto ``outcome.confirmed`` — one boolean, one
#: hardcoded sentence — and a submit attempted with no connection was written
#: into the transcript as possibly delivered.
_DELIVERY_BY_REASON: Final[Mapping[str, DeliveryState]] = {
    NOT_CONNECTED: "not_sent",
    # NEVER_SENT is deliberately *not* "not_sent". It is set when
    # ``connection.send()`` raises, and a send can raise after a partial write
    # — so the write failing is known, and "nothing reached the gateway" is
    # not. Mapping it to "not_sent" would print "send it again" over a message
    # that may have arrived, which is the same overclaim as the hardcoded
    # "the connection dropped" line, pointing the other way.
    NEVER_SENT: "unknown",
    NO_REPLY_IN_TIME: "no_reply",
    LOST_WITH_TRANSPORT: "connection_lost",
}


def delivery_of(outcome: RpcOutcome) -> DeliveryState:
    """Translate one call outcome into the delivery claim it earns.

    An unrecognized (or absent) reason falls back to ``"unknown"``, whose line
    names no cause at all. That is the conservative direction in both senses
    that matter: it never invents a cause, and it never invites the resend that
    ``"not_sent"`` invites — a resend is only safe when the message is *known*
    not to have gone out, and a reason nobody recognizes is not that.
    """
    if outcome.confirmed:
        return "confirmed"
    if outcome.reason is None:
        return "unknown"
    return _DELIVERY_BY_REASON.get(outcome.reason, "unknown")


#: What one ``*.respond`` call turned out to be, in the four kinds a caller has
#: to treat differently.
#:
#: ``used`` is the only one that may be written down as an answer.
AnswerDisposition = Literal["error", "not_sent", "discarded", "used"]


@dataclass(frozen=True)
class AnswerVerdict:
    """The single reading of a respond outcome that every answer path shares.

    **A shared choke point, not a shared helper.** Two paths answer prompts —
    one prompt at a time, and the whole approval queue at once — and each has
    to combine three independent signals to decide what the transcript may
    claim: the JSON-RPC envelope (did the call fail), U7's delivery table (was
    it acknowledged), and the reply *body* (did the gateway use the answer or
    throw it away). Written twice, the two readings disagreed, and they
    disagreed in the direction that matters: deny-all read none of the three
    and reported a denial the gateway had discarded as applied. LEARNINGS
    already carries the rule from a redaction defect of the same shape — a
    sanitizer attached to one selection rule is not a boundary — so the fix is
    one function both call rather than a second correct copy.

    ``reason`` is the clause the transcript puts after the em dash, and it is
    ``None`` exactly when the gateway confirmed it used the answer. ``restore``
    is true only for ``not_sent``, the one outcome that is *definite* about
    non-delivery and therefore the only one where re-offering the question
    cannot deliver a second answer to it.
    """

    disposition: AnswerDisposition
    reason: str | None = None

    @property
    def restore(self) -> bool:
        return self.disposition == "not_sent"

    @property
    def used(self) -> bool:
        return self.disposition == "used"


def _reply_resolved_count(outcome: RpcOutcome) -> int | None:
    """How many queue entries the gateway's own ``approval.respond`` reply
    says it resolved, or ``None`` when the reply carried no usable count.

    ``resolve_gateway_approval`` (``tools/approval.py``) returns exactly the
    length of the queue snapshot it took under its own lock, and the
    ``approval.respond`` handler (``tui_gateway/methods_prompt.py``) puts it
    on the wire verbatim as ``{"resolved": <int>}``. It is the one place the
    gateway's real, internal queue size at resolution time is visible to the
    client at all — frame arrival order on the wire says only what Talaria
    observed, not what the gateway's own snapshot held
    (:meth:`TalariaApp.deny_all_approvals_live`'s P1, U7 round four).
    """
    body = outcome.result if isinstance(outcome.result, Mapping) else {}
    resolved = body.get("resolved")
    if isinstance(resolved, int) and not isinstance(resolved, bool):
        return resolved
    return None


def _resolved_clause(outcome: RpcOutcome) -> str:
    """How many queue entries the gateway says it released, or that it did not say.

    ``resolved`` can honestly be smaller than the number Talaria was showing —
    an approval that timed out server-side leaves no trace here — so the count
    is reported rather than assumed. What it can never be is ``None`` spelled
    out: a missing count is the gateway declining to answer, and the sentence
    has to say that rather than print a Python literal that reads as zero.
    """
    count = _reply_resolved_count(outcome)
    if count is not None:
        return f"{count} resolved"
    return UNCOUNTED_RESOLUTION


def read_answer(kind: PromptKind, outcome: RpcOutcome) -> AnswerVerdict:
    """Decide what one answered prompt is allowed to claim. See
    :class:`AnswerVerdict` for why this exists once."""
    if outcome.status == "error":
        return AnswerVerdict("error", outcome.notice)
    delivery = delivery_of(outcome)
    if delivery == "not_sent":
        return AnswerVerdict("not_sent", DELIVERY_NOTES["not_sent"])
    refusal = gateway_refusal(kind, outcome.result) if outcome.confirmed else None
    if refusal is not None:
        return AnswerVerdict("discarded", refusal)
    # Confirmed leaves ``reason`` unset; every unconfirmed delivery carries its
    # own note, which is what makes "delivery unconfirmed" appear on both paths
    # for the same transport condition instead of on only one of them.
    return AnswerVerdict("used", DELIVERY_NOTES.get(delivery))


@runtime_checkable
class LiveDispatcher(Protocol):
    """The one thing the UI needs from the live transport: an honest call.

    Declared here rather than imported from ``talaria.transport`` so the UI
    depends on a shape instead of on a class, and so a test can drive the live
    paths with a five-line double. ``LiveSource.call`` satisfies it.
    """

    async def call(
        self,
        method: str,
        params: Mapping[str, Any] | None = None,
        *,
        timeout: float | None = None,
    ) -> RpcOutcome: ...


@runtime_checkable
class ModelAdmin(Protocol):
    """The one thing the picker needs from the admin transport (KTD1, U1).

    Declared here for the same reason :class:`LiveDispatcher` is: the UI
    depends on a shape rather than on
    :class:`~talaria.transport.admin.AdminClient` itself, so a test drives the
    picker with a small double instead of resolving a real HTTP origin.
    """

    async def model_options(self, *, profile: str | None = None) -> ProviderCatalog: ...


@runtime_checkable
class ProfileAdmin(Protocol):
    """The one thing the profile picker needs from the admin transport (U4).

    Kept separate from :class:`ModelAdmin` rather than folded into it, and the
    reason is a real state and not tidiness: an admin client that can read
    models but not profiles is what a gateway too old to serve
    ``GET /api/profiles`` produces, and it is what every U2-era test double
    already is. The app asks structurally — ``isinstance(client, ProfileAdmin)``
    — so that case renders "profiles unavailable" instead of raising
    ``AttributeError`` three layers down.

    There is deliberately no write method here. KTD5: Talaria never calls
    ``POST /api/profiles/active``, and a protocol that named such a method
    would be the seam through which one arrives.
    """

    async def list_profiles(self) -> ProfileDirectory: ...


@runtime_checkable
class EndpointSwitcher(Protocol):
    """Retargets the live transport at a different gateway (U4, KTD5/KTD6).

    Satisfied by :class:`~talaria.transport.source.LiveSource`. Declared as a
    shape for the same reason :class:`LiveDispatcher` is: a test proves what
    Talaria does with each outcome by choosing the outcome, not by standing up
    a second gateway to provoke it.

    **This is the single-connection primitive, not the fleet path (v0.4 KTD1).**
    Retargeting one socket means dropping whatever it was connected to, and a
    fleet client must not drop a gateway in order to look at another — the
    connection it just dropped is the only feed for that gateway's sessions.
    When a :class:`ConnectionEnsurer` is present it is used instead; this
    remains for a Talaria holding exactly one connection.
    """

    async def switch_to_endpoint(self, endpoint: str) -> SwitchReport: ...


@runtime_checkable
class ConnectionEnsurer(Protocol):
    """Brings one profile's connection up without touching the others (KTD1).

    Satisfied by :class:`~talaria.transport.connection_set.ConnectionSet`.
    Declared as a shape here for the same reason every other transport seam in
    this module is: the interface asserts what it does with each outcome by
    choosing the outcome, not by standing up a second gateway to provoke it.
    """

    async def ensure(self, profile: str) -> EnsureReport: ...

    @property
    def home(self) -> str: ...


@runtime_checkable
class TaggedFrameSource(Protocol):
    """A frame stream whose items carry the connection they crossed.

    The fleet shape of :class:`~talaria.transport.source.FrameSource`, satisfied
    by :class:`~talaria.transport.connection_set.ConnectionSet` live and, since
    U8, by :class:`~talaria.replay.source.TaggedReplaySource` for a version-2
    recording. Declared as a separate protocol rather than by widening
    ``FrameSource`` because the two are genuinely different contracts.

    **This used to say replay "can never satisfy this one: a recording is a
    single stream by construction and has no connection identity to carry".**
    That was true of the v0.3 format and false from U2, which gave the frame log
    a per-frame ``profile`` — the sentence outlived the format it described by
    two units. A *version-1* recording still has no connection identity and
    still satisfies ``FrameSource`` instead, which is the surviving half.
    """

    def __aiter__(self) -> AsyncIterator[TaggedFrame]: ...

    async def close(self) -> None: ...


@runtime_checkable
class ConnectionFleet(Protocol):
    """A :class:`ConnectionEnsurer` that also dials its whole inventory at once.

    Separate from ``ConnectionEnsurer`` because the two are needed at different
    moments and by different callers: ``/profiles`` ensures ONE connection and
    has always been able to, while only the launch dials them ALL, and only once.
    Declared as a shape for the same reason every other transport seam in this
    module is — a test drives it with a double rather than standing up N
    gateways. Satisfied by
    :class:`~talaria.transport.connection_set.ConnectionSet`.
    """

    async def start(self) -> Any: ...


@runtime_checkable
class ConnectionInventory(Protocol):
    """Every connection by name, a dispatcher aimed at each, and each one's epoch.

    The third seam onto :class:`~talaria.transport.connection_set.ConnectionSet`,
    and the one that makes a *non-focused* connection askable. ``ConnectionEnsurer``
    brings one up and ``ConnectionFleet`` dials them all; neither lets the app put
    a question to a connection it is not looking at, which is what a fleet-wide
    seam probe and a fleet-wide roster sweep both need.

    ``status_of`` is here for its ``epoch`` alone. That number is the connection's
    own socket generation — the same one every :class:`TaggedFrame` off that
    connection carries — and it is not interchangeable with
    :attr:`_connection_epoch`, which counts *focused* connects across the whole
    fleet. See :meth:`TalariaApp._socket_generation`.
    """

    @property
    def profiles(self) -> tuple[str, ...]: ...

    def source_for(self, profile: str) -> LiveDispatcher | None: ...

    def status_of(self, profile: str) -> FleetConnectionStatus | None: ...


@runtime_checkable
class DeclaredConnections(Protocol):
    """A frame source that knows which connections its frames came from (U8).

    Satisfied by :class:`~talaria.replay.source.TaggedReplaySource`, whose
    recording declares them in its header. Declared as a shape here for the
    reason every other seam in this module is, and for one of its own: the live
    fleet learns its connections by dialling them, and a recording learns them by
    having been written — two different facts that happen to answer one question.
    """

    @property
    def connections(self) -> tuple[str, ...]: ...


@runtime_checkable
class ModelDefaultWriter(Protocol):
    """The one thing the picker's "set as default" act needs (U5, KTD1).

    Kept separate from :class:`ModelAdmin` for the same structural reason
    :class:`ProfileAdmin` is kept separate from it: a client that can read the
    catalogue but not write a default is a real state — every U2-era test
    double already is exactly that — and the app asks structurally
    (``isinstance(client, ModelDefaultWriter)``) so that case renders
    "cannot set a default" instead of raising ``AttributeError``.
    """

    async def set_default_model(
        self,
        *,
        profile: str,
        provider: str,
        model: str,
        confirm_expensive_model: bool,
    ) -> ModelAssignmentResult: ...


class HelpBar(Static):
    """One-row binding listing, scoped to the current mode (A4 KTD4).

    A eaten key sends no bytes, so the program cannot detect that it was eaten
    and cannot warn at runtime. The correction is a redundant primary path and
    a static listing that names both, before the key is pressed. The listing is
    scoped so replay does not advertise live keys and live does not advertise
    the three replay pacing keys as if they toggle something.
    """

    DEFAULT_CSS = """
    HelpBar {
        height: 1;
        color: $text-muted;
        text-wrap: nowrap;
        text-overflow: ellipsis;
    }
    """

    def __init__(self, **kwargs: object) -> None:
        super().__init__("", markup=False, **kwargs)  # type: ignore[arg-type]
        self._help_text = ""

    def update_for_mode(self, mode: str) -> None:
        from talaria.ui.literal import literal_text  # local import to avoid cycle

        if mode == "replay":
            # Replay: keep the pacing controls and the inspector's reliable
            # routes visible without clipping at the standard 80-column size.
            text = (
                "ctrl+b inspector · / commands · F8 pause · "
                "F9/F10 speed · F1/F2 eaten on macOS"
            )
        else:
            # Live: pacing keys are inert, so not advertised. The inspector's
            # global chord is rendered beside its slash-command fallback.
            text = (
                "ctrl+b inspector · / commands · ctrl+g · ctrl+c · "
                "F1/F2 eaten on macOS"
            )
        self._help_text = text
        self.update(literal_text(text))

    @property
    def help_text(self) -> str:
        return self._help_text


class TalariaApp(App[None]):
    """The replay-driven shell: transcript, sub-agent rows, status region, composer."""

    TITLE = "talaria"

    CSS = """
    Screen {
        layout: vertical;
    }
    #body {
        height: 1fr;
        width: 1fr;
    }
    #main-and-inspector {
        height: 1fr;
        width: 1fr;
    }
    """

    BINDINGS: ClassVar[list[BindingType]] = [
        Binding("ctrl+q", "quit", "quit", priority=True),
        Binding("ctrl+b", "toggle_inspector", "inspector", priority=True),
        # A4 KTD1/KTD2: F1 removed — the focus-owning card (A1) is the anchor,
        # so the jump has no job on this desktop. On macOS F1/F2 are eaten before
        # Talaria sees them; a eaten key sends no bytes and the program cannot
        # detect it, so every eaten action gets a non-function-key primary.
        # F2/F4 remain as aliases where the desktop delivers them (KTD3).
        Binding("ctrl+g", "toggle_agents", "sub-agents", priority=True),
        Binding("f2", "toggle_agents", "sub-agents", priority=True, show=False),
        Binding("ctrl+c", "interrupt", "interrupt", priority=True),
        Binding("f4", "interrupt", "interrupt", priority=True, show=False),
        Binding("f8", "toggle_pause", "pause/resume", priority=True),
        Binding("f9", "slow_down", "slower", priority=True),
        Binding("f10", "speed_up", "faster", priority=True),
        # F3/F5/F6/F7 stay as secondary aliases; slash / and /models etc are
        # the primaries (KTD2). show=False so help footer is the listing, not
        # the auto-footer.
        Binding("f3", "toggle_palette", "commands", priority=True, show=False),
        Binding("f5", "follow_bottom", "follow", priority=True, show=False),
        # ``/models`` is the way in (U2); this is only for symmetry with
        # ``f3``/``toggle_palette`` — the two foldable regions are wired the
        # same way, so an operator's habit of reaching for a function key
        # works for either. Unlike the palette, this key never fetches: the
        # model catalogue is read once per connection epoch (KTD4), tied to
        # ``connected`` rather than to being asked for.
        Binding("f6", "toggle_picker", "models", show=False),
        # ``/profiles`` is the way in (U4); f7 is the same symmetry argument as
        # f6 one line up. Neither key fetches: both listings are read once per
        # connection epoch (KTD4), tied to ``connected`` rather than to being
        # asked for.
        Binding("f7", "toggle_profiles", "profiles", show=False),
    ]

    def __init__(
        self,
        source: FrameSource | TaggedFrameSource,
        *,
        mode: RunMode = "replay",
        controls: ReplayControls | None = None,
        status_runner: StatusRunner | None = None,
        status_interval: float = 5.0,
        status_bar_settings: StatusBarSettings | None = None,
        coalesce_interval: float = COALESCE_INTERVAL,
        mount_cap: int = DEFAULT_MOUNT_CAP,
        dispatcher: LiveDispatcher | None = None,
        admin_client: ModelAdmin | None = None,
        admin_factory: Callable[[str], ModelAdmin | None] | None = None,
        switcher: EndpointSwitcher | None = None,
        connections: ConnectionEnsurer | None = None,
        profile_endpoints: Mapping[str, str] | None = None,
        current_profile: str = "",
        call_timeout: float | None = 30.0,
        paste_threshold: PasteThreshold | None = None,
        startup: StartupSelection | None = None,
        theme_name: object = DEFAULT_THEME_SLUG,
        theme_registry: ThemeRegistry | None = None,
        reduced_motion: bool = False,
        startup_notices: tuple[str, ...] = (),
        theme_config_dir: Path | None = None,
        launch_cwd: Path | None = None,
    ) -> None:
        super().__init__()
        self.theme_registry = theme_registry or BUILTIN_THEME_REGISTRY
        resolved_theme = self.theme_registry.resolve(theme_name)
        self.theme_registry.register(self)
        self.configured_theme_slug = resolved_theme.slug
        self.session_theme_slug: str | None = None
        self.theme_config_dir = theme_config_dir
        self.launch_cwd = launch_cwd if launch_cwd is not None else Path.cwd()
        # One restart-scoped value is injected into every motion-aware widget.
        # It never changes in response to a file edit or session command.
        self.motion = MotionPolicy(reduced=reduced_motion)
        self.animation_level = "none" if reduced_motion else self.animation_level
        self._theme_preview_anchor: TranscriptAnchor | None = None
        self._startup_notices = (*startup_notices, *resolved_theme.notices)
        self._status_notices = tuple(
            notice for notice in startup_notices if notice.startswith("status")
        )
        # Registration, selection, and notices are all resolved before mount,
        # so the first frame cannot flash Textual's stock theme or hide a
        # configured fallback until a later render tick.
        self.theme = resolved_theme.slug
        self.source = source
        self.mode: RunMode = mode
        self.controls = controls if controls is not None else ReplayControls()
        self.status_runner = status_runner
        self.status_interval = status_interval
        self.status_bar_settings = (
            status_bar_settings if status_bar_settings is not None else StatusBarSettings()
        )
        self.local_status = capture_local_status(self.launch_cwd)
        self.coalesce_interval = coalesce_interval
        self.mount_cap = mount_cap
        self.dispatcher = dispatcher
        #: The admin HTTP surface (KTD1, U1) — ``None`` for replay and for
        #: every test that does not care about the picker, the same shape
        #: :attr:`dispatcher` already takes.
        self.admin_client = admin_client
        #: Builds the admin client for a *different* endpoint (U4). Called
        #: after a successful switch, because :class:`AdminClient` derives its
        #: origin once at construction: without this the picker would keep
        #: reading the gateway Talaria has just stopped talking to, and would
        #: do it silently — the worst shape of wrong, since the listing still
        #: renders and still looks current.
        self.admin_factory = admin_factory
        #: The live transport, when it can be retargeted (U4). ``None`` in
        #: replay and in every test that has no second gateway to reach.
        self.switcher = switcher
        #: The fleet's connection set (v0.4 KTD1), when there is one. Present
        #: means ``/profiles`` **ensures** a connection — brings it up beside
        #: the ones already open and makes it the home for the next create or
        #: resume — instead of drop-switching the single socket. ``None`` keeps
        #: the v0.3 behaviour, which is still correct for a Talaria holding
        #: exactly one connection.
        self.connections = connections
        #: Talaria's own name-to-gateway-URL map for profiles. Hermes publishes
        #: no endpoint for a profile, so this is the only source of one — see
        #: ``talaria/transport/admin.py``'s docstring.
        self.profile_endpoints: Mapping[str, str] = dict(profile_endpoints or {})
        #: Which profile this session believes it is connected to, or ``""``
        #: when nothing said. Used only to mark a row, never to decide one.
        self.current_profile = current_profile
        #: KTD16's bounds, resolved from configuration by the caller.
        self.paste_threshold = (
            paste_threshold if paste_threshold is not None else PasteThreshold()
        )
        #: The gateway's slash inventory, or ``None`` until it has been asked
        #: for. ``None`` and "fetched, and it failed" are different facts and
        #: the palette renders them differently, so they are not collapsed into
        #: one empty catalogue here.
        self.catalog: CommandCatalog | None = None
        #: The admin model catalogue (U2), or ``None`` until it has been read
        #: for this connection epoch. Unlike :attr:`catalog`, this is
        #: invalidated and re-read on *every* reconnect (KTD4) rather than kept
        #: across one — see :meth:`fetch_model_catalog`.
        self.model_catalog: ProviderCatalog | None = None
        #: Why the last model-catalogue read failed, or ``""`` when it did not.
        #: Kept beside :attr:`model_catalog` rather than folded into it because
        #: :class:`~talaria.domain.models_catalog.ProviderCatalog` is a pure
        #: decode with no ``available``/``failure`` pair of its own — that
        #: vocabulary lives in :class:`~talaria.transport.admin.AdminError`, one
        #: layer below the decode (R7).
        self.model_catalog_failure = ""
        #: The model switch Talaria last made on a session, or ``None`` when it
        #: has not made one. This is not redundant with :attr:`model_catalog`
        #: and cannot be replaced by refetching it: the catalogue's ``model``
        #: and ``provider`` fields are the *profile's* configured default —
        #: read off disk by ``load_picker_context()`` — while ``/models``
        #: switches the running session and nothing else, so the catalogue
        #: answers identically before and after. :class:`SessionModel` carries
        #: the session id it belongs to so a switch is never attributed to a
        #: session that did not receive it.
        self.session_model: SessionModel | None = None
        #: The gateway's profile directory (U4), or ``None`` until it has been
        #: read for this connection epoch. Re-read on every ``connected``
        #: transition for the same KTD4 reason the model catalogue is, and
        #: with more force: a switch is *by definition* a landing on a
        #: different gateway, whose profile inventory is its own.
        self.profiles: ProfileDirectory | None = None
        #: Why the last profile read failed, or ``""``. Held beside
        #: :attr:`profiles` for the same R7 reason :attr:`model_catalog_failure`
        #: is held beside :attr:`model_catalog`.
        self.profiles_failure = ""
        #: KTD7's resolved startup path, or ``None`` when the caller does not
        #: want a session opened — which is every test that drives the app with
        #: a dispatcher double, and replay, where there is nothing to open.
        self.startup = startup
        #: The startup compatibility check's report, or ``None`` until it has
        #: run. ``None`` and "ran, and found gaps" are different facts, so they
        #: are not collapsed (R34, AE7).
        self.compat: CompatReport | None = None
        #: The focused connection's seam board is **not** a field here: it lives
        #: in the fleet, one per connection, and :attr:`seams` reads and writes
        #: this connection's entry (U6's answer to U5's deferred question). A
        #: connection with no entry yet answers a board of never-observed seams,
        #: which is the honest state before a probe has run — not an empty board,
        #: and not a board of absences.
        #: The seam lines currently on screen, or ``None`` before the board has
        #: ever been painted. The two are different facts and the difference is
        #: load-bearing: ``None`` is what keeps the render tick from putting four
        #: never-observed rows on screen in replay, where no probe runs at all.
        self._seam_lines: tuple[str, ...] | None = None
        #: How many ``paste.collapse`` round trips are outstanding. Non-zero
        #: means the composer still holds a literal body that is about to be
        #: replaced, so Enter must not put it into the turn.
        self._collapses_in_flight = 0
        #: The last notice the paste-collapse path wrote, so a later success
        #: clears its own failure line and nobody else's.
        self._last_paste_notice = ""
        #: How long a live call waits before reporting an ``unknown`` outcome.
        #: Bounded because the gateway's own blocking bridges expire at 30s
        #: (``tui_gateway/server.py:2981-2998``), so a call still outstanding
        #: after that is not going to be answered by waiting longer.
        self.call_timeout = call_timeout

        #: The fleet root (v0.4 KTD3). The focused engine lives inside it as
        #: :attr:`FleetState.focused`, which is what :attr:`state` reads and
        #: writes — see that property for why the sixty-odd ``self.state = …``
        #: assignments in this file did not have to change.
        self._fleet = FleetState(
            focused=SessionState(), focused_profile=current_profile or DEFAULT_PROFILE
        )
        for notice in self._startup_notices:
            self.state = record_local_note(
                self.state, notice, at=self.state.last_observed_at
            )
        #: The poll epoch the two listing sweeps share. The dual-listing
        #: retirement rule only fires when ``session.list`` and
        #: ``session.active_list`` agree on an epoch, so both sweeps of one
        #: picker open carry this number and a sweep that failed carries none.
        self._poll_epoch = 0
        self.composer_history = ComposerHistory()
        self.snapshot: Snapshot | None = None
        self._inspector_view: InspectorView | None = None

        self._dirty = True
        self._teardown_started = False
        self._coalesce_timer: Timer | None = None
        #: The five-minute seam revalidation timer (U5). Separate from the
        #: render tick above so the two cadences cannot be confused for one.
        self._seam_timer: Timer | None = None
        #: KTD2's poll timer (U8B). Stopped in teardown beside the other two —
        #: a timer left running past shutdown reaches a torn-down inventory.
        self._poll_timer: Timer | None = None
        self._pump_task: asyncio.Task[None] | None = None
        #: KTD2's landing barrier: how many session landings are in flight.
        #:
        #: A counter rather than a flag so a nested landing (a switch issued
        #: while startup's own resume is still travelling) cannot have its
        #: inner exit release the outer one's hold.
        self._landing_depth = 0
        #: Inbound frames held while a landing is in flight, in arrival order.
        #: Frames held by the landing barrier, each with the connection tag it
        #: arrived under. The tag has to ride the deferral: a frame flushed
        #: after the seed must still route to the connection that sent it, and
        #: re-deriving the profile at flush time would attribute it to whichever
        #: connection happened to be focused then.
        #: The fleet's concurrent opening dial, when a connection set is present.
        self._fleet_task: asyncio.Task[Any] | None = None
        self._deferred_frames: list[tuple[FrameRecord, str | None, int | None]] = []
        self._status_task: asyncio.Task[None] | None = None
        self._catalog_task: asyncio.Task[None] | None = None
        #: The model catalogue's own fetch task, held separately from
        #: :attr:`_catalog_task` for the same "not an operator's call" reason.
        self._model_catalog_task: asyncio.Task[None] | None = None
        #: The profile listing's own fetch task, for the same reason.
        self._profiles_task: asyncio.Task[None] | None = None
        #: The one-shot live startup sequence: compatibility check, then KTD7's
        #: session open. Held in its own attribute for the same reason the
        #: catalogue fetch is — it is not an operator's call, so
        #: :meth:`settle_live` must not have to outlast it.
        self._startup_task: asyncio.Task[None] | None = None
        #: Set once the startup sequence has run to completion. A reconnect
        #: re-fetches the catalogue but must not re-open the session: the
        #: gateway keeps the session across a dropped socket, and a second
        #: ``session.create`` on reconnect would silently abandon the operator's
        #: conversation for a fresh one.
        self._startup_done = False
        #: In-flight live calls started from a key binding. Held so teardown can
        #: cancel them and so a test can await them without sleeping.
        self._live_tasks: set[asyncio.Task[None]] = set()
        #: Serializes :meth:`render_snapshot` against itself — see its docstring.
        self._render_lock = asyncio.Lock()
        #: Highest connection epoch already announced by :meth:`note_reconnect`.
        #: 0 means none: the first attach opens epoch 1 and is not a reconnect.
        self._last_reconnect_epoch = 0
        #: This app's own count of successful ``connected`` transitions,
        #: bumped once per transition in :meth:`note_connection_state`. Mirrors
        #: ``RpcCorrelator.epoch`` 1-for-1 — both increment exactly once per
        #: successful dial — without requiring :attr:`dispatcher`, typed only as
        #: :class:`LiveDispatcher`, to expose one. KTD4's staleness check
        #: (:meth:`select_model`) reads this rather than the correlator's own.
        self._connection_epoch = 0
        #: The :attr:`_connection_epoch` value at the moment
        #: :attr:`model_catalog` was last read successfully. 0 means "never
        #: fetched, or the fetch that ran failed" — indistinguishable from each
        #: other here on purpose, since both refuse a selection the same way.
        self._model_catalog_epoch = 0
        #: The same stamp for :attr:`profiles`.
        self._profiles_epoch = 0
        #: True for the duration of one ``session.create``/``session.resume``
        #: round trip (C1, U7 round three; supersedes the generation counter
        #: P1/U7 round two used here previously).
        #:
        #: Choosing B, reopening ``/sessions`` before B's ``session.resume``
        #: reply lands, and choosing C used to dispatch two resumes with no
        #: ordering guarantee on the gateway's side either — a client-side
        #: generation number could discard C's reply if it arrived first,
        #: but the gateway's own active session and Talaria's belief could
        #: still diverge (B live gateway-side, discarded client-side)
        #: because nothing had stopped the *second send*. This stops the
        #: send instead: :meth:`open_session` checks the flag before
        #: dispatching and refuses a newer selection outright (the same
        #: notice :func:`~talaria.domain.state.switch_refusal` already
        #: uses) rather than queuing it — a queued switch would still land
        #: eventually, silently, for a row the operator may not even
        #: remember choosing. With at most one resume ever in flight, a
        #: reply is never stale by the time it lands, which is what let the
        #: generation counter retire rather than needing to compose with
        #: this.
        self._resume_in_flight = False
        #: Request ids whose respond is already in flight. The render tick fires
        #: every 50ms and a terminal-read answer is dispatched from it, so
        #: without this the same read is answered once per tick until the reply
        #: lands — several identical answers to one blocking question.
        self._answering: set[str] = set()
        #: B1's latch: which no-text region last announced a discard. While the
        #: caret stays in that region, further printable keys or pastes are silent
        #: (KTD2). Cleared whenever the caret leaves the announced region (KTD4).
        self._discard_latch: str = ""
        #: A4's latch for the agents-toggle empty notice (AE12): which region was
        #: announced. Shares the same clear point as ``_discard_latch`` so a latched
        #: discard notice is not overwritten and a second toggle in the same hold
        #: is silent.
        self._agents_toggle_latch: str = ""

        # ── gate counters ────────────────────────────────────────────────
        #: Coalescing flushes that actually re-rendered. KTD14 measures render
        #: ticks here rather than by sampling the screen, because the flush
        #: callback is the one place a render can originate.
        self.render_ticks = 0
        #: Frames folded into domain state. Compared against the corpus size to
        #: prove nothing was skipped.
        self.frames_applied = 0
        #: Set once the source is exhausted, so a gate run knows when to stop
        #: without polling the source's internals.
        self.replay_complete = asyncio.Event()
        self._started_at = 0.0
        #: Why the frame stream stopped, when it stopped by failing. Empty on an
        #: ordinary end-of-corpus.
        self.stream_failure = ""
        #: Which background task died and why, when one did. Kept separate from
        #: :attr:`stream_failure` because a dead startup sequence and a dead
        #: frame stream are different incidents with the same exit code.
        self.background_failure = ""

    # ── the fleet root, and the focused cursor into it ───────────────────

    @property
    def fleet(self) -> FleetState:
        """The whole fleet: registry rows, per-connection channels, the focused
        engine, and the in-flight answers that protect rows (v0.4 KTD3)."""
        return self._fleet

    @fleet.setter
    def fleet(self, value: FleetState) -> None:
        self._fleet = value

    @property
    def seams(self) -> SeamBoard:
        """The **focused connection's** seam board, out of the fleet's per-connection map.

        A property rather than a field since U6, and the change is the answer to
        the question U5 deferred. U5 kept one board scoped to the focused
        connection and left "should it be per-connection" to the needs-you
        queue — the first consumer of fleet-wide state — on the reasoning that
        building the shape before its first consumer means inheriting a parallel
        structure that then has to be reconciled.

        The queue answered it: **per-connection**. It has to name, per
        connection, what that connection could not be asked (R24), and the
        connection whose probe failed is precisely the one whose silence must not
        read as quiet — so a single board could only ever describe one of them.
        The board *type* was already per-connection (it carries a ``profile``);
        what was single was this attribute, and it now reads and writes the
        fleet's entry for whichever connection is focused, so the app keeps its
        one-board view of a fleet that holds one board each.
        """
        return fleet_seam_board(self._fleet, self.fleet_profile) or empty_board(
            self.fleet_profile
        )

    @seams.setter
    def seams(self, value: SeamBoard) -> None:
        self._fleet = record_seam_board(
            self._fleet, profile=self.fleet_profile, board=value
        )

    @property
    def state(self) -> SessionState:
        """The focused session's state — :attr:`FleetState.focused`, exactly.

        A property rather than a field, and that is the whole of how the fleet
        root reached this class without a rewrite: every existing read and every
        existing ``self.state = …`` assignment goes on meaning what it meant,
        and the fleet stays consistent with the focused engine by construction
        rather than by a synchronisation step somebody has to remember. A second
        copy of the focused state living beside the fleet is exactly the drift
        :meth:`~talaria.domain.state.FleetState.focused_key` would then resolve
        against the wrong value.

        What U4 deliberately left standing here, and U6 has now discharged:
        inbound frames used to be folded by
        :func:`~talaria.domain.state.apply_frame`, so a background session's
        event took the shipped cross-talk discard and no registry row ever saw
        it. :meth:`ingest` now routes every frame through
        :func:`~talaria.domain.state.route_frame` instead. U4 declined the move
        because it changes what ``cross_session_events_ignored`` counts and that
        was not a focus-movement unit's business; it is this unit's, because the
        queue is derived from the rows the router writes and a queue fed by a
        discard is empty for the wrong reason (R14).
        """
        return self._fleet.focused

    @state.setter
    def state(self, value: SessionState) -> None:
        self._fleet = replace(self._fleet, focused=value)

    @property
    def fleet_profile(self) -> str:
        """Which connection the registry rows this app shows belong to.

        The registry key's first half. It follows the profile this session is
        connected to (:meth:`_adopt_profile`), because the key exists precisely
        to keep two gateways' sessions apart: a stored id seen on two
        connections is two rows, and one name for both would merge them into
        one wrong fleet. Rows of a profile the operator has switched away from
        stay in the registry under their own key — unreachable from the picker,
        which shows this profile's rows, and bounded by the same per-connection
        cap every other profile's rows are.
        """
        return self._fleet.focused_profile

    def _adopt_profile(self, name: str) -> None:
        """Re-point the fleet at the profile this session just connected to.

        Called from the two places ``current_profile`` is assigned, and from
        nowhere else — a property that quietly re-keyed the registry as a side
        effect of being read would be the worst possible shape for this.
        """
        self.current_profile = name
        if name and name != self._fleet.focused_profile:
            self._fleet = replace(self._fleet, focused_profile=name)

    # ── layout ───────────────────────────────────────────────────────────

    def _status_agent(self) -> tuple[str, str]:
        """Return the model facts already held for the focused session."""
        session = self.session_model_in_focus
        catalog = self.model_catalog
        if session is not None:
            provider_slug = session.provider_slug
            model = session.model
        else:
            session_id = self.state.session_key or self.state.focused_session_id or ""
            row = (
                fleet_row(
                    self.fleet,
                    profile=self.fleet_profile,
                    session_id=session_id,
                )
                if session_id
                else None
            )
            if row is not None and row.model:
                # The roster row is an observation of this session's actual
                # model; the catalogue describes configured defaults instead.
                provider_slug = (
                    catalog.current_provider
                    if catalog is not None and catalog.current_model == row.model
                    else ""
                )
                model = row.model
            elif catalog is not None:
                provider_slug = catalog.current_provider
                model = catalog.current_model
            else:
                return "", ""

        provider = provider_slug
        if catalog is not None:
            provider = next(
                (
                    candidate.name
                    for candidate in catalog.providers
                    if candidate.slug == provider_slug
                ),
                provider_slug,
            )
        return provider, model

    def _bottom_status_view(self) -> BottomStatusBarView:
        """Assemble the bar from state Talaria already holds, without fetching."""
        provider, model = self._status_agent()
        return build_status_bar_view(
            local=self.local_status,
            status=status_payload(self.state, mode=self.mode),
            queue=self.needs_you,
            agent_provider=provider,
            agent_model=model,
            context_window=None,
            version=__version__,
        )

    def _inspector_model(self) -> str:
        """Return the most specific focused-session model Talaria already holds."""
        provider, model = self._status_agent()
        if self.session_model_in_focus is not None:
            return "/".join(part for part in (provider, model) if part)

        session_id = self.state.session_key or self.state.focused_session_id or ""
        if session_id:
            row = fleet_row(
                self.fleet,
                profile=self.fleet_profile,
                session_id=session_id,
            )
            if row is not None and row.model:
                return row.model
        return "/".join(part for part in (provider, model) if part)

    def compose(self) -> ComposeResult:
        with Horizontal(id="main-and-inspector"):
            with Vertical(id="body"):
                yield TranscriptPane(
                    mount_cap=self.mount_cap,
                    motion=self.motion,
                    id="transcript",
                )
                yield AgentRows(id="agents")
                yield PromptRegion(motion=self.motion, id="prompts")
                yield PaletteRegion(id="palette")
                yield StatusRegion(
                    initial_marker="\n".join(self._status_notices),
                    id="status",
                )
            yield Inspector(id="inspector")
        yield Composer(
            notice=self._idle_notice(),
            paste_threshold=self.paste_threshold,
            id="composer",
        )
        yield HelpBar(id="help")
        yield BottomStatusBar(
            self._bottom_status_view(),
            settings=self.status_bar_settings,
            id="bottom-status",
        )

    @property
    def transcript(self) -> TranscriptPane:
        return self.query_one("#transcript", TranscriptPane)

    @property
    def agents(self) -> AgentRows:
        return self.query_one("#agents", AgentRows)

    @property
    def prompts(self) -> PromptRegion:
        return self.query_one("#prompts", PromptRegion)

    @property
    def palette(self) -> PaletteRegion:
        return self.query_one("#palette", PaletteRegion)

    @property
    def status_region(self) -> StatusRegion:
        return self.query_one("#status", StatusRegion)

    @property
    def composer(self) -> Composer:
        return self.query_one("#composer", Composer)

    @property
    def help_bar(self) -> HelpBar:
        return self.query_one("#help", HelpBar)

    @property
    def bottom_status_bar(self) -> BottomStatusBar:
        return self.query_one("#bottom-status", BottomStatusBar)

    @property
    def inspector(self) -> Inspector:
        return self.query_one("#inspector", Inspector)

    @property
    def needs_you(self) -> NeedsYouQueue:
        """The queue as the fleet stands, derived on every read (U6, R13).

        A property rather than a field, and the reason is U6's rather than this
        unit's: a stored queue kept in step by hand is a second copy of the truth
        that some path eventually forgets to update, which on this surface means
        either a phantom task or a silent one.
        """
        return fleet_queue(self._fleet)

    def _idle_notice(self) -> str:
        return INERT_NOTICE if self.mode == "replay" else ""

    # ── U6: layout-stable focus and chrome changes ──────────────────────

    def _capture_layout_anchor(self) -> TranscriptAnchor | None:
        """Capture an unpinned reading position before chrome can reflow it."""
        try:
            transcript = self.transcript
        except NoMatches:
            return None
        if transcript.follow:
            return None
        return transcript.capture_reading_anchor()

    def _restore_layout_anchor(self, anchor: TranscriptAnchor | None) -> None:
        """Restore one captured position, or the exact bottom when following."""
        try:
            transcript = self.transcript
        except NoMatches:
            return
        if transcript.follow or anchor is not None:
            transcript.restore_reading_anchor(anchor)

    def _sync_focus_indication(self) -> None:
        """Repaint both caret cues without mounting or resizing anything."""
        region = focused_region(self.focused)
        try:
            self.status_region.set_caret(region)
            self.composer.show_caret_location(region == "composer")
        except NoMatches:
            # The first focus event may arrive while compose is still mounting.
            return

    def _restore_after_theme_change(self, _theme: object) -> None:
        """Keep the open-time reading anchor through every picker preview."""
        self._restore_layout_anchor(self._theme_preview_anchor)

    def _clear_theme_preview_anchor(self) -> None:
        self._theme_preview_anchor = None

    # ── lifecycle ────────────────────────────────────────────────────────

    async def on_mount(self) -> None:
        self._started_at = time.monotonic()
        self.theme_changed_signal.subscribe(self, self._restore_after_theme_change)
        self._coalesce_timer = self.set_interval(self.coalesce_interval, self._render_tick)
        # A separate, far slower timer than the render tick, and deliberately
        # not folded into it: R12 wants the probe story kept live, and a probe
        # round on the 50ms render cadence would put eight RPCs and an HTTP
        # request on the wire twenty times a second. The callback re-checks
        # ``seam_probe_due`` before doing anything, so the interval is a floor
        # rather than the only thing standing between here and a busy loop.
        self._seam_timer = self.set_interval(PROBE_REVALIDATION_S, self.revalidate_seams)
        # KTD2's cadence. A third timer rather than a branch inside either of the
        # two above, because it answers a different question on a different
        # period: the render tick is 50ms and the seam re-probe is five minutes,
        # while a poll that must coalesce within two seconds of a hint sits
        # between them. The callback re-checks ``next_poll_due_at`` per
        # connection, so this interval is a floor.
        self._poll_timer = self.set_interval(POLL_TICK_S, self.poll_due_connections)
        # **A recording's connections were up, and the fleet must start saying
        # so.** ``ConnectionChannel.connected`` defaults False, so without this a
        # replayed two-connection log reports "connection down before it was ever
        # polled" for every connection — about gateways that demonstrably
        # answered, since the log exists. Measured before the fix: both
        # connections of a two-connection replay said exactly that.
        #
        # This states a recorded fact rather than assuming one. A connection that
        # really did drop mid-recording is marked down again by the log's own
        # terminal cause when replay reaches it, so nothing is lost by starting
        # from the truth. Pinned by
        # ``test_a_replayed_fleet_does_not_report_its_own_recording_as_down``.
        source = self.source
        # **Mode-gated as well as shape-gated, and the mode is the honest half.**
        # This fold stamps every declared connection with ``REPLAY_EPOCH``, which
        # is meaningful only for a recording. It is skipped in a live run today
        # because ``ConnectionSet`` — which IS the source in live mode — happens
        # not to expose a ``connections`` property, so the ``isinstance`` below
        # answers False. That is correctness resting on the absence of a name:
        # ``ConnectionSet`` already has ``profiles``, and anyone adding
        # ``connections`` beside it as a convenience would silently start
        # stamping a live fleet with a replay epoch at mount. Found by the v0.4
        # accumulated-code review; no harm demonstrated, a trap removed.
        if self.mode == "replay" and isinstance(source, DeclaredConnections):
            for profile in source.connections:
                self._fleet = fleet_connection_restored(
                    self._fleet,
                    profile=profile,
                    generation=REPLAY_EPOCH,
                    at=self.state.last_observed_at,
                )
        self._refresh_bottom_status_bar()
        self._pump_task = asyncio.create_task(self._pump())
        if isinstance(self.connections, ConnectionFleet):
            # Dialled here rather than in the launcher because the set's dials are
            # concurrent coroutines and the launcher is synchronous — and because
            # the pump above must already be draining the merged stream when the
            # first connection lands, or the earliest frames would queue behind a
            # consumer that had not started. That ``start`` never raises — every
            # dial's outcome comes back as a report, so one unreachable gateway
            # cannot take the launch down with it — is ``ConnectionSet.start``'s
            # own stated contract and U2's tests', not a claim made here.
            self._fleet_task = asyncio.create_task(self.connections.start())
        if self.status_runner is not None and self.status_runner.enabled:
            self._status_task = asyncio.create_task(self._status_loop())
        self.fetch_catalog()
        try:
            self.help_bar.update_for_mode(self.mode)
        except NoMatches:
            pass
        self.composer.text_area.focus()
        self.call_after_refresh(self._sync_focus_indication)

    def on_resize(self, event: events.Resize) -> None:
        """Pass terminal width to the inspector without losing the reader."""
        anchor = self._capture_layout_anchor()
        try:
            self.inspector.set_terminal_width(event.size.width)
        except NoMatches:
            # Textual can publish the app's first resize before compose mounts.
            pass
        self._restore_layout_anchor(anchor)

    async def on_unmount(self) -> None:
        await self.shutdown_sources()

    async def shutdown_sources(self) -> None:
        """Stop everything Talaria started. Idempotent (R36).

        The coalescing timer is stopped *first*. Textual keeps servicing timers
        while a screen tears down, and a tick that fires after the widgets are
        gone raises ``NoMatches`` from inside the framework — a teardown-order
        bug that surfaces as a flaky, unrelated-looking test failure.

        Note the attribute names. ``_teardown_started`` is deliberately not
        called ``_closing``: ``textual.message_pump.MessagePump`` already owns an
        instance attribute by that name, and assigning it here convinces the
        framework its own shutdown is already in progress, after which the app
        never finishes closing. See ``tests/ui/test_app_shadowing.py``, which
        fails the build if any attribute or method on this class shadows one of
        Textual's.
        """
        self._teardown_started = True
        if self._coalesce_timer is not None:
            self._coalesce_timer.stop()
            self._coalesce_timer = None
        if self._seam_timer is not None:
            self._seam_timer.stop()
            self._seam_timer = None
        if self._poll_timer is not None:
            self._poll_timer.stop()
            self._poll_timer = None
        for task in (
            self._pump_task,
            self._status_task,
            self._catalog_task,
            self._startup_task,
            # The fleet's opening dial. Cancelled with the rest rather than
            # awaited: a gateway that is not answering would otherwise hold
            # teardown open for its whole connect timeout, which is the R36
            # failure of an exit that cannot be reached from a hung transport.
            self._fleet_task,
            *self._live_tasks,
        ):
            if task is not None and not task.done():
                task.cancel()
        self._live_tasks.clear()
        if self.status_runner is not None:
            # What this call is, stated accurately, because an earlier comment
            # here claimed more than it does and the next reader would have
            # trusted it. Cancelling ``_status_task`` **is** enough for the tick
            # that task owns: cancellation unwinds into ``_run_once``'s
            # ``finally``, which sweeps the child's process group whatever the
            # leader's state. Removing this line and re-running the pty
            # teardown tests three times left no status child behind, so it is
            # not the only thing standing between R36 and a leaked process.
            #
            # It is kept for the three things cancellation does not cover. It
            # sweeps without waiting for the cancelled task to get another turn
            # (it yields only when a spawn is still in flight, which is the
            # third case below). It closes the window in which a child has been
            # forked but not yet recorded by the runner, where cancellation
            # sweeps nothing because the group is not known yet — an R36 leak
            # CI caught after this line was written; see
            # ``StatusRunner.aclose``. And it covers a tick this app does not own —
            # anything that called ``status_runner.tick()`` from a task other
            # than ``_status_task`` — which is what
            # ``test_teardown_stops_a_status_child_this_app_does_not_own``
            # exercises, and which cancelling ``_status_task`` cannot reach.
            await self.status_runner.aclose()
        await self.source.close()

    # ── the frame pump ───────────────────────────────────────────────────

    async def _pump(self) -> None:
        """Fold frames until the source ends, and stop the app if it fails.

        **A source that raises has to bring the app down.** Without the clause
        below, the exception left this task dead and unretrieved while the
        interface stayed up: the transcript froze at the last frame, every
        control still worked, and nothing on screen said the stream had ended.
        An operator has no way to tell that from a quiet session, which makes it
        strictly worse than a crash — they keep typing into a client that can no
        longer hear anything. R36 wants teardown to be reachable from an induced
        failure, and it cannot be if the failure is swallowed by the task that
        suffered it.

        ``CancelledError`` is re-raised, not reported: teardown itself cancels
        this task, and reporting that as a transport failure would put a failure
        line on screen during every orderly exit.
        """
        try:
            async for item in self.source:
                if isinstance(item, TaggedFrame):
                    # The fleet shape. Each frame carries the connection it
                    # crossed and that connection's epoch at the moment it was
                    # taken off the queue, so the router keys the right registry
                    # rows and can tell a frame from the socket Talaria is
                    # talking to now from one that arrived on a socket since
                    # replaced. Deriving either from app state instead would
                    # attribute every connection's traffic to the focused one.
                    self.ingest(item.record, profile=item.profile, epoch=item.epoch)
                else:
                    # The single-connection shape: one ``LiveSource``, or a
                    # VERSION-1 replay — both yielding bare records.
                    #
                    # **This used to say "replay has no connection set and never
                    # will — a recording is one stream by construction".** True
                    # of the v0.3 format, false from U2 onward, and reached by
                    # the branch above from U8: a version-2 recording opens as
                    # ``TaggedReplaySource`` and takes the tagged path. The
                    # surviving half is that a version-1 log genuinely is one
                    # stream and no tag is invented for it (KTD6).
                    self.ingest(item)
        except asyncio.CancelledError:  # pragma: no cover - teardown path
            raise
        except Exception as exc:  # noqa: BLE001 - reported, then the app stops
            self._fail_stream(exc)
        finally:
            await self.source.close()
            self.replay_complete.set()

    def _supervise(self, task: asyncio.Task[None], label: str) -> asyncio.Task[None]:
        """Make a fire-and-forget task's failure visible instead of silent.

        Every background task this class starts is created with
        ``asyncio.create_task`` and awaited by nobody outside teardown, so an
        exception inside one lands in a future nothing retrieves. asyncio prints
        "Task exception was never retrieved" to stderr — under a full-screen
        Textual app that goes nowhere a person will look — and the interface
        stays up. :meth:`_pump` had exactly this defect and it was fixed there;
        the startup sequence and the catalogue fetch had it too, one function
        away, and this is the shared fix rather than a third copy.

        **Why it exits rather than only reporting.** The two supervised tasks
        are the ones that make a live client usable: without the startup
        sequence the compatibility check never ran and no session was ever
        opened, so the operator is looking at a connected client attached to
        nothing. Neither coroutine is *supposed* to be able to raise — every
        dispatcher call below them returns an
        :class:`~talaria.transport.rpc.RpcOutcome` on every exit rather than
        raising — so an exception here is a defect, and a defect that leaves the
        interface looking healthy is the worst shape it can take.
        """

        def _done(finished: asyncio.Task[None]) -> None:
            if finished.cancelled():
                return
            exc = finished.exception()
            if exc is not None:
                self._fail_background(label, exc)

        task.add_done_callback(_done)
        return task

    def _fail_background(self, label: str, exc: BaseException) -> None:
        """Name a dead background task and bring the app down (R36)."""
        detail = scrub_urls(str(exc)) or type(exc).__name__
        self.background_failure = f"{label}: {detail}"
        self.state = record_local_note(
            self.state,
            f"{BACKGROUND_FAILED} {self.background_failure}",
            at=self.state.last_observed_at,
        )
        self._dirty = True
        self.exit(return_code=STREAM_FAILURE_EXIT_CODE)

    def _fail_stream(self, exc: BaseException) -> None:
        """Name a frame-source failure and ask Textual to shut down.

        The detail goes through :func:`~talaria.transport.attach.scrub_urls` for
        the same reason every other operator-facing exception string does: the
        one string the dialler is handed is the credentialed URL, and an
        exception that quotes it back would put a live token on screen.
        """
        detail = scrub_urls(str(exc)) or type(exc).__name__
        self.stream_failure = detail
        self.state = record_local_note(
            self.state, f"{STREAM_FAILED} {detail}", at=self.state.last_observed_at
        )
        self._dirty = True
        self.exit(return_code=STREAM_FAILURE_EXIT_CODE)

    def ingest(
        self, record: FrameRecord, *, profile: str | None = None, epoch: int | None = None
    ) -> None:
        """Fold one frame into domain state. Pure except for the dirty flag.

        ``profile`` and ``epoch`` are the connection tag the frame arrived under,
        supplied by the fleet pump from :class:`~talaria.transport.connection_set.TaggedFrame`.
        They default to the focused connection and this app's own epoch, which is
        the single-connection shape every existing caller and every replay uses.

        Outbound frames are not folded through the reducer: a recording of what
        Talaria itself sent is not a description of what the session became, and
        replaying a request as if it were an event would double-apply the
        operator's turn.

        **One outbound frame is read anyway, and only in replay mode.** The
        gateway never echoes a submitted prompt back, so the operator's own line
        is written locally by :func:`~talaria.domain.state.record_submission` —
        which happens on submit, and never happens in a replay. A replay of a
        real session therefore rebuilt the agent's half of a conversation and
        not the question it answered, which makes R3's own evidence method (one
        live turn, compared against a replay of the same frames) impossible to
        complete. The text is in the recording, in the outbound ``prompt.submit``
        this recovers it from.

        Live mode must not take this branch, and that is the whole reason for
        the mode test: there the local write has already happened, so folding
        the frame too would print the operator's message twice.
        """
        if self._landing_depth and record.direction == "in":
            # KTD2's barrier. The transport resolves the RPC future
            # (``transport/source.py:589``) and enqueues later frames
            # independently (``:601``) while this pump runs concurrently, so an
            # event the gateway sent *after* the resume reply can reach the
            # reducer before the awaiting coroutine wakes up and seeds. Held
            # here and flushed by :meth:`_landing` the moment the seed is
            # applied, which puts the event after the history it follows —
            # which is where the gateway put it.
            self._deferred_frames.append((record, profile, epoch))
            return
        self.frames_applied += 1
        if record.direction == "out":
            if self.mode == "replay":
                text = replayed_submission_text(record.frame)
                if text is not None:
                    self.state = record_replayed_submission(self.state, text, at=record.at)
                    self._dirty = True
            return
        decoded = normalize_frame(
            record.frame,
            at=record.at,
            seq=record.seq,
            parse_error=record.parse_error,
        )
        # Routed rather than folded: ``apply_frame`` only ever sees the focused
        # engine, so a background session's event was discarded and no registry
        # row learned anything from it — which is how U6's queue could be empty
        # because Talaria never recorded the question rather than because none
        # was asked. ``route_frame`` feeds focused traffic to that same engine
        # unchanged and sends everything else to its row.
        #
        # ``generation`` is this app's CURRENT connection epoch, and it is
        # LOAD-BEARING — passing a constant here silently breaks a registry
        # row's liveness recovery. ``route_frame`` computes
        # ``stale_generation = generation < channel.generation``, and
        # ``stale_generation`` is what WITHHOLDS ``_fresh_observation``, the call
        # that clears a row's ``disconnected`` and ``stale_since``. Because
        # ``fleet_connection_restored`` raises the channel's generation to the
        # epoch on every reconnect, passing the epoch is exactly what keeps that
        # comparison false so a row recovers on its next event; passing ``0``
        # makes it true from the first connect of the run and no inbound event
        # ever ends any row's staleness again. Pinned by
        # ``test_a_reconnected_row_recovers_liveness_from_its_next_event``, which
        # fails under precisely that substitution.
        #
        # Three earlier drafts of this comment were wrong, the last one badly,
        # and the correction is kept because the mistake is instructive. It said
        # the argument was inert, on the evidence that replacing it with ``0``
        # left the whole suite green. The suite WAS green — that measurement was
        # real — but a green suite means the tests do not distinguish the
        # change, never that behaviour does not differ. It was a coverage gap
        # read as an equivalence, and the test above is the probe that was
        # missing. See the corrected rule in
        # ``docs/engineering-journal/LEARNINGS.md``.
        #
        # **That last paragraph described a gap that is now closed, and the
        # correction is dated because the shape of the mistake matters.** It read
        # "``TaggedFrame`` reaches no consumer until U7 assembles the
        # ``ConnectionSet``", which was true when written and false from
        # ``ecb6b9d`` (2026-08-18). The set is assembled in ``build_live_app``,
        # the pump above branches on ``TaggedFrame``, and ``epoch`` is that
        # frame's own ``arrival_epoch`` off its own socket — so the ``>`` guard
        # DOES fire, and a frame that outlived its connection is genuinely
        # distinguishable from one on the current socket.
        #
        # ``epoch is None`` is now the narrower case rather than the usual one:
        # a direct ``ingest`` from replay or from a test, where there is one
        # stream and the app's own focused counter is the only generation there
        # is. Which counter belongs where is not interchangeable — see
        # :meth:`_socket_generation`.
        self._fleet = route_frame(
            self._fleet,
            decoded,
            profile=profile or self.fleet_profile,
            generation=self._connection_epoch if epoch is None else epoch,
        )
        self._dirty = True

    # ── the coalescing render tick (KTD14) ───────────────────────────────

    async def _render_tick(self) -> None:
        if self._teardown_started:
            return
        self._age_out_approvals()
        # Before the dirty check, and for the same reason ageing out an approval
        # is: a seam line growing older is a change with no event behind it, so
        # nothing else marks the app dirty when it becomes due.
        await self._refresh_seam_ages()
        self._refresh_bottom_status_bar()
        if not self._dirty:
            return
        self._dirty = False
        await self.render_snapshot()

    def _refresh_bottom_status_bar(self) -> None:
        """Repaint the true-bottom row from facts already held by the app."""
        try:
            bar = self.bottom_status_bar
        except NoMatches:  # pragma: no cover - teardown race
            return
        bar.apply(self._bottom_status_view())

    def _age_out_approvals(self) -> None:
        """Withdraw an approval the gateway has almost certainly stopped holding.

        **Run before the dirty check, not after it.** Ageing something out is
        the one state change with no event behind it, so nothing else marks the
        app dirty when it becomes due — placed after the early return it would
        have fired only when some unrelated frame happened to arrive, which for
        a session blocked on a stale approval is precisely never.

        The clock is the one the prompt's own ``opened_at`` came from. Live
        frames are stamped by ``LiveSource`` with the wall clock; a replayed
        frame carries the time it was recorded at, and reading a wall clock
        there would age out an entire corpus on the first tick and break AE2's
        "replay it twice, get the same state".
        """
        now = time.time() if self.mode == "live" else self.state.last_observed_at
        next_state = age_out_approvals(self.state, now=now)
        if next_state is self.state:
            return
        self.state = next_state
        self._dirty = True

    async def render_snapshot(self) -> None:
        """Project once, then update only the regions the projection says moved.

        **Serialized against itself.** The coalescing timer is not the only
        caller: :meth:`drain` and the gate's forced checkpoints call this
        directly, from a task that is *not* the message pump, so two renders can
        interleave. ``TranscriptPane.apply`` is a read-modify-write over its own
        window bookkeeping across several awaits, and two concurrent passes leave
        the pane holding a window the projection does not have — observed as a
        one-line skew (`'line 38.3' != 'line 38.4'`) in
        ``tests/ui/test_transcript_bounds.py``, intermittently and only under
        whole-suite load. Textual's own timer never re-enters its callback, so
        this lock is uncontended in the ordinary path and costs nothing there.
        """
        async with self._render_lock:
            await self._render_snapshot_locked()

    async def _render_snapshot_locked(self) -> None:
        # Counted here, where a render actually happens, rather than in
        # _render_tick. _render_tick is the callback of a set_interval timer, so
        # a count taken there is bounded by the timer frequency (20/s at a 50ms
        # interval) and can never breach the gate's 25/s ceiling however the
        # renderer behaves. Defeating coalescing entirely — scheduling a render
        # per inbound frame — drove real renders to one per frame while the
        # reported rate went *down*. The point of this metric is to notice
        # exactly that, so it counts renders, not timer firings.
        self.render_ticks += 1
        previous = self.snapshot
        snapshot = project(self.state, mode=self.mode, previous=previous)
        self.snapshot = snapshot
        entries = entry_scoped_view(self.state)

        if "transcript" in snapshot.changed:
            # KTD6: the pane needs entry identity and raw (unwelded) bodies
            # that TranscriptView's flattened line buffer does not carry, so
            # U4 computes the entry-scoped surface here rather than growing
            # Snapshot's frozen shape — entry_scoped_view is a pure function
            # of the same SessionState project() already read this tick.
            await self.transcript.apply(snapshot.transcript, entries)
        if "subagents" in snapshot.changed:
            await self.agents.apply(snapshot.subagents)
        if {"prompts", "status"} & snapshot.changed:
            # Both regions, because the activity line is a function of the
            # prompts *and* of the derived turn status. Watching only "prompts"
            # leaves "working…" on screen after a turn ends with a prompt still
            # outstanding, which is the one sentence R8 forbids.
            await self.prompts.apply(
                snapshot.prompts,
                snapshot.status.turn,
                focus_new=not self.composer.text.strip(),
            )
        profile = self.current_profile
        inspector_projection = inspector_view(
            entries,
            queue=self.needs_you,
            agents=snapshot.subagents,
            session_id=self.state.focused_session_id or "",
            profile=profile,
            endpoint=self.profile_endpoints.get(profile, "") if profile else "",
            model=self._inspector_model(),
            usage=self.state.usage,
        )
        if inspector_projection != self._inspector_view:
            self._inspector_view = inspector_projection
            await self.inspector.apply(inspector_projection)
        self._answer_unattended_prompts(snapshot)
        self._refresh_bottom_status_bar()

    # ── the status region (U6) ───────────────────────────────────────────

    async def _status_loop(self) -> None:
        runner = self.status_runner
        if runner is None:  # pragma: no cover - guarded by the caller
            return
        while True:
            await self.status_tick()
            await asyncio.sleep(self.status_interval)

    async def status_tick(self) -> StatusTickResult | None:
        """Run one status tick and render its rows. Returns the result for tests."""
        runner = self.status_runner
        if runner is None or not runner.enabled:
            return None
        if self.snapshot is None:
            self.snapshot = project(self.state, mode=self.mode)
        result = await runner.tick(self.snapshot.status)
        anchor = self._capture_layout_anchor()
        try:
            await self.status_region.apply(result)
        finally:
            self._restore_layout_anchor(anchor)
        return result

    # ── replay controls (R40, AE11) ──────────────────────────────────────

    def _pacing_notice(self) -> str:
        """The one sentence the pacing state is reported with.

        Written once because there are now two ways to reach every pacing
        control — the function keys and U9's ``/pause``, ``/resume``,
        ``/speed`` — and two renderings of one fact drift. U8's whole
        :class:`AnswerVerdict` exists because that happened to a safety claim.
        """
        return f"{self._idle_notice()} · {self.controls.label}".strip(" ·")

    def _pacing_refused_live(self, name: str) -> bool:
        """Refuse a pacing control in a live session, out loud. True if refused.

        The controls scale a *recorded* clock: ``ReplaySource`` is the only
        thing that reads ``ReplayControls``, and a live session is fed by
        ``LiveSource``, which does not. Before this, F8 in a live session
        flipped a flag nobody read and reported "paused" — a control that looks
        like it worked and did nothing, which is the failure AE11 makes visible
        for the replay direction and had never been checked in this one.
        """
        if self.mode == "replay":
            return False
        self._notice(f"{name} {LIVE_HAS_NO_REPLAY_CLOCK}")
        return True

    def action_toggle_pause(self) -> None:
        if self._pacing_refused_live("/pause"):
            return
        self.controls.toggle_pause()
        self._notice(self._pacing_notice())

    def action_speed_up(self) -> None:
        if self._pacing_refused_live("/speed"):
            return
        self.controls.speed_up()
        self._notice(self._pacing_notice())

    def action_slow_down(self) -> None:
        if self._pacing_refused_live("/speed"):
            return
        self.controls.slow_down()
        self._notice(self._pacing_notice())

    def action_toggle_inspector(self) -> None:
        """Toggle the process-local dock or narrow overlay."""
        anchor = self._capture_layout_anchor()
        self.inspector.toggle()
        self._restore_layout_anchor(anchor)

    def on_inspector_file_selected(self, message: Inspector.FileSelected) -> None:
        """Open the selected held file without reading or dispatching anything."""
        message.stop()
        self._open_diffs(message.selection)

    def _open_diffs(self, selection: DiffSelection | None = None) -> None:
        anchor = self._capture_layout_anchor()
        inspector = self.inspector
        inspector.set_diff_open(inspector.is_docked)
        self._restore_layout_anchor(anchor)
        self.push_screen(
            DiffViewer(
                adapt_diff_document(inspector.document),
                file_key=None if selection is None else selection.file_key,
                hunk_index=0 if selection is None else selection.hunk_index,
                motion=self.motion,
            ),
            self._diff_closed,
        )

    def _diff_closed(self, _: None) -> None:
        """Restore the inspector state that the diff modal left untouched."""
        anchor = self._capture_layout_anchor()
        self.inspector.set_diff_open(False)
        self._restore_layout_anchor(anchor)

    async def action_toggle_agents(self) -> None:
        if not self.agents.is_populated:
            # A4 AE12: the empty-toggle notice shares the per-focus-hold latch
            # that B1 introduced. While a discard notice is latched, a toggle
            # that would otherwise overwrite it is silent — the toggle still
            # flips (it decides how the next fan-out arrives) but the latched
            # notice remains on the bar. A second empty toggle in the same hold
            # is also silent, so the later notice wins and the earlier is not
            # resurrected.
            if self._discard_latch:
                await self.agents.toggle_collapsed()
                return
            if self._agents_toggle_latch:
                await self.agents.toggle_collapsed()
                return
            # B3: the toggle still flips its flag when empty — it decides how
            # the next fan-out arrives — but the flip is invisible, so the
            # keypress says there was nothing on screen for it to act on.
            self._notice(AGENTS_NOTHING_TO_TOGGLE)
            # Latch to the focused widget's identity, so a second toggle while
            # the caret stays put is silent; moving the caret clears it.
            try:
                focused = self.focused
                self._agents_toggle_latch = str(focused) if focused is not None else "no-focus"
            except Exception:
                self._agents_toggle_latch = "agents"
        await self.agents.toggle_collapsed()

    def action_follow_bottom(self) -> None:
        """Follow the newest transcript line, or say the key arrived when already there.

        Both F5 and the raw ``end`` key reach this one method (KTD2), so the
        fact "already following" cannot be rendered two ways and drift — the
        failure :meth:`_pacing_notice`'s docstring records two renderings of
        the pacing state doing. Re-following at the bottom of a paused replay
        is a legitimate no-op, and silence there is ambiguous (charter E2).
        """
        if self.transcript.follow:
            self._notice(ALREADY_FOLLOWING_BOTTOM)
            return
        self.transcript.follow_bottom()

    def action_interrupt(self) -> None:
        """Stop the in-flight turn (R4) — inert in replay (AE11) and inert when idle (P1-A).

        ``ctrl+c`` reclaims Textual's system quit binding, so the guard is
        load-bearing: the first press when no turn is in flight is a no-op
        that says so, matching :data:`ALREADY_FOLLOWING_BOTTOM`'s visible
        nothing-happened.
        """
        if self.mode == "replay":
            self._refuse_mutation("interrupt")
            return
        if self.state.turn != "streaming":
            self._notice(NOTHING_TO_INTERRUPT)
            return
        self._spawn_live(self.interrupt_live())

    def _refuse_mutation(self, name: str) -> None:
        outcome = self.controls.attempt(name)
        self.composer.show_notice(f"{outcome.notice} — {name} did nothing")

    def _spawn_live(self, coroutine: Any) -> asyncio.Task[None]:
        """Run a live call off the message pump, and remember it for teardown.

        A key binding must not await an RPC inline: the message pump that
        delivered the keypress is the same one that has to keep rendering the
        stream the RPC is about to affect, so blocking it would freeze the
        interface for the duration of the call.
        """
        task: asyncio.Task[None] = asyncio.create_task(coroutine)
        self._live_tasks.add(task)
        task.add_done_callback(self._live_tasks.discard)
        return task

    async def settle_live(self) -> None:
        """Await every in-flight live call. For tests and for orderly teardown."""
        while self._live_tasks:
            await asyncio.gather(*tuple(self._live_tasks), return_exceptions=True)

    # ── the live transport's own state (R35, F6) ─────────────────────────

    def note_connection_state(
        self,
        state: ConnectionStatus,
        detail: str = "",
        cause: TerminalCause | None = None,
        *,
        profile: str | None = None,
    ) -> None:
        """Fold a transport state change into domain state and show it.

        ``profile`` names the connection this is about, and splits the method in
        two along the same seam U6's approval age-out already uses: the FLEET
        half is unconditional, because every connection's rows must learn that
        their stream broke whether or not the operator is looking at them, while
        the PRESENTATION half — the focused session's ``connection`` field and
        the notice line — stays scoped to the connection on screen. Firing the
        presentation half for a background connection would tell the operator
        that *this* session had dropped when another one had. ``None`` means the
        focused connection, which is the single-connection shape.

        Wired to ``LiveSource(on_connection=…)`` via :meth:`LiveSource.bind`
        (``talaria/cli.py``). It is a callback rather than a synthetic frame
        on purpose: a fabricated ``gateway.disconnected`` event would land in
        the recorded corpus as though the gateway had sent it.

        ``detail`` carries the cause for the one distinction the frozen KTD5
        enum cannot express — a gateway that could not be reached versus one
        that hung up — so R35's four states stay four on screen.

        ``cause`` is KTD7's typed end-of-stream cause — ``None`` for a
        transient status change, one of ``auth_failed``/``dial_failed``/
        ``orderly_close``/``reconnect_exhausted`` when the transport is
        telling the domain the stream genuinely will not resume. Passed
        straight through to :func:`~talaria.domain.state.set_connection`,
        which is what actually commits any partial streaming and reasoning
        text as transcript entries before clearing it (R6) — this method
        does no committing of its own. ``at`` is
        ``self.state.last_observed_at`` rather than a fresh clock read,
        matching :meth:`interrupt_live`'s own call into ``cancel_turn``.
        """
        focused = profile is None or profile == self.fleet_profile
        if focused:
            self.state = set_connection(
                self.state, state, cause=cause, at=self.state.last_observed_at
            )
        if state == "disconnected":
            # The fleet half of the same event, and U6's rather than R35's: the
            # focused engine's ``connection`` field describes the session on
            # screen, while every *other* row on this connection has just lost
            # the stream that was naming its waits. Without this the rows keep
            # reporting a kind no surviving stream can clear, and the queue
            # either starves (a suppressed kind) or offers an answer to a
            # question that may already be gone. Pinned by
            # ``test_a_dropped_connection_marks_the_fleet_rows_not_just_the_focused_session``.
            self._fleet = fleet_connection_lost(
                self._fleet,
                profile=profile or self.fleet_profile,
                at=self.state.last_observed_at,
            )
        self._dirty = True
        line = _CONNECTION_NOTICE[state]
        if detail:
            line = f"{line} · {detail}" if line else detail
        # ``_notice`` rather than ``composer.show_notice``: this is a transport
        # callback, and the transport reports ``disconnected`` from inside
        # ``source.close()`` — which :meth:`shutdown_sources` calls *after* the
        # screen has come down. The unguarded query raised ``NoMatches`` at the
        # end of an orderly exit, which is exactly the R36 failure the guard on
        # the prompt path was added for; this path had not inherited it.
        if focused:
            # Presentation, so focus-scoped for the same reason ``set_connection``
            # above is: this line says "the connection dropped" with no subject,
            # and on a fleet client the operator would read it as the session in
            # front of them. A background connection's drop is already visible
            # where it belongs — on its own rows, and in the queue's
            # ``connection_notices`` line naming that profile. Pinned by
            # ``test_a_background_connections_drop_does_not_write_the_focused_notice``,
            # which asserts both halves: the rows learn, the focused view does not.
            self._notice(line)
        if state == "connected":
            named = profile or self.fleet_profile
            if focused:
                # Bumped before either fetch, so a fetch that starts on this
                # transition is stamped with the epoch it actually ran on rather
                # than the previous one — see :attr:`_connection_epoch`. It counts
                # FOCUSED connects only, which is exactly why it is not the number
                # written into the channel below.
                self._connection_epoch += 1
            # The fleet's symmetric half of the disconnect above, and it restores
            # the CHANNEL rather than the rows. Without it ``channel.connected``
            # would stay False for the rest of the run, and that flag is what
            # ``connection_notices`` reads to tell the operator a connection is
            # down (``talaria/domain/queue.py:1350``) — so a reconnected
            # connection would go on claiming it was down, which is precisely
            # the queue-that-lies failure R14 exists to prevent.
            #
            # It deliberately does NOT clear any row's ``disconnected``: a
            # reconnect proves the socket, not the sessions (R20). Each row
            # clears its own break when a fresh poll or event re-confirms it
            # (``_fresh_observation``), which is the flag U6's stale-kind rule
            # reads. Pinned by
            # ``test_a_reconnect_restores_the_channel_and_leaves_the_rows_to_re_confirm``.
            #
            # **Fleet-scoped, unlike the presentation above.** A background
            # connection coming up has to mark its own channel connected too, or
            # ``connection_notices`` reports every connection but the focused one
            # as down for the life of the run — R14's queue-that-lies failure
            # pointed the other way, since a fleet whose gateways are all healthy
            # would say most of itself could not be asked. Pinned by
            # ``test_a_background_connection_coming_up_marks_its_own_channel``.
            self._fleet = fleet_connection_restored(
                self._fleet,
                profile=named,
                generation=self._socket_generation(named),
                at=self.state.last_observed_at,
            )
            if focused:
                self.fetch_catalog()
                self.fetch_model_catalog()
                self.fetch_profiles()
                # Order matters: ``begin_live_startup`` runs the whole startup
                # sequence — compatibility check included — but only on the *first*
                # connection, because re-running it would re-open the session. A
                # reconnect still has to re-establish the probe story (R9: "probe
                # results are re-validated on reconnect"), so the seam round is
                # scheduled separately for exactly the case the startup guard
                # declines to handle.
                if self._startup_done:
                    self._reprobe_seams("reconnect")
                self.begin_live_startup()
            else:
                # R9 for a connection nothing else will ever ask about. The
                # focused connection gets its probe story from the startup
                # sequence and its reconnect story from the branch above; a
                # background connection has neither, so without this line its
                # board stays never-probed forever and its sessions are never
                # enumerated. ``attach`` on the first round for this connection
                # and ``reconnect`` afterwards, decided by whether a board
                # already exists rather than by a flag somebody has to maintain.
                trigger: ProbeTrigger = (
                    "reconnect"
                    if fleet_seam_board(self._fleet, named) is not None
                    else "attach"
                )
                self._sweep_connection_soon(named, trigger)

        if focused:
            # A reconnect can advance to the next dial state before the 50ms
            # coalescing tick. Hand this observed state to the existing bar now
            # so its non-colour form is not skipped between timer frames.
            self._refresh_bottom_status_bar()

    def note_reconnect(self, epoch: int) -> None:
        """Mark a successful reconnect in the transcript, once (F6).

        Nothing is cleared and nothing is re-requested here, and that *is* the
        reconciliation: the domain transcript is append-only and the prompt
        registry is keyed by ``request_id``, so a gateway that re-announces an
        outstanding prompt after the socket comes back updates the existing
        entry instead of adding a second one. The failure this avoids is the
        tempting alternative — resetting the session and re-reading history —
        which is precisely how a reconnect duplicates a transcript.

        ``epoch`` is the connection generation the correlator just opened
        (``RpcCorrelator.epoch``): 1 is the first attach of the run, 2 the first
        reconnect, and so on. It is what makes the "once" in the first line an
        enforced property rather than a description of the caller's manners. The
        marker is written only for an epoch newer than the last one marked, so a
        callback delivered twice for one connection — two ``bind`` calls, a
        re-armed reconnect loop — leaves one line, and a stale callback arriving
        after a newer connection is already up leaves none. Epochs only ever
        increase, so a single integer is the whole bookkeeping.
        """
        if epoch <= self._last_reconnect_epoch:
            return
        self._last_reconnect_epoch = epoch
        self.state = record_local_note(
            self.state, "reconnected to the gateway", at=self.state.last_observed_at
        )
        self._dirty = True

    # ── live calls (R3, R4, AE8) ─────────────────────────────────────────

    async def submit_live(self, text: str) -> RpcOutcome | None:
        """Send a composed message and write only what is actually known.

        Three outcomes, three different transcripts:

        * **confirmed** — the operator's line is written and the composer is
          cleared.
        * **refused** — nothing is written and the text is kept, because a
          message the gateway rejected was not said.
        * **unknown** — the line is written *and* marked with the reason the
          correlator actually reported, and the composer is cleared. Keeping the
          text as well would put the same message in two places and invite a
          resend, and a resend of a message that did arrive makes the agent do
          the work twice. The text is not lost either way: it is in the
          transcript.
        * **never sent** — a special case of unknown that is not unknown at all.
          When the call ended before anything reached a socket, the message was
          definitely not delivered, so it is marked as not sent *and* left in the
          composer, where one keypress sends it. This is the only unconfirmed
          case where a resend is the right thing to do, which is exactly why it
          has to be told apart from the ones where it is not.
        """
        dispatcher = self.dispatcher
        if dispatcher is None:  # pragma: no cover - guarded by every caller
            return None
        body = text.strip()
        if not body:
            return None

        outcome = await dispatcher.call(
            SUBMIT_METHOD,
            {"session_id": self.state.focused_session_id or "", "text": body},
            timeout=self.call_timeout,
        )

        if outcome.status == "error":
            self.composer.show_notice(outcome.notice)
            self._dirty = True
            return outcome

        delivery = delivery_of(outcome)
        self.state = record_submission(
            self.state, body, at=self.state.last_observed_at, delivery=delivery
        )
        if delivery == "not_sent":
            # `outcome.notice` ends "It may or may not have taken effect", which
            # is true of every other unknown and false of this one. The domain's
            # own line is shown instead so the screen and the transcript agree.
            self.composer.show_notice(DELIVERY_NOTES["not_sent"])
        else:
            self.composer.clear()
            self.composer.show_notice(outcome.notice)
        self._dirty = True
        return outcome

    async def interrupt_live(self) -> RpcOutcome | None:
        """Cancel the in-flight turn, and only claim it when the gateway agreed.

        The cancelled state is applied **only** on a confirmed reply. Applying
        it on an ``unknown`` would be worse than cosmetic: ``cancelled`` is
        sticky and suppresses later deltas, so an interrupt that never landed
        would silently swallow the rest of a turn that is still streaming.

        **A confirmed interrupt also declines the turn's outstanding prompts
        (R4/KTD8), and only a confirmed one does.** The prompts belong to the
        turn that just died, and the gateway is blocking on each of them;
        releasing them now beats leaving the operator with cards for a dead
        turn and the gateway waiting out its own timeout. An interrupt whose
        outcome is unknown declines nothing at all, for exactly the reason the
        cancelled state is not applied either: the turn may still be alive, and
        denying its approvals would refuse commands for work that is still
        running.
        """
        dispatcher = self.dispatcher
        if dispatcher is None:  # pragma: no cover - guarded by every caller
            return None

        # Captured once, before the await, and used for both the call and
        # the sweep below. Re-reading ``self.state.focused_session_id`` after
        # the await let a focus change during the round trip — a slow reply
        # plus a switch, reachable today at reconnect and once the ``/sessions``
        # switcher lands — decline the WRONG session's prompts: the interrupt
        # that was confirmed for session A would sweep whatever session B had
        # raised in the meantime (CR4 finding 2).
        target_session_id = self.state.focused_session_id
        outcome = await dispatcher.call(
            INTERRUPT_METHOD,
            {"session_id": target_session_id or ""},
            timeout=self.call_timeout,
        )

        if outcome.confirmed:
            # Only applied while the interrupted session is still the one
            # displayed (B1). ``cancel_turn`` mutates ``state.turn`` and its
            # streaming fields unconditionally, and those belong to whichever
            # session is *currently* focused — not necessarily the one this
            # call was for. A focus change during the round trip (reconnect,
            # or the ``/sessions`` switcher) would otherwise mark a live,
            # still-streaming session B interrupted because session A's
            # delayed confirm arrived after the switch. Skipped rather than
            # redirected: there is no other session's turn state on this
            # object to redirect it to (the same non-goal A3 documents), so A
            # is left to land its own, correct turn state the next time it is
            # focused.
            if self.state.focused_session_id == target_session_id:
                self.state = cancel_turn(self.state, at=self.state.last_observed_at)
        else:
            self.state = record_local_note(
                self.state, outcome.notice, at=self.state.last_observed_at
            )
        self.composer.show_notice(outcome.notice)
        self._dirty = True
        if outcome.confirmed:
            # After the interrupt's own notice, not before: the sweep's
            # sentences are the newer information, and the last one left on
            # the notice bar should be what the sweep did.
            await self.decline_outstanding_prompts(target_session_id)
        return outcome

    async def decline_outstanding_prompts(self, session_id: str | None) -> None:
        """Decline every outstanding prompt of one session, per kind (KTD8).

        The algorithm is per kind because the bridges are not alike on the
        wire:

        * **Approvals resolve as one** ``approval.respond {all: true, choice:
          "deny"}``. Not a loop of single answers: ``approval.respond`` carries
          no discriminator and pops the queue's head, so answering two
          uncorrelated approvals individually is the exact defect
          :func:`~talaria.domain.state.respond_to_prompt` refuses
          (``REFUSED_UNCORRELATED_APPROVAL``). One choice applied to the whole
          queue needs no correlation, which is what makes it correct here — and
          it is a denial, the only direction that is safe to apply to commands
          nobody re-read.
        * **Clarify, sudo and secret each get their kind's empty answer**, one
          ``*.respond`` apiece. They carry real request ids, so each one is
          aimed at the question that asked it.
        * **``terminal_read`` gets nothing.** Talaria answers it itself and it
          renders no card, so there is no blocked human to release and no
          reason to put a value on the wire for it.

        Sequential rather than concurrent: every call folds its outcome into
        ``self.state``, and two coroutines doing that from the same starting
        value would lose one of the two results.

        **Every outstanding id is latched before any of this sends (CR4
        findings 1 and 4).** The installed gateway's own ``session.interrupt``
        clears every pending clarify/sudo/secret and deny-alls the approval
        queue *before* it replies — so by the time this method's caller
        observes a confirmed interrupt, the gateway has already resolved
        every prompt this sweep is about to answer. A prompt still in
        ``prompts`` **and** one already ``answering`` (its own single answer
        in flight when the interrupt landed) are both tombstoned first, so a
        later definite ``not_sent`` for either — this sweep's own call, or the
        in-flight one that started before it — cannot restore a control the
        gateway is no longer holding. The sweep's sends below are kept
        regardless: harmless once latched, and the belt for a gateway build
        that does not clear pending state this way.
        """
        self.state = latch_resolved_prompts(
            self.state,
            (
                prompt
                for prompt in (*self.state.prompts, *self.state.answering)
                if session_id is None
                or prompt.session_id is None
                or prompt.session_id == session_id
            ),
        )

        # The approval test is the registry's own, not a re-derivation: an
        # approval still in ``prompts`` is exactly what
        # :func:`~talaria.domain.state.respond_to_all_approvals` will take, so
        # the sweep never makes a deny-all call that has nothing to deny and
        # then reports "no longer live" about an interrupt that went fine.
        if any(
            self.state.prompt_for(prompt.request_id) is not None
            for prompt in self.state.outstanding_approvals(session_id)
        ):
            await self.deny_all_approvals_live(session_id)

        mine = tuple(
            prompt
            for prompt in self.state.prompts
            if prompt.kind != "approval"
            and (
                session_id is None
                or prompt.session_id is None
                or prompt.session_id == session_id
            )
        )
        for prompt in mine:
            value = decline_value(prompt.kind)
            if value is None:
                # ``terminal_read``. Nothing is sent and nothing is said: the
                # operator was never shown a control for it.
                continue
            # The sweep's own captured session (B2), not whatever is
            # focused when this particular await lands — see
            # :meth:`respond_live`'s ``session_id`` paragraph.
            await self.respond_live(
                prompt.request_id, value, declined=True, session_id=session_id
            )

    # ── blocking prompts: the approval path and the four bridges (U8) ────

    def on_prompt_card_answered(self, message: PromptCard.Answered) -> None:
        """An operator answered a control. Route it, off the message pump.

        The value is taken out of the message here and passed straight to the
        coroutine. Nothing in this method logs it, formats it, or stores it on
        the app — for two of the five bridges it is a credential, and the code
        path is the same for all five so that it cannot be right for three of
        them and wrong for the others.

        **In replay the refusal is visible, like every other mutation.** A
        recorded corpus contains the prompts that were outstanding at the time,
        so the controls render; there is no gateway to answer, and a control
        that silently does nothing is exactly what AE11's inert-control rule
        exists to prevent. The value is dropped without being named — a replay
        corpus is a shared artifact, and "you typed this into a dead control" is
        not worth putting on screen.
        """
        message.stop()
        if self.mode == "replay" or self.dispatcher is None:
            # ``prompt-respond`` is the name ``MUTATION_CONTROLS`` already
            # reserved for this control. Passing the prompt's kind instead would
            # read better on screen and would route an unclassified name through
            # the refusal path, which that registry exists to refuse.
            self._refuse_mutation(PROMPT_RESPOND_CONTROL)
            return
        self._spawn_live(
            self._respond_and_discard(message.request_id, message.value, message.kind)
        )

    def on_prompt_card_declined(self, message: PromptCard.Declined) -> None:
        """The operator pressed ``escape`` on a card. Refuse the prompt (R3).

        **The wire value is decided here, from the kind, and never carried on
        the message.** :func:`~talaria.ui.prompts.decline_value` is the one
        place that knows an approval's decline is the explicit ``deny`` choice
        while the other three send their field empty — an empty *approval*
        choice is not a decline at all, because the gateway's consumer blocks
        only on ``None`` and ``"deny"`` and returns approved for anything else
        resolved (``tools/approval.py:3584``, ``:3679``; the ``:3291``/``:3320``
        this used to cite were re-verified on 2026-08-18 and are wrong).

        Everything after that is the ordinary answer path: same
        :meth:`respond_live`, same registry guards, same outcome discipline.
        A decline is an answer that happens to say no, so nothing about how it
        is sent, cleared, restored or recorded is a second set of rules.

        The replay refusal is the same one an answer gets, for the reason
        AE11's inert-control rule gives: a control that swallows a keypress
        and does nothing is indistinguishable from one that worked.

        **The kind this message carries is passed on to** :meth:`respond_live`
        **as the pairing it must still hold at send time.** ``value`` above is
        computed from ``message.kind`` — the kind the card had when the
        operator pressed escape — while :meth:`respond_live` picks the wire
        *method* from a later, independent read of the registry's kind for
        this id. Nothing used to check the two still agreed (CR4 finding 5):
        a registry id that expires and is reused under a different kind
        between the two reads would pair one kind's value with another
        kind's method — for approval, the one case where that matters, an
        empty value paired with the approval method is read as *approved*.
        """
        message.stop()
        if self.mode == "replay" or self.dispatcher is None:
            self._refuse_mutation(PROMPT_RESPOND_CONTROL)
            return
        value = decline_value(message.kind)
        if value is None:  # pragma: no cover - the card refuses these already
            return
        self._spawn_live(self._decline_and_discard(message.request_id, value, message.kind))

    async def _decline_and_discard(self, request_id: str, value: str, kind: PromptKind) -> None:
        await self.respond_live(request_id, value, declined=True, expected_kind=kind)

    def on_prompt_card_decline_refused(self, message: PromptCard.DeclineRefused) -> None:
        """Escape did nothing on the unanswerable card, and says so (CR4
        finding 6b, AE11). No mutation of any kind is attempted — the card's
        one control is the button in front of the operator, not this key.
        """
        message.stop()
        self._notice(DECLINE_NOT_OFFERED_HERE)

    def on_prompt_card_denied_all(self, message: PromptCard.DeniedAll) -> None:
        """Deny every approval queued in the session, as one call.

        Reachable only from a card the projection already marked unanswerable,
        and it is the escape from that state rather than a way around it: one
        choice applied to every queue entry needs no correlation, so it is
        correct whatever order the gateway holds them in.
        """
        message.stop()
        if self.mode == "replay" or self.dispatcher is None:
            self._refuse_mutation(PROMPT_RESPOND_CONTROL)
            return
        self._spawn_live(self._deny_all_and_discard(message.session_id))

    async def _deny_all_and_discard(self, session_id: str | None) -> None:
        await self.deny_all_approvals_live(session_id)

    async def deny_all_approvals_live(self, session_id: str | None) -> RpcOutcome | None:
        """Send one ``approval.respond`` with ``all: true``, denying the queue.

        The answerable queue is taken out of the registry before the call, for
        the same reason a single answer is: a second denial while the first is
        travelling is a second value delivered for questions that already have
        one.

        **No approval-kind outcome ever restores a card, on this call or on
        any follow-up it sends (the round-six policy, terminal for this
        path).** ``approval.respond`` carries no request id (R9) — the
        gateway pops whatever sits at the queue's FIFO head, or clears the
        whole queue for ``all: true`` — so no client-side scheme can aim a
        wire call at one specific approval. Every earlier attempt at this
        (B3's seq boundary, round four's resolved-count bound, round five's
        follow-up deny) only relocated the same race one layer down; round
        six stops redesigning the mechanism and adopts a policy instead.
        Restoring on ambiguity is the dangerous direction: a live-looking
        card the gateway has actually already resolved sends every later
        answer attempt back through the same ambiguous reply shape,
        restoring it again — an unkillable zombie. Settling and latching is
        safe even when it is *wrong* about a specific card: if the gateway
        genuinely still holds that approval, its own unannounced 300-second
        timeout (``tools/approval.py``'s ``_get_approval_timeout``, the same
        number :func:`~talaria.domain.state.age_out_approvals` mirrors
        locally) unblocks the waiting agent thread regardless of what
        Talaria's screen shows, and the gateway fails closed on that timeout
        (``"Silence is not consent."``) — which is itself the denial the
        operator's deny-all asked for. Nothing latching does locally changes
        whether that timeout fires; it only stops offering a control for a
        question the wire's own ambiguity had very likely already answered.
        The accepted residual is bounded: a card whose approval the gateway
        resolved by some other path may sit on screen, looking answerable,
        until the operator's next interaction with it settles it — one
        wasted keypress. :meth:`_deny_one_approval_followup`'s own docstring
        names the specific way a follow-up deny can leave exactly this
        residual. Non-approval kinds (clarify/sudo/secret) are unaffected:
        they carry real request ids on the wire, so their existing
        restore-on-``not_sent`` discipline (:meth:`respond_live`) is aimed
        and stays correct.

        **This path reads the outcome through :func:`read_answer`, the same
        function the single-answer path uses, and that is the fix rather than an
        implementation detail.** Deny-all is the *only* action the interface
        offers once two approvals queue, so the safety-critical case was funnelled
        into the one path that read neither the reply body nor the delivery
        table: a gateway that answered ``{"status": "expired"}`` — the exact body
        the single-answer path was taught to read — produced "denied every
        waiting approval", and an unconfirmed call produced the same sentence as
        a confirmed one. Two readings of one question drift, and these two
        drifted apart in the direction that grants rather than the direction
        that refuses.

        **The counts are reported as two numbers, and only one of them is
        called a denial.** ``all: true`` resolves every entry in the gateway's
        queue, including an approval whose own answer is still in flight — so
        reporting only the cards this call cleared under-counted a safety
        action by exactly the approvals the operator could least afford to lose
        track of. But summing the two over-claimed in the other direction: an
        in-flight approval's own respond may carry an affirmative, so calling
        it denied put two different fates for one command in one transcript.
        It is named and counted as undecided instead. See
        :class:`~talaria.domain.state.DenyAllScope`.
        """
        dispatcher = self.dispatcher
        if dispatcher is None:  # pragma: no cover - guarded by every caller
            return None

        focused = self.state.focused_session_id
        target = session_id or focused
        next_state, scope = respond_to_all_approvals(self.state, session_id=target)
        self.state = next_state
        self._dirty = True
        if not scope.taken:
            self._notice(PROMPT_NO_LONGER_LIVE)
            return None

        outcome = await dispatcher.call(
            RESPOND_METHODS["approval"],
            respond_params(
                "approval",
                request_id="",
                session_id=target,
                value=DENY_ALL_CHOICE,
                all_approvals=True,
            ),
            timeout=self.call_timeout,
        )

        verdict = read_answer("approval", outcome)
        # Defined here, not only inside the branch below, so the notice's
        # arithmetic (round five/six) can read them whichever way ``verdict``
        # came back — all zero unless a follow-up deny actually ran.
        followed_up: tuple[PendingPrompt, ...] = ()
        followup_resolved = 0
        followup_withdrawn = 0
        for prompt in scope.taken:
            self.state = settle_prompt(
                self.state, prompt.request_id, session_id=prompt.session_id
            )
        if verdict.restore:
            # **Round six: even a genuine not_sent for this call's own reply
            # latches rather than restores.** There is no resolved count and
            # no queue to follow up against when the call itself never
            # reached a socket, so ``scope.taken`` is latched directly — the
            # same policy the block below applies through the follow-up
            # mechanism, taken here in one step because there is nothing
            # ambiguous left to resolve against the wire.
            self.state = latch_resolved_prompts(self.state, scope.taken)
        else:
            # **The gateway's ``all: true`` resolves the queue as it stands
            # *there*, when the call executes — not as it stood *here*, when
            # ``scope`` was built.** An approval that registered while the
            # reply was on the wire is in that resolved queue too (CR4
            # finding 3), and ``scope`` alone has no way to name it. Taking it
            # the same way ``scope`` took everything else — through
            # :func:`~talaria.domain.state.respond_to_all_approvals` again,
            # now that the reply is in — moves it out of ``prompts`` so its
            # card leaves the screen, the same as every other approval this
            # call swept. Nothing on the wire follows this second call: this
            # confirmed reply already covers it, and only its own domain
            # state needs to catch up.
            late_state, late_scope = respond_to_all_approvals(self.state, session_id=target)
            self.state = late_state
            # **Bounded to approvals that existed at reply time (B3).** The
            # re-scope above has no causal boundary of its own: it takes
            # *everything* answerable right now, and "right now" is after an
            # ``await`` — an approval that registered on a frame arriving
            # strictly after this reply's own frame is not one the gateway's
            # ``all: true`` could have seen, because it did not exist yet
            # when the gateway acted. ``outcome.seq`` is the reply's own
            # frame position (``RpcOutcome.seq``); a prompt's ``seq`` is the
            # frame it registered on. ``outcome.seq is None`` means the
            # outcome never had a reply frame to read a position from (a
            # test double, or a call that never reached the gateway at all)
            # — nothing to bound against, so every taken approval is treated
            # as pre-existing, matching this method's behaviour before B3.
            boundary = outcome.seq
            if boundary is None:
                seq_eligible_late = late_scope.taken
                really_late_flight = late_scope.already_in_flight
            else:
                seq_eligible_late = tuple(p for p in late_scope.taken if p.seq <= boundary)
                really_late_flight = tuple(
                    p for p in late_scope.already_in_flight if p.seq <= boundary
                )
                # Given back exactly as this call found them: never sent,
                # never settled, never latched. The gateway's own reply
                # could not have resolved a queue entry that did not exist
                # when it was built, so this call has nothing to claim about
                # it — the operator sees its card and answers it normally.
                for prompt in late_scope.taken:
                    if prompt.seq > boundary:
                        self.state = restore_prompt(self.state, prompt)
            for prompt in seq_eligible_late:
                self.state = settle_prompt(self.state, prompt.request_id, session_id=target)
            # **The seq boundary is necessary but not sufficient (P1, U7
            # round four/five).** An approval whose event frame arrived on
            # the wire *before* the reply's own frame can still postdate the
            # gateway's own queue snapshot: a concurrent timeout, or a
            # different call's own ``approval.respond``, can pop an entry
            # from the gateway's queue between when it announced the
            # approval and when this call's ``all: true`` actually ran —
            # the frame ordering Talaria observed says nothing about that.
            # The reply's own ``resolved`` count (``resolve_gateway_approval``,
            # ``tools/approval.py``) is the one place the gateway's real
            # snapshot size is visible at all.
            #
            # **The count's only job is to detect ambiguity, never to pick
            # survivors (round five).** Guessing which oldest-``k`` of the
            # candidates the count actually covers and *restoring* the rest
            # was the round-four design, and it could misidentify: if the
            # gateway's real snapshot dropped one from the *middle* rather
            # than the youngest, restoring an already-resolved approval put
            # a live-looking card back on screen for a command the gateway
            # is done with — and because ``approval.respond`` reads no
            # ``request_id`` (verified against the installed gateway,
            # ``tui_gateway/methods_prompt.py:958-977`` and
            # ``tools/approval.py:2486-2519``: an empty or already-resolved
            # queue answers ``{"resolved": 0}`` with an ordinary confirmed
            # reply, never a raw exception or a mis-resolved unrelated
            # entry), any later answer against that phantom card comes back
            # the same ``not_sent``-shaped way and restores it again — an
            # unkillable zombie.
            #
            # When the count covers every candidate, they are ordered
            # oldest first by registration order and all settle and latch,
            # unchanged from round four. When it does not, the candidates
            # beyond the count get an individual follow-up
            # ``approval.respond`` instead of a restore
            # (:meth:`_deny_one_approval_followup`). A follow-up is safe
            # under *either* arm of the ambiguity, which is exactly why it
            # replaces the guess rather than refining it: if the gateway
            # already resolved this specific entry, the follow-up finds an
            # empty queue and answers harmlessly; if it is still genuinely
            # queued, the follow-up denies it — which is what the
            # operator's original deny-all meant for every entry it
            # reached, not only the ones the reply happened to name. Which
            # *specific* candidates are treated as "covered by the reply"
            # versus "followed up" does not have to be correct, only their
            # count does: an approval the gateway already resolved is safe
            # to latch directly OR to follow up (both land on "resolved,
            # not shown again"), so there is no wrong split, only wrapped
            # work when the guess happens to differ from the gateway's own.
            resolved_count = _reply_resolved_count(outcome)
            settled = tuple(
                sorted((*scope.taken, *seq_eligible_late), key=lambda p: (p.seq, p.request_id))
            )
            if resolved_count is not None and resolved_count < len(settled):
                followed_up, settled = settled[resolved_count:], settled[:resolved_count]
                for prompt in followed_up:
                    if await self._deny_one_approval_followup(prompt, target):
                        followup_resolved += 1
                    else:
                        followup_withdrawn += 1
            # **Every id this call swept is latched, including the ones it did
            # not take.** ``all: true`` resolves the whole queue at the
            # gateway, so an approval whose own single answer is still on the
            # wire has been resolved by *this* call as well — and under the
            # round-six policy that single answer's own owner never restores
            # it either (:meth:`respond_live` / ``_record_prompt_outcome``'s
            # approval carve-out), so nothing else clears that card. The
            # latch is the mechanism ``restore_prompt`` already consults,
            # applied to ``already_in_flight`` (both readings of it) as well
            # as to ``taken``.
            #
            # This ``else`` is reached only when this call's own reply was
            # not a definite ``not_sent``; that other case is handled above,
            # before this ``if``/``else``, by latching ``scope.taken``
            # directly instead of restoring it — the round-six policy means
            # both arms end in a latch, just by different routes.
            self.state = latch_resolved_prompts(
                self.state,
                (
                    p
                    for p in (
                        *settled,
                        *scope.already_in_flight,
                        *really_late_flight,
                    )
                ),
            )
        covered = f"{scope.denied} waiting"
        if scope.undecided:
            covered = f"{covered} (+{scope.undecided} {ANSWER_ALREADY_TRAVELLING})"
        if verdict.used:
            line = f"{DENIED_EVERY_APPROVAL}: {covered}"
            if verdict.reason is None:
                line = f"{line}, {_resolved_clause(outcome)}"
                # **The count only ever names what the reply itself resolved
                # (round five finding 2).** ``_resolved_clause`` reads the
                # reply's own ``resolved`` field, and before round five's
                # follow-up redesign that number was always the complete
                # story. It is not once a follow-up deny ran: those
                # candidates are denials too by the time this line is
                # written — every follow-up above has already been awaited
                # — but the wire's own count never counted them, so saying
                # only that count would understate what actually happened.
                # Named rather than folded into one bigger number, because
                # the two came from different calls and claiming one figure
                # would erase that the reply itself only vouches for the
                # first.
                #
                # **Round six splits that clause in two, at the wire, not at
                # whether a queue entry actually existed.** A follow-up's own
                # reply either reaches the gateway — an ordinary denial
                # (``resolved: 1``) or the harmless empty-queue answer
                # (``resolved: 0``), both counted as "followed up
                # individually" because both mean the follow-up was
                # genuinely served — or comes back a definite not_sent
                # (counted separately, as "unreachable and withdrawn").
                # Under the no-restore policy both outcomes settle the card
                # the same way, but only the first pair is a follow-up the
                # wire actually carried; the second is a card the operator's
                # deny-all could not reach at all, which this method's
                # docstring documents as the accepted residual. Folding the
                # second into "followed up" would claim contact the wire
                # never had.
                if followup_resolved:
                    line = f"{line}, {followup_resolved} followed up individually"
                if followup_withdrawn:
                    line = f"{line}, {followup_withdrawn} unreachable and withdrawn"
            else:
                # An unacknowledged call carries no count, so the delivery note
                # already answers "how many"; a second clause saying the gateway
                # did not say would repeat it. This used to be argued from
                # length — the combined line ran past the old 120-character cut
                # — but ``clip_transcript_line`` is far looser now and the
                # argument stands without it: the clause adds no information.
                line = f"{line} — {verdict.reason}"
        else:
            line = f"{scope.denied} approvals not denied — {verdict.reason}"
        self.state = record_local_note(self.state, line, at=self.state.last_observed_at)
        self._notice(line)
        self._dirty = True
        return outcome

    async def _deny_one_approval_followup(
        self, prompt: PendingPrompt, session_id: str | None
    ) -> bool:
        """Individually deny one approval the deny-all reply's own count
        left ambiguous (round five finding 1), never restoring it (round
        six policy).

        ``prompt`` is already settled (out of both ``prompts`` and
        ``answering``) by the time this runs — it was already taken as part
        of this same call's sweep. This sends one ordinary, single
        ``approval.respond`` (``all`` unset), which — verified against the
        installed gateway before this was written
        (``tui_gateway/methods_prompt.py:958-977``,
        ``tools/approval.py:2486-2519``) — pops whatever is at the head of
        the session's queue, or answers ``{"resolved": 0}`` harmlessly if
        the queue is already empty. It never raises for an absent target and
        never touches a different session's queue.

        **Which specific command this denies, if any, is not knowable from
        here — and that is fine.** ``approval.respond`` carries no
        ``request_id`` on the wire (R9), so there is no way to aim this at
        ``prompt`` specifically. The operator's original action was "deny
        every queued approval", and every candidate reaching this method
        already passed the seq boundary — it is part of that same original
        intent, not a later, unrelated approval. Denying whatever the queue
        actually holds fulfills that intent regardless of which entry it
        turns out to be — which is also why this is the specific site where
        a follow-up meant for one candidate can pop a different, genuinely
        later approval instead (:meth:`deny_all_approvals_live`'s docstring
        names this as the accepted residual): the queue has no way to tell
        this call which entry it is popping, only that popping one matches
        the operator's intent.

        **Never restored, on any outcome (round six).** Round five restored
        ``prompt`` on a definite not_sent; that instruction is superseded.
        Under the round-six policy ``prompt`` is latched here regardless of
        which of the two outcomes this call reaches — an already-resolved
        queue (``{"resolved": 0}``) or a genuine not_sent — because
        restoring on either one risks the unkillable zombie the class
        docstring describes, and latching wrongly self-heals through the
        gateway's own timeout instead. The return value distinguishes them
        only for the caller's notice text, and the line is drawn at the
        wire, not at whether a real queue entry was denied: ``True`` means
        this call's own reply reached the gateway at all — an ordinary
        denial (``resolved: 1``) or the harmless empty-queue answer
        (``resolved: 0``, ``disposition == "discarded"``) both count,
        because both mean the follow-up was genuinely served, and the caller
        reports either one as "followed up individually". ``False`` means
        ``verdict.restore``: a definite not_sent, this call never reaching a
        socket at all, and the caller reports it separately, as unreachable,
        because the wire gave it nothing to point to. The state action —
        latch, never restore — is identical either way; only the notice
        text this feeds back to the caller differs.
        """
        dispatcher = self.dispatcher
        if dispatcher is None:  # pragma: no cover - guarded by every caller
            return False
        outcome = await dispatcher.call(
            RESPOND_METHODS["approval"],
            respond_params(
                "approval", request_id="", session_id=session_id, value=DENY_ALL_CHOICE
            ),
            timeout=self.call_timeout,
        )
        verdict = read_answer("approval", outcome)
        self.state = latch_resolved_prompts(self.state, (prompt,))
        self._dirty = True
        return not verdict.restore

    async def _respond_and_discard(self, request_id: str, value: str, kind: PromptKind) -> None:
        await self.respond_live(request_id, value, expected_kind=kind)

    async def respond_live(
        self,
        request_id: str,
        value: str,
        *,
        declined: bool = False,
        expected_kind: PromptKind | None = None,
        session_id: str | None = None,
    ) -> RpcOutcome | None:
        """Answer one outstanding prompt, and only the one that asked (R9).

        ``declined`` changes **only the wording written down** (R3/KTD4): a
        decline is sent, cleared, restored and latched by exactly the rules
        above, because it is an answer that says no rather than a second kind
        of act. What it is not is an *answer* in the transcript — "sudo
        answered" for a control the operator refused is a false entry in the
        one record that says what was allowed — so the verb changes and
        nothing else does.

        **The registry is consulted before anything is sent, and it is what
        clears the prompt.** Both halves of the correlation clause are checked
        there — the request id must still be live *and* it must belong to the
        session currently focused — so an answer typed into a control that a
        ``*.expire`` cleared a moment earlier reaches no socket at all (R8), and
        an answer for a session that is no longer the focused one cannot be
        delivered to whatever question the new session happens to be asking.

        **``expected_kind``, when given, must still match the registry's own
        kind for this id (CR4 finding 5, B4).** Both single-answer callers —
        :meth:`on_prompt_card_answered` and :meth:`on_prompt_card_declined` —
        compute ``value`` from the kind their message carried and pass that
        same kind here; the wire *method* below is picked from
        ``prompt.kind``, an independent, later read of the registry. A
        mismatch means the id was reused under a different kind between the
        two reads, and sending would pair one kind's value with another
        kind's method — refused instead of guessed. This used to be wired
        only from the decline path; a stale answer card was still sent
        under whatever kind the registry now held for its id, which for a
        sudo password reused as a clarify made the value cross bridges.

        **``session_id``, when given, is who this answer is for — not
        necessarily whoever is focused right now (B2).** A single card the
        operator is looking at answers for the current focus, and every
        caller but one passes nothing and gets exactly that. The interrupt
        sweep (:meth:`decline_outstanding_prompts`) captures its own target
        session before it starts a *sequential* run of these calls; each
        earlier call in that run can itself await a round trip, and a focus
        change during one would otherwise make every later call in the same
        sweep re-read the *new* focus and refuse the session it was actually
        declining for (``REFUSED_WRONG_SESSION``) instead of sending.

        **The registry is consulted before anything is sent, and it is what
        clears the prompt.** Both halves of the correlation clause are checked
        there — the request id must still be live *and* it must belong to the
        session currently focused — so an answer typed into a control that a
        ``*.expire`` cleared a moment earlier reaches no socket at all (R8), and
        an answer for a session that is no longer the focused one cannot be
        delivered to whatever question the new session happens to be asking.

        **The prompt is cleared before the call goes out, not after it
        succeeds.** One question must not be able to collect two answers while
        the first is in flight; for a sudo password or a secret that is the
        worst retry available. The cost is that a call which fails is a question
        the operator can no longer answer — so the single outcome that is
        *definite* about non-delivery, ``not_sent``, puts the control back
        (:func:`~talaria.domain.state.restore_prompt`). Every other unconfirmed
        outcome leaves it cleared and marks the transcript, because a resend of
        an answer that did arrive is a second value delivered for one question.
        """
        dispatcher = self.dispatcher
        if dispatcher is None:  # pragma: no cover - guarded by every caller
            return None

        if session_id is None:
            session_id = self.state.focused_session_id
        prompt = self.state.prompt_for(request_id, session_id=session_id)
        if expected_kind is not None and prompt is not None and prompt.kind != expected_kind:
            # Refused before the registry is touched at all: the prompt now
            # live under this id may be a real, unrelated question, and it is
            # left exactly as it was for whoever asks about it correctly.
            self._notice(PROMPT_KIND_CHANGED)
            self._dirty = True
            return None
        next_state, refusal = respond_to_prompt(
            self.state, request_id, session_id=session_id
        )
        if refusal is not None or prompt is None:
            # The refused state is kept, not discarded. It carries the registry's
            # ``rejected_responses`` tally, which is the only observable trace
            # that the guard fired at all — dropping it leaves a counter that
            # can never move and a guard nothing can measure.
            self.state = next_state
            # Nothing is sent, and the refusal is named rather than silent: a
            # control that swallowed a keystroke and did nothing is
            # indistinguishable from one that answered. The registry chose which
            # sentence, because it is the thing that knows which guard fired.
            self._notice(refusal or PROMPT_NO_LONGER_LIVE)
            self._dirty = True
            return None
        self.state = next_state
        self._dirty = True

        with self.fleet_answer(session_id=session_id or "", request_key=request_id):
            outcome = await dispatcher.call(
                RESPOND_METHODS[prompt.kind],
                respond_params(
                    prompt.kind,
                    request_id=request_id,
                    session_id=session_id,
                    value=value,
                    # R18 as amended 2026-08-17: the gateway's own id, when it
                    # sent one, so an approval answer names the entry it means.
                    # ``""`` for a locally synthesized key, which never goes on
                    # the wire.
                    observed_request_id=prompt.observed_request_id,
                ),
                timeout=self.call_timeout,
            )

        self._record_prompt_outcome(prompt, value, outcome, declined=declined)
        return outcome

    @contextmanager
    def fleet_answer(self, *, session_id: str, request_key: str) -> Iterator[None]:
        """Hold one answer's fleet-scoped in-flight record for its round trip (KTD9).

        Two things ride on the record and both are R6's: while it is held, a
        session switch is refused fleet-wide, and the registry row the answer
        names cannot be retired or evicted out from under it. Released in a
        ``finally``, because a call that raised is a call that is no longer
        travelling — leaving the record behind would refuse every switch for the
        rest of the run.

        **This is the queue's entry point too**, not only
        :meth:`respond_live`'s. An answer aimed at a session Talaria does not
        drive never enters the focused engine's ``answering`` tuple — that
        tuple is the focused session's bookkeeping — so without this it would
        occupy no switch-refusal window at all, which is exactly the hole KTD9
        names.

        ``session_id`` should be the session's **durable** id where the caller
        holds one; :func:`~talaria.domain.state.begin_fleet_answer` documents
        why a runtime id is the weaker choice. The focused session's own
        ``session_key`` is substituted here when the caller named the focused
        runtime id, which is the case every card-answer path takes.
        """
        durable = session_id
        if session_id and session_id == self.state.focused_session_id:
            durable = self.state.session_key or session_id
        profile = self.fleet_profile
        self.fleet = begin_fleet_answer(
            self.fleet,
            profile=profile,
            session_id=durable,
            request_key=request_key,
            # The frame clock, never the wall clock (R20/KTD12) — it is what
            # R18's requested-with-age is measured from, and what makes a
            # replayed recording render the same age twice.
            at=self.state.last_observed_at,
        )
        try:
            yield
        finally:
            self.fleet = end_fleet_answer(
                self.fleet, profile=profile, session_id=durable, request_key=request_key
            )

    def _record_prompt_outcome(
        self,
        prompt: PendingPrompt,
        value: str,
        outcome: RpcOutcome,
        *,
        declined: bool = False,
    ) -> None:
        """Write what is known about one answer — never the answer itself.

        The only value that reaches the transcript is one the *gateway* offered
        (:func:`~talaria.ui.prompts.echoable_answer`), which is what keeps the
        approval audit trail useful without making the transcript an egress for
        the operator-typed bridges.

        **What the answer was applied to is written down beside it.** For an
        approval that is the command, because "did I allow that" is the question
        the transcript exists to answer afterwards and the choice alone does not
        answer it. The whole command is already in the arrival entry
        unclipped (:func:`~talaria.domain.state.prompt_registration_line`); the
        copy here is bounded by ``record_local_note``'s system-line clip, which
        marks its own cut.

        :func:`read_answer` decides what may be claimed, and it is the same
        function the deny-all path calls — so an unconfirmed delivery, a
        gateway that discarded the answer, and an outright error make the same
        claim whichever control the operator used.

        **Where the sentence goes is decided once, by
        :meth:`_report_prompt_outcome`, for all four outcome classes.** Round 4
        moved one of them — the answer that reached no socket — off the
        transcript, because a bridge that *serves* the transcript must not write
        into it. That was the loop, and fixing the loop is not the same as
        honouring the rule: refused, discarded and delivery-unconfirmed still
        wrote a line each, one line of self-contamination per failed read, with
        the code's own comment two branches lower stating the rule they broke.

        **The transcript line is written only while ``prompt``'s own session
        is still the one focused (the sweep-transcript fix, U7 round four).**
        This is the same caller ``session_id`` can name a session other than
        the currently-focused one for: the interrupt sweep
        (:meth:`decline_outstanding_prompts`) answers for its own captured
        target regardless of what becomes focused meanwhile. ``self.state``
        holds exactly one transcript, the focused session's, so a sweep
        outcome for a session that is no longer displayed has nowhere correct
        to be written — appending it anyway put "sudo declined" into whatever
        session the operator had since switched to. Settling and latching
        still happen unconditionally below; only the presentation is
        skipped, the same choice B1 made for ``cancel_turn``.
        """
        in_focus = prompt.session_id is None or prompt.session_id == self.state.focused_session_id
        row = PromptRow(
            request_id=prompt.request_id,
            kind=prompt.kind,
            summary=prompt.summary,
            choices=prompt.choices,
            session_id=prompt.session_id,
        )
        shown = echoable_answer(row, value)
        label = prompt.kind.replace("_", " ")
        applied_to = f" · {APPROVAL_COMMAND_LABEL}{prompt.command}" if prompt.command else ""
        # One verb pair, read from one flag, so the sentence a decline leaves
        # behind cannot drift from the sentence an answer leaves behind. The
        # negative form is what every unconfirmed and refused branch below
        # uses; "sudo not declined — nothing was sent" and "sudo not answered —
        # nothing was sent" then say the same true thing about the same
        # outcome, and the operator can tell which act failed.
        verb, negated = ("declined", "not declined") if declined else ("answered", "not answered")
        answered = (
            f"{label} {verb}: {shown}{applied_to}"
            if shown
            else f"{label} {verb}{applied_to}"
        )
        verdict = read_answer(prompt.kind, outcome)

        if verdict.disposition == "error":
            self.state = settle_prompt(self.state, prompt.request_id, session_id=prompt.session_id)
            self._report_prompt_outcome(
                prompt, f"{answered} — {verdict.reason}", notice=outcome.notice, in_focus=in_focus
            )
            self._dirty = True
            return

        if verdict.restore and prompt.kind in UNATTENDED_KINDS:
            # **Neither half of the restore applies to a prompt Talaria answers
            # itself, and both halves did damage.**
            #
            # Restoring re-offers a control to an operator — and there is no
            # operator here. The prompt went straight back into the projection,
            # where ``_answer_unattended_prompts`` re-dispatched it on the very
            # next render, which failed the same way, which restored it again:
            # measured at 136 ``terminal.read.respond`` calls in 400ms, for as
            # long as the socket stayed down.
            #
            # Writing the note is worse, because it is the failure the clean
            # path two branches below already refuses to commit: the line goes
            # into the buffer this bridge *serves*, so each attempt made the
            # answer larger than the one before it — 159 characters to 884
            # across three cycles. The operator is still told, on the notice
            # bar, which the transcript projection does not read.
            self.state = settle_prompt(self.state, prompt.request_id, session_id=prompt.session_id)
            self._notice(f"{label} not answered — {verdict.reason}")
            self._dirty = True
            return

        if verdict.restore and prompt.kind == "approval":
            # **An approval-kind not_sent never restores, on this ordinary
            # single-answer path exactly as on the deny-all's own follow-up
            # path (round six policy;
            # :meth:`deny_all_approvals_live`'s docstring, and
            # :meth:`_deny_one_approval_followup`).** ``approval.respond``
            # carries no request id (R9), so a restored card here carries the
            # same unkillable-zombie risk the deny-all docstring describes:
            # a later answer for this same, still-unaimed approval reaches
            # the wire the same ambiguous way and would restore it again.
            # Settling and latching instead is safe even when the gateway
            # genuinely never saw this call, because the gateway's own
            # approval timeout unblocks the waiting agent thread regardless
            # of what Talaria's screen shows, and that timeout is itself a
            # denial. The transcript wording below is unchanged from the
            # general branch beneath this one — the wire outcome really was
            # a not_sent — only the state action differs.
            #
            # **``latch_resolved_prompts`` alone is not enough here.** Its
            # own docstring is explicit that it touches only
            # ``flushed_prompt_ids`` and leaves ``answering`` untouched,
            # because the deny-all's own use of it names ids whose in-flight
            # call is *someone else's* to settle. Here it is this very call's
            # own prompt, still sitting in ``answering`` — the settle
            # ``restore_prompt`` would otherwise have performed internally
            # has to be done explicitly before the latch.
            self.state = settle_prompt(
                self.state, prompt.request_id, session_id=prompt.session_id
            )
            self._latch_queue_copy(prompt)
            self.state = latch_resolved_prompts(self.state, (prompt,))
            if in_focus:
                self.state = record_local_note(
                    self.state,
                    f"{label} {negated}{applied_to} — {verdict.reason}",
                    at=self.state.last_observed_at,
                )
            self._notice(DELIVERY_NOTES["not_sent"])
            self._dirty = True
            return

        if verdict.restore:
            # ``restore_prompt`` settles the in-flight entry itself, and it is
            # the one path that may decline to put the control back — an expiry
            # that landed while this call was out already closed the question.
            # Approval kind never reaches here (carved out above); this
            # branch now handles only clarify/sudo/secret, which carry real
            # request ids and so restore safely and correctly.
            self.state = restore_prompt(self.state, prompt)
            if in_focus:
                self.state = record_local_note(
                    self.state,
                    f"{label} {negated}{applied_to} — {verdict.reason}",
                    at=self.state.last_observed_at,
                )
            self._notice(DELIVERY_NOTES["not_sent"])
            self._dirty = True
            return

        self.state = settle_prompt(self.state, prompt.request_id, session_id=prompt.session_id)
        self._latch_queue_copy(prompt)
        if verdict.disposition == "discarded":
            self._report_prompt_outcome(
                prompt,
                f"{label} {negated}{applied_to} — {verdict.reason}",
                notice=verdict.reason or "",
                in_focus=in_focus,
            )
            self._dirty = True
            return

        note = verdict.reason
        if note is None and prompt.kind in UNATTENDED_KINDS:
            # A terminal-read that went through cleanly says nothing anywhere.
            # No human was involved, so there is no act to record, and there is
            # nothing to tell the operator either.
            self._dirty = True
            return
        line = answered if note is None else f"{answered} — {note}"
        # **The same sentence on both surfaces, not two readings of one fact.**
        # This used to show ``outcome.notice``, which is the transport layer's
        # own wording — so one ``NO_REPLY_IN_TIME`` produced "delivery
        # unconfirmed — the message was sent and no reply arrived before the
        # deadline" in the transcript and "approval.respond outcome unknown —
        # no reply arrived before the deadline. It may or may not have taken
        # effect." on the notice bar, at the same moment, about the same call.
        # ``submit_live`` already overrides ``outcome.notice`` for exactly this
        # reason and says so in a comment; the prompt path did not inherit it.
        # ``line`` carries no operator-typed value: ``answered`` only ever
        # names a choice the *gateway* offered (:func:`echoable_answer`).
        self._report_prompt_outcome(prompt, line, in_focus=in_focus)
        self._dirty = True

    def _latch_queue_copy(self, prompt: PendingPrompt) -> None:
        """Settle the queue's copy of a prompt the card path just cleared (R18/AE2).

        ``settle_prompt`` clears the focused engine's registry, which removes
        feed A's row. It cannot touch feed B's: a polled approval is derived from
        ``FleetState.approval_detail`` and stays there until the gateway stops
        listing it, which is one poll away at best and never if the answer's
        outcome was ambiguous. Until U8B wired feed B that was invisible, because
        ``approval_detail`` was permanently empty — ``settle_queue_item`` had no
        production caller and ``build_queue``'s ``settled=`` was always empty.

        **Called from both settle branches and from neither restore branch**, and
        that is the whole rule: a restored control is a question the operator can
        still answer, so its queue row must stay. Every other outcome leaves the
        control cleared, and an item offered with no control behind it is exactly
        the inert control AE11 forbids.

        One helper rather than the same two lines in two branches — this file's
        own ``AnswerVerdict`` docstring records what happened the last time one
        reading of an outcome was written twice.
        """
        # A prompt with no session id names no row, so there is no queue copy to
        # settle — feed A builds its identity from the row the prompt anchors to
        # and an unanchored prompt has none.
        if not prompt.session_id:
            return
        self.fleet = settle_queue_item(
            self._fleet,
            profile=self.fleet_profile,
            session_id=prompt.session_id,
            request_key=prompt.request_id,
            at=self.state.last_observed_at,
        )

    def _report_prompt_outcome(
        self,
        prompt: PendingPrompt,
        line: str,
        *,
        notice: str | None = None,
        in_focus: bool = True,
    ) -> None:
        """Put one outcome sentence where that prompt's kind allows it to go.

        **The transcript is not a neutral log for four of the five bridges and
        is an input for the fifth.** ``terminal.read`` serves this buffer
        straight back to the agent, so a line Talaria writes about its own
        answer becomes part of the next answer: self-contamination, one line per
        failed read, growing with the number of reads rather than with anything
        the session did. Round 3 met the compounding form of this — a restore
        loop that took one answer from 159 characters to 884 in three cycles —
        and round 4 fixed the loop by taking one outcome class off the
        transcript. Three others were still writing.

        So the rule is applied here, once, over every class: a prompt Talaria
        answers itself (:data:`~talaria.ui.prompts.UNATTENDED_KINDS`) reports on
        the notice bar only. The operator still learns what happened — the
        notice bar is not a surface the read projection reads — and the reason
        the notice carries the full sentence for those kinds is that it is now
        the only place carrying it.

        ``notice`` overrides what the operator is shown; the default is the same
        sentence that went to the transcript, which is the property
        ``test_the_notice_bar_and_the_transcript_say_one_thing_about_one_answer``
        pins.

        ``in_focus`` is ``False`` only for a sweep answering a session that
        is no longer the one displayed (the sweep-transcript fix, U7 round
        four) — ``self.state`` holds one transcript, the focused session's,
        and a line about a different session's prompt has nowhere correct
        to go. The notice bar still shows: it is not part of any session's
        own record, and the operator gets no other signal that the sweep's
        answer landed.
        """
        if prompt.kind not in UNATTENDED_KINDS:
            if in_focus:
                self.state = record_local_note(
                    self.state, line, at=self.state.last_observed_at
                )
            self._notice(line if notice is None else notice)
            return
        self._notice(line)

    def _notice(self, message: str) -> None:
        """Show a composer notice, unless the composer is no longer mounted.

        A respond can outlive the screen: the call is in flight when the app
        tears down, the reply or the timeout lands afterwards, and the handler
        runs to completion against a composed tree that no longer exists.
        Textual raises ``NoMatches`` from the query in that case, which would
        surface as an unrelated-looking error at the end of an orderly exit
        (R36). Only the *absence* of the widget is tolerated here — nothing else
        is caught, so a genuine rendering failure still raises.
        """
        try:
            composer = self.composer
        except NoMatches:  # pragma: no cover - teardown ordering
            return
        composer.show_notice(message)

    # ── terminal-read: answered from the projection, with no human (F2) ──

    def _answer_unattended_prompts(self, snapshot: Snapshot) -> None:
        """Dispatch an answer for every prompt Talaria answers itself.

        Called from the render pass because that is where a fresh projection
        exists, and a terminal-read is a question *about* that projection. It
        never blocks the pass: the answer is spawned as a live task.

        **This dispatches on sight, so the bound is that every outcome settles
        the prompt.** ``_answering`` covers only the round trip — it is
        discarded in a ``finally`` — so a prompt still in the registry after
        its answer resolves is re-dispatched on the very next tick, forever.
        That is what made the ``restore`` branch in :meth:`_record_prompt_outcome`
        a loop rather than a retry, and the fix belongs there rather than in a
        second latch here: a latch would bound the symptom while leaving a row
        on screen that the projection says is outstanding and nothing will ever
        answer.
        """
        if self.mode != "live" or self.dispatcher is None or self._teardown_started:
            return
        for row in snapshot.prompts.rows:
            if row.kind not in UNATTENDED_KINDS or row.request_id in self._answering:
                continue
            self._answering.add(row.request_id)
            self._spawn_live(self._answer_and_discard(row))

    async def _answer_and_discard(self, row: PromptRow) -> None:
        await self.answer_terminal_read(row)

    def transcript_view_for_read(self) -> TranscriptView | None:
        """The buffer terminal-read serves, or ``None`` when nothing honest is.

        Two conditions, and both are ordinary rather than exceptional. Teardown
        has begun, so the interface the read describes is being dismantled while
        the answer is composed. Or no snapshot exists yet, which is the case for
        a read that arrives before the first render — the state may hold frames,
        but nothing has been on screen, and terminal-read's contract is about
        the screen.

        ``None`` is the honest answer to both, and
        :func:`~talaria.domain.projection.terminal_read` turns it into a raised
        :class:`ProjectionUnavailableError` rather than an empty buffer, because
        "the terminal has no lines" is a claim and this is an absence of one.
        """
        if self._teardown_started or self.snapshot is None:
            return None
        return self.snapshot.transcript

    async def answer_terminal_read(self, row: PromptRow) -> RpcOutcome | None:
        """Serve the transcript buffer, or send nothing at all (KTD10, F2).

        Public because it is reachable from outside the render pass and has to
        be drivable from there. :meth:`shutdown_sources` cancels the in-flight
        live tasks, which covers a read that has not started; a read that is
        already running when teardown begins is the case this method's own guard
        exists for, and a test drives exactly that order.

        The gateway's bridge tolerates silence — the read blocks for 30 seconds
        and then expires (``tui_gateway/server.py:2981-2998``) — so "the
        projection cannot answer" has a correct behaviour that is not an error
        reply and is certainly not a plausible-looking screen. The failure is
        surfaced locally instead, where the operator can see it and the agent
        cannot mistake it for the contents of a terminal.
        """
        try:
            response = terminal_read(
                self.transcript_view_for_read(),
                viewport_rows=self.viewport_rows(),
                start_line=row.read_start,
                count=row.read_count,
            )
        except ProjectionUnavailableError as exc:
            self._answering.discard(row.request_id)
            # Scrubbed for the same reason every other operator-facing failure
            # string is: this text is built from an exception, and an exception
            # is the one place a dial target can arrive somewhere nobody
            # expected it. Both halves are asserted in the test suite — the
            # credential is gone *and* the message still says what went wrong.
            detail = scrub_urls(str(exc))
            line = f"{TERMINAL_READ_UNAVAILABLE} {detail}".strip()
            self.state = record_local_note(
                self.state, line, at=self.state.last_observed_at
            )
            # **And it settles** (U6, R14). Surfacing the failure and leaving the
            # prompt registered was the recorded unavailable-projection defect:
            # this method is dispatched from the render pass on sight, so a
            # prompt still in the registry afterwards is re-dispatched on the
            # very next tick — one failure line per tick, forever, for a question
            # nothing would ever answer. The latch takes it out of the registry,
            # tombstones it against a late restore, and names the resolved-failed
            # on the session's registry row as well as in its transcript.
            prompt = self.state.prompt_for(row.request_id, session_id=row.session_id)
            if prompt is not None:
                self.fleet, _ = latch_unservable_prompt(
                    self.fleet,
                    prompt,
                    profile=self.fleet_profile,
                    reason=detail,
                    at=self.state.last_observed_at,
                )
            self._notice(line)
            self._dirty = True
            return None

        try:
            return await self.respond_live(
                row.request_id, json.dumps(response.to_json_dict())
            )
        finally:
            self._answering.discard(row.request_id)

    def viewport_rows(self) -> int:
        """How many transcript rows are actually on screen (KTD10).

        Served truthfully because it is a number the UI already knows, and an
        agent paging through scrollback with ``start``/``count`` uses it to
        decide how far to step. A pane that has not been laid out yet reports
        the projection's documented default rather than zero, which would tell
        the agent the screen has no rows at all.
        """
        try:
            height = self.transcript.size.height
        except Exception:  # noqa: BLE001 - queried before the screen exists
            return DEFAULT_VIEWPORT_ROWS
        return height if height > 0 else DEFAULT_VIEWPORT_ROWS

    # ── composer ─────────────────────────────────────────────────────────

    def on_chat_text_area_submitted(self, message: ChatTextArea.Submitted) -> None:
        """Enter on composed text: a local control, a command, or a message.

        The order is fixed by PC6 and is not an implementation convenience. The
        Talaria-local four are resolved *first*, before the catalogue is
        consulted and before the replay refusal — they never touch a socket, so
        there is nothing for replay to refuse, and a gateway that later ships a
        command called ``/quit`` must not be able to take the operator's exit
        away.

        In replay everything below that echoes nothing and keeps the text.
        Writing the composed message into the transcript would render a line
        identical to one that had actually been delivered, and no operator
        could tell the difference afterwards — which is the whole reason AE11
        makes inertness visible rather than silent.
        """
        message.stop()
        invocation = resolve_command(message.text, self.catalog)

        if isinstance(invocation, LocalInvocation):
            dispatched = self.perform_local_command(invocation)
            if dispatched:
                self._push_history(message.text)
            return

        if isinstance(invocation, UnsupportedInvocation):
            # Nothing is sent and the text is kept. AE9's clause is that these
            # degrade *honestly*: the operator learns the command exists, that
            # it belongs to a different client, and that Talaria did not
            # quietly do something else instead.
            self._refuse_unsupported(invocation)
            return

        if self._collapses_in_flight > 0:
            # A large paste is out at the gateway and the composer still holds
            # the literal body, so this Enter would put the whole thing into the
            # turn — the exact outcome KTD16 exists to prevent. The local four
            # above are already past this point: they never touch a socket, and
            # ``/quit`` must work whatever else is in flight.
            self._notice(PASTE_COLLAPSE_IN_FLIGHT)
            return

        if isinstance(invocation, GatewayInvocation):
            if self.mode == "replay" or self.dispatcher is None:
                self._refuse_mutation(COMMAND_DISPATCH_CONTROL)
                return
            self._push_history(message.text)
            self._spawn_live(self._dispatch_and_discard(invocation))
            return

        if self.mode == "replay":
            self._refuse_mutation("submit")
            return
        self._push_history(message.text)
        self._spawn_live(self._submit_and_discard(message.text))

    def _push_history(self, raw_text: str) -> None:
        """Record a dispatched line in the in-memory recall list (C1, KTD1).

        The list is bounded, append-only, and never persisted to disk. Empty
        strings after stripping never enter. This is the single place history
        grows — the widget's Up/Down path only moves the cursor, never appends.
        """
        stripped = raw_text.strip()
        if not stripped:
            return
        self.composer_history = history_push(self.composer_history, stripped)

    async def _submit_and_discard(self, text: str) -> None:
        """Adapt :meth:`submit_live` to the ``None``-returning task shape."""
        await self.submit_live(text)

    # ── U9: the command catalogue, dispatch, and the local control set ───

    async def action_toggle_palette(self) -> None:
        await self.palette.toggle()

    async def open_theme_picker(self) -> None:
        """Open theme mode at the currently applied session selection."""
        self._theme_preview_anchor = self._capture_layout_anchor()
        await self.palette.open_theme_picker(
            self.theme_registry.specs,
            current_slug=self.theme,
            session_slug=self.session_theme_slug,
        )
        self._restore_layout_anchor(self._theme_preview_anchor)

    def on_palette_region_theme_selected(
        self, message: PaletteRegion.ThemeSelected
    ) -> None:
        """Keep an accepted picker choice in memory for this process only."""
        self.session_theme_slug = message.slug
        self._restore_layout_anchor(self._theme_preview_anchor)
        self.call_after_refresh(self._clear_theme_preview_anchor)

    def on_palette_region_theme_cancelled(
        self, message: PaletteRegion.ThemeCancelled
    ) -> None:
        """Restore the exact session selection captured when theme mode opened."""
        self.session_theme_slug = message.session_slug
        self._restore_layout_anchor(self._theme_preview_anchor)
        self.call_after_refresh(self._clear_theme_preview_anchor)

    # ── U2: the model picker ──────────────────────────────────────────────

    async def action_toggle_picker(self) -> None:
        await self.open_picker("models")

    async def action_toggle_profiles(self) -> None:
        await self.open_picker("profiles")

    async def open_picker(self, mode: PickerMode) -> None:
        """Put the modal picker up, or say why there is nothing to put up.

        The dialog is built from whatever listing the app is holding *at the
        moment it opens*, rather than being kept in sync with one. A listing
        that arrives while the dialog is up does not disturb it, which is the
        behaviour an operator mid-selection wants; a listing that arrives from
        a *different* gateway is caught on selection instead, by the epoch
        check in :meth:`select_model` and :meth:`select_profile` that both
        surfaces already share.

        The dialog opens **on the row already in use** rather than at row one,
        which is what :meth:`session_model_in_focus` is read for here. An
        operator who has switched models re-opens ``/models`` to change away
        from something, and starting them at the top of a hundred-row list
        makes them find their own position before they can leave it.

        Refusing before opening rather than opening an empty dialog is the same
        AE9 honesty clause the rest of this module follows, and there are
        **three** states to keep apart, not two: Talaria never read the list, a
        read failed and said why, and a read succeeded and the gateway offers
        nothing. All three would look identical as an empty dialog — a modal
        with no rows and no explanation is the worst of the three, because the
        operator has to close it to find out anything. So each says its own
        sentence in the notice bar and no dialog opens at all.
        """
        if mode == "models":
            catalog = self.model_catalog
            if catalog is None:
                self._notice(self.model_catalog_failure or MODELS_NOT_FETCHED)
                return
            if catalog.is_empty:
                self._notice(NO_PROVIDERS)
                return
            source: PickerSource = ModelPickerSource(catalog, self.session_model_in_focus)
        else:
            directory = self.profiles
            if directory is None:
                self._notice(self.profiles_failure or PROFILES_NOT_FETCHED)
                return
            if directory.is_empty:
                self._notice(NO_PROFILES)
                return
            source = ProfilePickerSource(
                directory, self.profile_endpoints, current=self.current_profile
            )

        self.push_screen(PickerDialog(source), self._picker_dismissed(mode))

    @property
    def session_model_in_focus(self) -> SessionModel | None:
        """The remembered switch, but only if it was made on the focused session.

        Checked on every read rather than cleared on every transition that
        could invalidate it, because the list of such transitions is not one
        anybody can enumerate once and keep correct — a profile switch, a
        resume onto a different session, a reconnect that lands somewhere
        else. Comparing the session id at the point of use cannot fall behind
        a transition nobody thought of; a clear-on-event scheme silently can,
        and the failure it produces is a confident claim about the wrong
        session.
        """
        remembered = self.session_model
        if remembered is None:
            return None
        if not remembered.applies_to(self.state.focused_session_id or ""):
            return None
        return remembered

    def _picker_dismissed(self, mode: PickerMode) -> Callable[[str | None], None]:
        """What happens when the dialog closes, as a callback rather than a wait.

        ``push_screen_wait`` reads better and cannot be used here: Textual
        raises ``NoActiveWorker`` unless it is awaited inside a worker, and the
        picker is opened from a local command handler running as an ordinary
        supervised task. The callback form has no such requirement, and routing
        the selection back through :meth:`_spawn_live` keeps it on the same
        supervised path every other live action takes — so a failure in a
        selection is reported the same way, rather than vanishing into a screen
        callback nobody is watching.
        """

        def dismissed(chosen: str | None) -> None:
            # The caret went to the dialog when it opened; the composer is
            # where it belongs once the dialog is gone (``talaria/ui/focus.py``).
            self.composer.focus()
            if chosen is None:
                return
            if mode == "models":
                self._spawn_live(self._select_model_and_discard(chosen))
            else:
                self._spawn_live(self._switch_profile_and_discard(chosen))

        return dismissed

    def fetch_model_catalog(self) -> None:
        """Start the admin catalogue read, once per connection epoch (KTD4).

        Called only from the ``connected`` transition, not from ``on_mount``
        the way :meth:`fetch_catalog` is. The two fetches have different
        dependencies: ``commands.catalog`` is a WebSocket RPC and needs
        :attr:`dispatcher` actually connected, which is why that fetch also
        fires at mount as a fallback for a dispatcher double already usable
        there. The admin catalogue is a separate HTTP surface (KTD1) with no
        such dependency — but tying it to :attr:`_connection_epoch`, which
        only exists once a connection has opened, means there is nothing
        useful to stamp a mount-time fetch with anyway.

        **Unlike** :meth:`fetch_catalog`, **an available result is never
        reused across a call.** The command listing is treated as a property
        of Talaria's registry and kept until a fetch actually fails; the model
        list is a property of whichever gateway is on the other end of the
        socket, and a reconnect may land on a different one — U4's whole
        purpose. Every ``connected`` transition here starts a fresh read.
        """
        if self.mode != "live" or self.admin_client is None or self._teardown_started:
            return
        if self._model_catalog_task is not None and not self._model_catalog_task.done():
            return
        self._model_catalog_task = self._supervise(
            asyncio.create_task(self._load_model_catalog_and_discard()),
            "the model catalogue fetch",
        )

    async def _load_model_catalog_and_discard(self) -> None:
        await self.load_model_catalog()

    async def load_model_catalog(self) -> ProviderCatalog | None:
        """Read the admin model catalogue, or record why it could not be read.

        A failure leaves :attr:`model_catalog` at ``None`` rather than an
        empty catalogue, the same AE9 honesty clause :meth:`load_catalog`
        follows: an empty picker says the gateway has no providers, which is a
        claim, while ``None`` plus :attr:`model_catalog_failure` says Talaria
        could not read the list, which is what happened (R7).
        """
        admin_client = self.admin_client
        if admin_client is None:  # pragma: no cover - guarded by every caller
            return None
        # Captured before the call, so a reconnect that lands mid-fetch
        # stamps this read with the epoch it was actually asked for on, not
        # whatever epoch is current when the await returns.
        epoch = self._connection_epoch
        try:
            catalog = await admin_client.model_options()
        except AdminError as exc:
            self.model_catalog = None
            self.model_catalog_failure = str(exc)
            self._model_catalog_epoch = 0
            return None
        self.model_catalog = catalog
        self.model_catalog_failure = ""
        self._model_catalog_epoch = epoch
        return catalog

    # ── U4: the profile picker ────────────────────────────────────────────

    def fetch_profiles(self) -> None:
        """Start the profile-directory read, once per connection epoch (KTD4).

        Wired to the same ``connected`` transition as
        :meth:`fetch_model_catalog` and re-read as unconditionally, for a
        sharper version of the same reason: ``gateway_running`` is a fact about
        the moment it was read, and a switch is precisely the case where the
        previous gateway's answer is about a machine Talaria is no longer
        talking to.
        """
        if self.mode != "live" or self._teardown_started:
            return
        if not isinstance(self.admin_client, ProfileAdmin):
            # Not an error and not a failed fetch: no gateway was asked. The
            # picker says so in its own words rather than showing an empty
            # list, which would be a claim about the gateway's inventory.
            self.profiles = None
            self.profiles_failure = PROFILES_UNAVAILABLE
            return
        if self._profiles_task is not None and not self._profiles_task.done():
            return
        self._profiles_task = self._supervise(
            asyncio.create_task(self._load_profiles_and_discard()),
            "the profile listing fetch",
        )

    async def _load_profiles_and_discard(self) -> None:
        await self.load_profiles()

    async def load_profiles(self) -> ProfileDirectory | None:
        """Read the profile directory, or record why it could not be read.

        A failure leaves :attr:`profiles` at ``None`` rather than an empty
        directory — AE9's honesty clause again: an empty listing says the
        gateway knows of no profiles, which is a claim, while ``None`` plus
        :attr:`profiles_failure` says Talaria could not read the list (R7).
        """
        client = self.admin_client
        if not isinstance(client, ProfileAdmin):
            self.profiles = None
            self.profiles_failure = PROFILES_UNAVAILABLE
            return None
        epoch = self._connection_epoch
        try:
            directory = await client.list_profiles()
        except AdminError as exc:
            self.profiles = None
            self.profiles_failure = str(exc)
            self._profiles_epoch = 0
            return None
        self.profiles = directory
        self.profiles_failure = ""
        self._profiles_epoch = epoch
        return directory

    def fetch_catalog(self) -> None:
        """Start the catalogue read, if one is wanted and none is running.

        Called at mount **and** every time the transport reports ``connected``,
        which is one path rather than two special cases. Mount alone is too
        early against a real socket: ``LiveSource`` dials asynchronously, so a
        call issued from ``on_mount`` resolves ``not connected`` before the
        handshake finishes and the palette would spend the session saying the
        catalogue was unavailable. Connect alone would miss a dispatcher that
        is already usable at mount, which is every test double.

        An *available* catalogue is never re-fetched, so an ordinary reconnect
        costs nothing; one that failed is retried when the socket comes back,
        which is the case that made this a retry rather than a one-shot.

        The task is held in its own attribute rather than in ``_live_tasks``,
        for the same reason the status loop is: those are calls the *operator*
        made, and :meth:`settle_live` exists so a caller can wait for them. A
        background fetch nobody asked for must not be something every such wait
        has to outlast — a gateway that never answers it would otherwise stall
        an unrelated interrupt's settle for the whole call timeout.
        """
        if self.mode != "live" or self.dispatcher is None or self._teardown_started:
            return
        if self.catalog is not None and self.catalog.available:
            return
        if self._catalog_task is not None and not self._catalog_task.done():
            return
        self._catalog_task = self._supervise(
            asyncio.create_task(self._load_catalog_and_discard()), "the catalogue fetch"
        )

    async def _load_catalog_and_discard(self) -> None:
        await self.load_catalog()

    # ── the live startup sequence (R2, R34, AE7) ─────────────────────────

    def begin_live_startup(self) -> None:
        """Start the compatibility check and KTD7's session open, once.

        Driven from the ``connected`` callback rather than from ``on_mount``,
        and that is not a stylistic choice. ``LiveSource`` dials asynchronously
        and only :meth:`~talaria.transport.source.LiveSource.start` knows when
        the handshake finished; a sequence kicked off at mount would find itself
        racing the dial, and every probe would resolve *not connected* and be
        graded ``unproved`` — a clean gateway reported as an unverifiable one,
        every single run.

        Guarded on :attr:`_startup_done` rather than on connection count, so a
        reconnect re-fetches the catalogue (which is cheap and may have changed)
        and does **not** re-open the session (which would abandon the operator's
        conversation for a new one).

        **A live app with no** :attr:`startup` **selection runs no sequence at
        all.** That is the signal that this app owns a session rather than being
        pointed at one — the launcher supplies a selection, the framework gate
        and every dispatcher-double test do not, and neither of those wants a
        ``session.create`` fired at whatever is on the other end.
        """
        if self.mode != "live" or self.dispatcher is None or self._teardown_started:
            return
        if self.startup is None or self._startup_done:
            return
        if self._startup_task is not None and not self._startup_task.done():
            return
        self._startup_task = self._supervise(
            asyncio.create_task(self._run_live_startup()), "the live startup sequence"
        )

    async def _run_live_startup(self) -> None:
        """Verify first, then open. Order matters, in one direction only.

        The check runs before the session exists, so ``spawn_tree.list`` is
        probed with an empty session id and may come back ``refused`` — which is
        reported and does not block. Running the check *after* the open would
        give it a real id, and would also mean creating a session before finding
        out whether this gateway is one Talaria understands.
        """
        await self.verify_gateway()
        if self.startup is not None:
            await self.open_session(self.startup)
        self._startup_done = True

    async def verify_gateway(
        self, *, trigger: ProbeTrigger = "attach"
    ) -> CompatReport | None:
        """Probe the permitted set and name every gap on screen (R34, AE7, R9).

        What lands in the transcript is only the blocking rows. A clean check
        says nothing, because a line reading "every method verified" would be
        false — most of the baseline is never probed at all — and a line that
        told the truth about that would be an operator-facing paragraph on
        every launch about a thing that is fine.

        **No counts appear in this docstring any more, and that is the fix.**
        They went stale three times. The paragraph used to read "a line reading
        '17 methods verified' … five were verified and twelve were not probed";
        commit ``ec861fa`` pinned ``slash.exec`` and never touched this file,
        U7 of the 2026-08-08 v0.2 plan pinned ``session.list`` read-only, and
        v0.4's U5 promoted ``session.active_list`` and added
        ``approval.pending`` as presence-only — each time leaving arithmetic
        here that no longer added up. The counts live in
        :data:`~talaria.domain.compat.COMPAT_BASELINE` and are printed from it
        by :meth:`~talaria.transport.compat_check.CompatReport.lines`, which is
        the only place they can be right by construction.

        The gaps do not stop the launch. AE7 blocks the *daily-driver verdict*
        on any gap, and that verdict lives in
        ``docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md``. A client that
        refused to start because one response grew a key would be less useful
        than one that starts and says which surface it could not verify.

        **v0.4 U5 folded the seam round into this method rather than beside
        it.** The three cadence points R12 names — attach, reconnect, and the
        five-minute revalidation — are exactly the three moments this method
        already runs or is now made to run, and ``trigger`` records which one
        it is so the seam line can say where its knowledge came from. Nothing
        on a render path calls this.
        """
        dispatcher = self.dispatcher
        if dispatcher is None:  # pragma: no cover - guarded by every caller
            return None
        report = await self._probe_round(
            self.fleet_profile,
            dispatcher,
            trigger=trigger,
            session_id=self.state.focused_session_id or "",
            health=self.admin_client if isinstance(self.admin_client, HealthProbe) else None,
        )
        self.compat = report
        # The status region is the *only* surface a seam change writes to, and
        # deliberately. It is a current-state surface, which is what a seam is;
        # an absence also written into the append-only transcript would repeat
        # every reconnect and would put "the roster is off" in the middle of a
        # conversation, which is where R10 wants a named absence least.
        await self._render_seams()
        if report.blocking:
            for verdict in report.blocking:
                self.state = record_local_note(
                    self.state,
                    f"{COMPAT_BLOCKED} {verdict.describe()}",
                    at=self.state.last_observed_at,
                )
            self._dirty = True
            self._notice(report.lines()[0])
        return report

    async def _render_seams(self) -> None:
        """Push the seam board onto the status region, ages and all.

        The clock is ``state.last_observed_at`` — the frame clock, never a wall
        clock read at render time — so a replayed recording renders the same
        ages twice (R20, AE8).

        Guarded on the widget being present for the same reason
        :meth:`note_connection_state` guards its notice: this can be reached
        from a probe round that lands after the screen has come down.

        Nothing repaints when the text has not moved, which is what lets the
        render tick call this at its own frequency — see :meth:`_refresh_seam_ages`
        for why it must.
        """
        lines = board_lines(self.seams, self.state.last_observed_at)
        if lines == self._seam_lines:
            return
        try:
            region = self.status_region
        except NoMatches:  # pragma: no cover - teardown race
            return
        self._seam_lines = lines
        anchor = self._capture_layout_anchor()
        try:
            await region.apply_seams(lines)
        finally:
            self._restore_layout_anchor(anchor)

    async def _refresh_seam_ages(self) -> None:
        """Let a painted seam line grow older on screen.

        **A seam's age is the one thing on that line that changes with no event
        behind it**, and until this existed nothing repainted it. The board was
        painted exactly once per probe round, on the statement after the round
        folded in, with no await in between — so every seam was drawn at an age
        of zero and stayed there. Two whole revalidation intervals could pass
        with the line still claiming "0s ago", and the ``stale`` wording the
        renderer already knew how to write could not appear on screen at all.
        That is R20's obligation reported as a constant, and R24's "say how old
        the knowledge is" answered with a number that was never true after the
        instant it was written.

        The repaint probes nothing: :func:`~talaria.domain.compat.board_lines` is
        a pure function over the board and a clock, so the cadence rule this unit
        is built around — attach, reconnect, revalidation, never per render —
        governs *probing* and is untouched by *drawing*.

        It returns before painting when the board has never been painted, which
        is not an optimization. An unprobed board renders four never-observed
        rows; painting them from the render tick would put a seam board on screen
        in replay, where no probe runs and there is nothing to be never-observed
        *about* yet.
        """
        if self._seam_lines is None:
            return
        await self._render_seams()

    async def _probe_round(
        self,
        profile: str,
        dispatcher: LiveDispatcher,
        *,
        trigger: ProbeTrigger,
        session_id: str,
        health: HealthProbe | None,
    ) -> CompatReport:
        """One probe round against ONE named connection, folded into ITS board.

        The whole of what :meth:`verify_gateway` used to do inline, lifted out
        so a connection that is not the focused one can have the same round run
        against it. :func:`~talaria.transport.compat_check.probe_seams` was
        already per-connection — it takes a dispatcher and hands back results
        rather than a board, precisely so the caller decides which board they
        belong to — and this method is that decision made explicit.

        Nothing here renders, writes a transcript note, or touches
        :attr:`compat`. Those are the focused connection's presentation and stay
        with the caller that owns a screen; what a background connection needs
        is its own row in :attr:`FleetState.seam_boards`, which the queue reads
        per connection (R24).
        """
        report, results = await probe_seams(
            dispatcher,
            trigger=trigger,
            session_id=session_id,
            timeout=self.call_timeout,
            health=health,
        )
        board = fleet_seam_board(self._fleet, profile) or empty_board(profile)
        self._fleet = record_seam_board(
            self._fleet,
            profile=profile,
            board=apply_probe_round(board, results, at=self.state.last_observed_at),
        )
        return report

    async def sweep_connection(self, profile: str, *, trigger: ProbeTrigger) -> None:
        """Ask one NON-focused connection what it can do, then what it holds.

        **Both halves, and the pairing is the point rather than a convenience.**
        Probing alone would be a regression dressed as a fix: while a background
        connection has no board, ``connection_notices`` says "capabilities not
        probed … its sessions are not in the queue", and a probe on its own
        deletes that sentence — the roster seam comes back ``present``, every
        notice for that connection falls silent, and its sessions are still not
        enumerated. The queue would go from a true sentence to no sentence with
        the gap unchanged, which is exactly the reading R14 forbids: an empty
        queue must never mean "we could not ask". So the sweep earns the silence
        the probe would otherwise take for free. Pinned by
        ``test_a_background_connection_is_probed_and_swept_together``.

        ``session_id`` is empty and ``health`` is ``None``, both deliberately.
        The focused session belongs to a different gateway, so sending its id
        here would probe a session-scoped method with an id that connection
        never issued; and :attr:`admin_client` is the home profile's admin
        surface, so its health answer describes the wrong process. The
        ``http-runner`` seam therefore stays never-observed on a background
        connection — which costs the queue nothing, because
        :func:`~talaria.domain.queue.connection_notices` reads exactly two seams,
        ``roster`` and ``approval-detail``, and neither is the health one.
        """
        inventory = self.connections
        if not isinstance(inventory, ConnectionInventory):
            return
        dispatcher = inventory.source_for(profile)
        if dispatcher is None:
            return
        await self._probe_round(
            profile, dispatcher, trigger=trigger, session_id="", health=None
        )
        await self.sweep_roster(profile, dispatcher)

    async def sweep_roster(self, profile: str, dispatcher: LiveDispatcher) -> None:
        """The roster half of :meth:`sweep_connection`, on its own.

        Factored out for :meth:`poll_due_connections`, which reaches the FOCUSED
        connection too — and there the probe half would be a second, conflicting
        probe path beside :meth:`verify_gateway`'s, issued with an empty session
        id that connection never asked for.

        Calling this alone on a *background* connection would be U7's defect in
        reverse, so nothing does: :meth:`sweep_connection` remains the only way a
        background connection is reached, and it still pairs the two halves. The
        asymmetry is safe in this direction and not the other — a roster sweep
        without a probe leaves ``connection_notices`` saying "capabilities not
        probed", which stays true, whereas a probe without a roster sweep deletes
        a true sentence and leaves the gap it described.
        """
        outcome = await dispatcher.call(
            LIST_SESSIONS_METHOD, {"limit": SESSIONS_LIST_LIMIT}, timeout=self.call_timeout
        )
        if not outcome.confirmed:
            # Marked stale rather than emptied, and no notice written: this is a
            # connection the operator is not looking at, so the report belongs in
            # the queue's own per-connection lines rather than in the focused
            # composer, which would name a failure with no visible subject.
            self.fleet = listing_failed(
                self.fleet, profile=profile, at=self.state.last_observed_at
            )
            return
        await self._seed_registry(
            dispatcher, decode_session_list(outcome.result), profile=profile
        )
        self._dirty = True

    def _sweep_connection_soon(self, profile: str, trigger: ProbeTrigger) -> None:
        """Schedule :meth:`sweep_connection` from a synchronous callback.

        ``note_connection_state`` is a transport callback and cannot await, so
        the round is supervised the same way :meth:`_reprobe_seams` supervises
        the focused one. Supervised identically on purpose, and the premise is
        read from the code rather than assumed: every dispatcher call underneath
        returns an :class:`~talaria.transport.rpc.RpcOutcome` on every exit
        rather than raising, and both decoders degrade instead —
        :func:`~talaria.domain.session_list.decode_session_list`
        (``talaria/domain/session_list.py:200``, "degrading rather than raising")
        and :func:`~talaria.domain.session_list.decode_active_list` beside it
        return an empty listing for a reply that is not a mapping. So an
        exception here is a Talaria defect rather than a misbehaving gateway —
        and a defect that only ever fires on a connection nobody is looking at is
        the one that most needs to be loud.
        """
        if self.mode != "live" or self._teardown_started:
            return

        async def _round() -> None:
            await self.sweep_connection(profile, trigger=trigger)

        self._supervise(asyncio.create_task(_round()), "the background connection sweep")

    def _socket_generation(self, profile: str) -> int:
        """This connection's own socket generation, never the focused counter.

        :attr:`_connection_epoch` counts *focused* connects across the whole
        fleet, while ``route_frame`` compares a channel's generation against the
        per-connection ``arrival_epoch`` that every
        :class:`~talaria.transport.connection_set.TaggedFrame` carries off its
        own socket. The two numberings coincide only for a fleet of one whose
        home never moves; where they diverge, stamping a channel with the larger
        focused counter makes ``generation < channel.generation`` true for every
        frame that connection will ever deliver, so no inbound event ends a row's
        staleness again — U6's round-twelve defect, one scope up. Pinned by
        ``test_a_background_rows_liveness_survives_a_busier_focused_connection``.

        Falls back to the focused counter when there is no inventory to ask,
        which is the single-connection and replay case: there the two numberings
        are the same one.
        """
        inventory = self.connections
        if isinstance(inventory, ConnectionInventory):
            status = inventory.status_of(profile)
            if status is not None:
                return status.epoch
        return self._connection_epoch

    def _reprobe_seams(self, trigger: ProbeTrigger) -> None:
        """Schedule one seam round from a synchronous callback.

        ``note_connection_state`` is a transport callback and cannot await, so
        the round is supervised like every other background task this app
        starts rather than fired and forgotten.
        """
        if self.mode != "live" or self.dispatcher is None or self._teardown_started:
            return

        async def _round() -> None:
            await self.verify_gateway(trigger=trigger)

        self._supervise(asyncio.create_task(_round()), "the seam probe round")

    async def revalidate_seams(self) -> None:
        """The five-minute re-probe (R12), and the only scheduled one.

        Asks :func:`~talaria.domain.compat.seam_probe_due` first, so a timer
        firing early — or several timers, or a caller with a shorter interval —
        cannot turn the cadence into something faster than the interval. The
        render tick is a different timer entirely and never reaches this method,
        which is the mechanical half of "never per-render".
        """
        if self.mode != "live" or self._teardown_started:
            return
        if self.dispatcher is not None and seam_probe_due(
            self.seams, self.state.last_observed_at, "revalidation"
        ):
            await self.verify_gateway(trigger="revalidation")
        inventory = self.connections
        if not isinstance(inventory, ConnectionInventory):
            return
        for profile in inventory.profiles:
            if profile == self.fleet_profile:
                continue
            board = fleet_seam_board(self._fleet, profile)
            if board is not None and not seam_probe_due(
                board, self.state.last_observed_at, "revalidation"
            ):
                continue
            # Each connection is asked against ITS OWN board's clock, not the
            # focused one's. A shared due-check would let one connection's fresh
            # round suppress another's overdue one, and the connection whose
            # probe is overdue is the one whose silence the queue is least
            # entitled to read as quiet (R24).
            await self.sweep_connection(profile, trigger="revalidation")

    async def poll_approval_detail(self, profile: str, dispatcher: LiveDispatcher) -> None:
        """Feed B: ask one connection for its foreign sessions' pending approvals.

        **This call warms an agent build on someone else's session, and the
        trigger-status restriction is what makes that acceptable.** Read
        first-hand rather than inferred: ``approval.pending``
        (``tui_gateway/methods_prompt.py:1454``) resolves through ``_sess``,
        which is ``_sess_building`` plus ``_wait_agent``, and
        ``_sess_building``'s own docstring at ``tui_gateway/server.py:2519``
        says it warms the agent build. That is exactly the side effect the
        *probe* of this method exists to avoid, which is why the probe must stay
        bare — ``_sessions.get("")`` misses and refuses 4001 before touching
        anything.

        A parameterless call cannot warm a build; this one can. So what covers it
        is not the probe's argument but the operator ruling of 2026-08-17:
        :func:`~talaria.domain.queue.approval_detail_due` restricts the call to
        :data:`~talaria.domain.queue.APPROVAL_DETAIL_TRIGGER_STATUSES` —
        ``waiting`` and ``working``. Pinned by
        ``test_no_approval_detail_is_asked_for_a_session_outside_the_trigger_statuses``,
        which asserts NO CALL rather than a call that returns nothing.

        **Those two words are not merely "sessions that look busy" — they are
        derived from the agent being live**, which is what makes the restriction
        a safety property rather than a heuristic. ``_session_live_status``
        (``tui_gateway/server.py:8545-8555``) returns ``waiting`` only when
        ``_session_pending_kind`` finds a pending prompt, and ``working`` only
        when the session is ``running``; both require a built agent. The two
        words it returns for a session whose agent is NOT live are exactly the
        two this gate excludes — ``starting`` (build in flight, where
        ``_wait_agent`` would block) and ``idle`` (no build at all). So the
        restriction cannot admit a session that a call here would build.

        The ids named here are RUNTIME ids and the fold files the answer under
        the durable key — see :func:`~talaria.domain.state.approval_detail_targets`
        for the wire evidence that they differ.
        """
        for session_id in approval_detail_targets(self._fleet, profile):
            outcome = await dispatcher.call(
                APPROVAL_PENDING_METHOD,
                {"session_id": session_id},
                timeout=self.call_timeout,
            )
            if not outcome.confirmed:
                # **Redundant by construction, and kept anyway — the comment says
                # which, because a mutation pass proved it.** Removing this line
                # changes no behaviour: ``RpcOutcome`` is constructed in exactly
                # five places (``talaria/transport/rpc.py:332-388``) and passes
                # ``result=`` in only one of them, the ``status="ok"`` branch, so
                # an unconfirmed outcome always carries ``result=None`` —
                # ``decode_pending_approvals`` then answers ``answered=False``
                # and ``apply_approval_pending`` returns the fleet unchanged.
                # That is a structural proof of equivalence rather than a
                # surviving mutant read as inertness.
                #
                # It stays because it puts R10's rule where the call is made:
                # a refused detail call is the seam's story, already told by
                # ``connection_notices``, and must never read as "this session
                # has no approvals". A reader of this loop should not have to
                # trace two modules to learn that.
                continue
            self.fleet = apply_approval_pending(
                self._fleet,
                decode_pending_approvals(outcome.result, at=self.state.last_observed_at),
                profile=profile,
                session_id=session_id,
                at=self.state.last_observed_at,
            )
            self._dirty = True

    async def poll_due_connections(self) -> None:
        """KTD2's cadence, finally driven: sweep every connection that is due.

        ``next_poll_due_at`` has carried the whole rule since U3 — a 2-second
        coalesce after a ``sessions.changed`` hint, a 30-second backstop — and
        nothing called it, so a gateway announcing that its session list had
        changed was heard and not acted on. ``route_frame`` records that hint for
        every connection including the focused one, which is why this loop covers
        the whole inventory rather than only the background half.

        **The due-check reads the wall clock, and that is the point of the stamp
        split.** ``last_poll_at`` is a schedule stamp, not an age; measuring it
        against ``state.last_observed_at`` made a background connection's cadence
        a function of traffic on the connection it is not, and a fleet quiet
        everywhere never polled at all. In live mode the two clocks share a scale
        anyway — ``talaria/transport/source.py:824`` stamps every live frame from
        ``time.time()`` — so the hint and this comparison are directly
        comparable, which is what ``next_poll_due_at``'s own ``max()`` needs.

        Live-only, like every other scheduled round here: a replay has no gateway
        to poll, so no determinism claim touches this.
        """
        if self.mode != "live" or self._teardown_started:
            return
        inventory = self.connections
        if not isinstance(inventory, ConnectionInventory):
            return
        now = _wall_clock()
        for profile in inventory.profiles:
            channel = self._fleet.channels.get(profile)
            # A connection with no channel yet has never been polled, which
            # ``next_poll_due_at`` reads as due immediately — the same answer,
            # reached without constructing a row for it first.
            if channel is not None and now < next_poll_due_at(channel):
                continue
            dispatcher = inventory.source_for(profile)
            if dispatcher is None:
                continue
            if profile == self.fleet_profile:
                await self.sweep_roster(profile, dispatcher)
            else:
                await self.sweep_connection(profile, trigger="revalidation")
            # Detail AFTER the roster, never before: which sessions are due is a
            # function of the statuses the sweep just refreshed, so asking first
            # would aim this at the previous poll's picture of the fleet.
            await self.poll_approval_detail(profile, dispatcher)

    async def open_session(self, selection: StartupSelection) -> RpcOutcome | None:
        """Resolve KTD7's selection into one focused session (R2).

        Three paths and no switcher afterwards, which is what
        :class:`~talaria.domain.startup.StartupSelection` already guarantees.
        ``--resume`` is two calls rather than one: ``session.most_recent`` is the
        read-only method that names the target, and ``session.resume`` is the
        mutating one that opens it. Splitting them is what lets the "no previous
        session" case be *reported* instead of quietly turning into a new
        conversation.

        **At most one call runs at a time (C1, U7 round three).** See
        :attr:`_resume_in_flight`, checked and held below. A second
        selection made while one is still on the wire is refused before it
        ever reaches the dispatcher — startup's own two paths (``new``/
        ``resume``) run once, before any switch is possible, and have
        nothing to race, so the guard costs them nothing.

        This used to be a client-side generation counter instead: choosing
        B then C while B's reply was still on the wire dispatched two
        ``session.resume`` calls with no ordering guarantee on the
        gateway's side either, and comparing a generation number when a
        reply landed could discard C's *reply* but never stopped the second
        *send* — the gateway's own active session and Talaria's belief
        could still diverge underneath a client that showed no error. With
        the send itself refused instead, the generation counter had nothing
        left to discard and was retired along with it.

        **This has run against a real Hermes gateway**, first on 2026-08-04. R2
        is graded *measured* in
        ``docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md`` (evidence-table
        row 17): across 15 of the 17 recordings in the cited corpus, a gateway
        reply carrying a ``session_id`` came back from ``session.create`` or
        ``session.most_recent``. The live evidence is those recordings; what the
        *tests* prove is the call sequence, the precedence, and what the
        interface does with each outcome, all against a stub.

        This paragraph used to read: "**This is not covered by any live
        evidence.** It has never run against a Hermes gateway — see R2 in [the
        verdict], which records it as unmet." Both halves were true when written
        and both stopped being true on 2026-08-04.
        """
        dispatcher = self.dispatcher
        if dispatcher is None:  # pragma: no cover - guarded by every caller
            return None

        # Refuses a second send outright rather than queuing it (C1) — see
        # :attr:`_resume_in_flight` and this method's own docstring.
        if self._resume_in_flight:
            self._notice(SWITCH_ALREADY_IN_FLIGHT)
            return None
        self._resume_in_flight = True
        try:
            cols = max(self.size.width or DEFAULT_SESSION_COLS, 1)

            if selection.mode == "new":
                with self._landing():
                    return self._land_session(
                        await dispatcher.call(
                            CREATE_METHOD, {"cols": cols}, timeout=self.call_timeout
                        )
                    )

            target = selection.session_id
            if selection.mode == "resume":
                found = await dispatcher.call(MOST_RECENT_METHOD, {}, timeout=self.call_timeout)
                if not found.confirmed:
                    self._report_startup_failure(found)
                    return found
                raw = (found.result or {}).get("session_id")
                target = raw if isinstance(raw, str) and raw else None
                if target is None:
                    self._notice(NO_SESSION_TO_RESUME)
                    self.state = record_local_note(
                        self.state, NO_SESSION_TO_RESUME, at=self.state.last_observed_at
                    )
                    self._dirty = True
                    return found

            # Rechecked immediately before the dispatch, not only inside
            # ``_land_session`` after the reply lands (P1, U7 round two). That
            # later check refuses to *apply* a switch whose answer raced it, but
            # by then ``session.resume`` has already gone out — an answer that
            # started travelling in the window since :meth:`open_sessions_picker`
            # returned (listing fetched, dialog open, operator selects) reached
            # this point unrefused and put the RPC on the wire regardless of
            # what its reply would do with it.
            refusal = fleet_switch_refusal(self.fleet)
            if refusal:
                self._notice(refusal)
                return None
            with self._landing():
                outcome = await dispatcher.call(
                    RESUME_METHOD,
                    {"session_id": target or "", "cols": cols},
                    timeout=self.call_timeout,
                )
                return self._land_session(outcome)
        finally:
            self._resume_in_flight = False

    @contextmanager
    def _landing(self) -> Iterator[None]:
        """Hold inbound frames for the duration of one landing (KTD2).

        Everything between issuing the ``session.create``/``session.resume``
        call and applying its seeded history runs inside this. On exit the held
        frames are folded in arrival order, so a live event that raced the
        reply lands *after* the history it follows rather than before it.

        The flush is unconditional — a landing that failed still releases what
        it held. Dropping those frames to keep the transcript "clean" would
        lose real session content on exactly the path where the operator most
        needs to see what the gateway is doing.
        """
        self._landing_depth += 1
        try:
            yield
        finally:
            self._landing_depth -= 1
            if self._landing_depth == 0 and self._deferred_frames:
                held, self._deferred_frames = self._deferred_frames, []
                for record, held_profile, held_epoch in held:
                    self.ingest(record, profile=held_profile, epoch=held_epoch)

    def _land_session(self, outcome: RpcOutcome) -> RpcOutcome:
        """Focus the session the gateway just handed back and seed its history.

        Reads the id out of the reply rather than reusing the one that was
        asked for. ``session.resume`` answers with the id it actually resumed
        (``tui_gateway/methods_session.py:306-699``), and those differ whenever
        the gateway maps a stored id onto a live one — focusing the id Talaria
        sent would then point the whole interface at a session the gateway is
        not streaming.

        **Both identities are kept** (R6/R7). ``session_id`` is the runtime id
        events are stamped with, so it drives correlation; ``session_key`` (and
        ``resumed``, which is the same target on every resume return site,
        ``methods_session.py:494-506``, ``:581-596``, ``:643-801``) is the
        durable id that survives the process, so it is what the picker names a
        session by and what a later resume asks for.

        **The seed is applied inside the landing barrier**, which the caller
        holds open across the RPC — see :meth:`_landing`.
        """
        if not outcome.confirmed:
            self._report_startup_failure(outcome)
            return outcome
        result = outcome.result or {}
        raw = result.get("session_id")
        if not isinstance(raw, str) or not raw:
            self._notice(f"{SESSION_START_FAILED} the reply named no session")
            return outcome
        refusal = fleet_switch_refusal(self.fleet)
        if refusal:
            # Landing is refused for the same reason a switch is: an answer is
            # still travelling — anywhere in the fleet, not only in the focused
            # engine (R6/KTD9). Seeding into a state that did not move would
            # append the landed session's history to the session still on
            # screen.
            self._notice(refusal)
            return outcome
        # ``session.resume`` carries the durable id as ``session_key`` and
        # ``resumed``; ``session.create`` carries none of those two, only
        # ``stored_session_id`` (``talaria/domain/compat.py:160``) — reading
        # only the first two left every newly created session with no
        # durable key at all, so the picker could never recognize it as the
        # current row (P1, U7 round two).
        stored = (
            result.get("session_key")
            or result.get("resumed")
            or result.get("stored_session_id")
        )
        # Captured before ``land_session`` runs: it is the *previous* focus
        # that decides whether the transcript ``land_session`` returns is the
        # fresh buffer of a real switch or the retained one of a reconnect —
        # by the time ``land_session`` returns, ``focused_session_id`` is
        # ``raw`` either way, so this is the only point that still knows
        # which branch it took (CR6 finding 1).
        previously_focused = self.state.focused_session_id
        self.state = land_session(
            self.state,
            raw,
            session_key=stored if isinstance(stored, str) and stored else None,
        )
        if previously_focused != raw:
            # The retain branch (landing the session already focused) keeps
            # the transcript on screen exactly as it is — seeding into it
            # unconditionally re-appended the same history a second time,
            # reachable the moment a picker lets the operator choose the
            # already-focused row (``/sessions``, U7).
            if outcome.method == RESUME_METHOD:
                # B5: a resumed session names itself on arrival. Only the
                # real-switch branch adds the transcript row; the retain
                # branch below is deliberately without one (B5 KTD3b), and
                # ``--new`` landings are silent too — the operator created
                # the session, so there is no identity question (KTD3). The
                # durable id is the one the picker names a session by; when
                # the reply carries none, the runtime id is named and
                # labelled as such (AE4).
                if isinstance(stored, str) and stored:
                    line = RESUMED_SESSION_ANNOUNCEMENT.format(session_key=stored)
                else:
                    line = RESUMED_SESSION_ANNOUNCEMENT_RUNTIME.format(session_id=raw)
                self.state = record_local_note(
                    self.state, line, at=self.state.last_observed_at
                )
            count = result.get("message_count")
            self.state = seed_history(
                self.state,
                result.get("messages"),
                omitted=result.get("messages_omitted") is True,
                count=count if isinstance(count, int) and not isinstance(count, bool) else 0,
            )
            self._apply_handover(
                result,
                runtime_id=raw,
                durable_id=stored if isinstance(stored, str) and stored else "",
            )
        else:
            # B3: landing the session already focused — the picker row the
            # marker did not recognize — confirms the keypress on the
            # composer surface instead (KTD4). The transient half of B5's
            # KTD3b handoff: the notice lives here, B5's durable row lives in
            # the moved-focus branch above, so the two stay mutually
            # exclusive (never both, never neither).
            self._notice(SESSION_ALREADY_FOCUSED_NOTICE)
        self._dirty = True
        return outcome

    def _apply_handover(
        self, result: Mapping[str, Any], *, runtime_id: str, durable_id: str
    ) -> None:
        """What an attach recovered, and what it visibly did not (KTD8).

        Three acts, in this order, on the branch where focus actually moved:

        **Ownership.** The session is now one this run drives, so its registry
        row says ``we_drive`` and stops being a candidate for OP2's dialog. A
        session Talaria has attached to is not a session Talaria can steal from
        somebody else again.

        **Hydration.** The shared live-session payload builder behind
        ``session.create``, ``session.resume`` and ``session.activate`` carries
        ``pending_approval`` and ``pending_clarify`` when they exist at the
        pinned revision (``tui_gateway/server.py:8708-8711``). Each becomes an
        ordinary request event and is folded through the ordinary reducer, so a
        hydrated card is registered, deduped, transcribed and answered by
        exactly the machinery a live request uses — no second path, nothing to
        drift.

        **Residue.** Everything else the session was waiting on is
        unrecoverable by construction: it was announced to the transport this
        attach displaced, no method re-announces it, and inventing a card for it
        would be a claim. So it latches a visible resolved-failed on the row and
        writes the same sentence to the transcript. On the revision serving this
        machine today the reply hydrates *nothing at all* (U1), which makes this
        the ordinary path rather than the exotic one.

        Nothing runs on the retain branch — landing the session already focused
        is not an attach, and an approval that arrives with no ``request_id``
        gets a fresh synthesized one each time it is registered, so replaying a
        hydration would register a second card for one question.
        """
        session_id = durable_id or runtime_id
        at = self.state.last_observed_at

        # Read ownership BEFORE marking it, because marking destroys the answer.
        # A landing on a session this run already drives is a return, not a
        # steal: no transport was displaced, so there is no residue to latch and
        # saying otherwise names a client that never existed.
        #
        # The predicate is shared with OP2's dialog rather than restated here.
        # A first attempt asked only "do we already drive this row", which is
        # half the question: a row can be not-ours and no longer live at once,
        # and on that row — an ordinary historical resume — the dialog correctly
        # stayed away while the residue told the operator a client was
        # displaced. Two questions with one answer must not have two spellings.
        #
        # An earlier attempt also treated the prompts Talaria still holds as
        # cards that were never lost. True, but unreachable: prompts are only
        # registered while a session is focused, and a focused session is one
        # this run drives. It was dead weight that made the live guard
        # untestable — neither mutation failed a test while both were in.
        existing = fleet_row(self.fleet, profile=self.fleet_profile, session_id=session_id)
        displaced = existing is not None and attach_displaces_client(existing)

        self.fleet = mark_we_drive(
            self.fleet, profile=self.fleet_profile, session_id=session_id, at=at
        )
        events = activation_hydration_events(
            result, session_id=runtime_id, at=at, seq=self.state.entry_seq
        )
        for hydrated in events:
            self.state = apply_frame(self.state, hydrated)
        kinds = frozenset(
            PROMPT_EVENT_KINDS[event.type]
            for event in events
            if event.type in PROMPT_EVENT_KINDS
        )
        residue = ""
        if displaced:
            self.fleet, residue = latch_attach_residue(
                self.fleet,
                profile=self.fleet_profile,
                session_id=session_id,
                hydrated=kinds,
                at=at,
            )
        if residue:
            self._notice(residue)
            self.state = record_local_note(self.state, residue, at=at)

    def _report_startup_failure(self, outcome: RpcOutcome) -> None:
        """Put a failed session open in front of the operator, and in the log.

        Both, not either. The notice is what they see now; the transcript line
        is what survives the next notice overwriting the bar — and a launch that
        silently landed in no session at all is the state in which every
        subsequent action fails for a reason that is no longer on screen.
        """
        line = f"{SESSION_START_FAILED} {outcome.notice}"
        self._notice(line)
        self.state = record_local_note(self.state, line, at=self.state.last_observed_at)
        self._dirty = True

    async def load_catalog(self) -> CommandCatalog:
        """Read the gateway's slash inventory once, and never guess at it.

        A call that fails leaves an *unavailable* catalogue rather than an
        empty one, and the difference is the whole of AE9's honesty clause: an
        empty listing says the gateway offers no commands, which is a claim,
        while an unavailable one says Talaria could not read the listing, which
        is what happened. Both still carry the Talaria-local four, because
        those never needed the gateway — an operator whose gateway is refusing
        calls can still type ``/quit``.
        """
        dispatcher = self.dispatcher
        if dispatcher is None:  # pragma: no cover - guarded by every caller
            return unavailable_catalog("no gateway is attached")

        outcome = await dispatcher.call(CATALOG_METHOD, {}, timeout=self.call_timeout)
        if outcome.confirmed:
            catalog = decode_catalog(outcome.result)
        else:
            catalog = unavailable_catalog(outcome.notice)
        self.catalog = catalog
        # **A failed fetch does not take the notice bar.** This read is
        # background work nobody asked for, and the bar carries whatever the
        # operator's last action or the transport last said. Announcing here
        # overwrote the one line that was actionable: a run that could not find
        # a credential showed the credential chain's refusal (at the time, "set
        # HERMES_DASHBOARD_SESSION_TOKEN"; now the file and endpoint routes that
        # replaced it) and then, a moment later,
        # "commands.catalog was not sent — not connected", which
        # names a symptom of the first problem as though it were the problem.
        # The failure is still visible, in the listing, where an operator who
        # opens it is asking about exactly this.
        await self.render_catalog()
        return catalog

    async def render_catalog(self) -> None:
        """Push the held catalogue at the listing, unless the screen is gone.

        Tolerated for the same reason :meth:`_notice` tolerates it: this fetch
        is started at mount and can outlive the screen, and a ``NoMatches``
        raised from inside a torn-down tree surfaces as an unrelated-looking
        error at the end of an orderly exit (R36). Only the widget's absence is
        caught, so a genuine rendering failure still raises.
        """
        try:
            palette = self.palette
        except (NoMatches, ScreenStackError):  # pragma: no cover - teardown ordering
            return
        await palette.apply(self.catalog)

    def perform_local_command(self, invocation: LocalInvocation) -> bool:
        """Act on one command from Talaria's closed local command table.

        Returns whether the line was dispatched and should enter history.
        Refused submissions (pacing controls in live mode, malformed rate)
        return False and leave history unchanged — the local analogue of the
        replay-refused gateway commands that never reached dispatch.

        Dispatch is a table lookup on :class:`LocalCommand`'s ``action`` rather
        than a chain of name comparisons, for the same reason the gateway side
        is generic: the set is data, and a fifth control should be a row in
        that data rather than an edit here. ``/models`` *is* that row — and,
        exactly as predicted, adding it took an edit here too: a branch below,
        because selecting a model is the one control in this set that reaches
        the gateway (:meth:`select_model`), where every other one here stays
        entirely local.
        """
        command = invocation.command
        if command.replay_only and self._pacing_refused_live(command.name):
            # Refused through the same helper the function keys use, so the two
            # routes to one control cannot come to say different things about
            # it.
            return False

        if command.action == "quit":
            self.exit()
            return True
        if command.action == "models":
            # The one control here that is not synchronous end to end: opening
            # the picker renders instantly, but selecting a row dispatches
            # over the socket, and this method cannot be ``async`` (it is
            # called from :meth:`on_chat_text_area_submitted`, which the
            # framework requires be synchronous). Scheduled through
            # :meth:`_spawn_live`, the same escape :meth:`on_chat_text_area_submitted`
            # itself uses two paragraphs down for ``GatewayInvocation``.
            self._perform_models(invocation.argument)
            return True
        if command.action == "profiles":
            # Scheduled for the same reason ``models`` is: opening the region
            # is instant, but selecting a row drops a socket and dials another.
            self._perform_profiles(invocation.argument)
            return True
        if command.action == "sessions":
            # Scheduled for the same reason: the fetch and the switch both
            # cross the socket (U7, KTD6).
            self._perform_sessions(invocation.argument)
            return True
        if command.action == "needs":
            # Scheduled for the same reason ``sessions`` is: choosing a row can
            # dial a connection and resume a session, both across the socket.
            self._perform_needs(invocation.argument)
            return True
        if command.action == "agents":
            # A4: redundant typed path for the eaten F2. Scheduled because
            # toggle_collapsed is async (mounts/removes rows). Clears the
            # composer and dispatches locally, so on C1's rule it was
            # dispatched and should enter history (same as /models /
            # /profiles / /sessions) — return True.
            self.composer.clear()
            self._spawn_live(self._toggle_agents_and_discard())
            return True
        if command.action == "theme":
            self._perform_theme(invocation.argument)
            return True
        if command.action == "bar":
            self._perform_bar(invocation.argument)
            return True
        if command.action == "inspector":
            self.composer.clear()
            self.action_toggle_inspector()
            return True
        if command.action == "diffs":
            self.composer.clear()
            self._open_diffs()
            return True
        if command.action == "pause":
            self.controls.pause()
        elif command.action == "resume":
            self.controls.resume()
        else:
            speed = parse_speed(invocation.argument)
            if speed is None:
                self._notice(
                    f"{command.name} wants a rate: a positive multiplier, or 'max' "
                    f"— nothing changed"
                )
                return False
            self.controls.set_speed(speed)

        self.composer.clear()
        self._notice(self._pacing_notice())
        return True

    def _perform_bar(self, argument: str) -> None:
        """Show or toggle the session-only status-bar segment set."""
        self.composer.clear()
        words = argument.strip().split()
        if not words:
            visible = ", ".join(self.bottom_status_bar.settings.segments) or "none"
            self._notice(f"bar: visible segments: {visible}")
            return
        if len(words) != 1:
            self._notice("/bar takes one segment name — nothing changed")
            return

        segment = words[0]
        was_visible = segment in self.bottom_status_bar.settings.segments
        notice = self.bottom_status_bar.toggle_segment(segment)
        if notice is not None:
            self._notice(f"{notice} — nothing changed")
            return
        state = "hidden" if was_visible else "shown"
        self._notice(f"bar: {segment} {state} for this session")

    def _perform_theme(self, argument: str) -> None:
        """Open theme browsing or explicitly persist the current selection."""
        self.composer.clear()
        words = argument.strip().lower().split()
        if not words:
            self._spawn_live(self.open_theme_picker())
            return
        if words[0] != "save" or len(words) > 2:
            self._notice(
                f"/theme {argument.strip()!r} is not understood — "
                "try /theme, /theme save, or /theme save repository"
            )
            return
        scope: ThemeSaveScope = "user"
        if len(words) == 2:
            if words[1] not in ("user", "repository"):
                self._notice(
                    f"/theme save {words[1]!r} is not understood — "
                    "choose user or repository"
                )
                return
            scope = "repository" if words[1] == "repository" else "user"

        selected = self.session_theme_slug or self.configured_theme_slug
        try:
            path = save_theme(
                selected,
                scope,
                config_dir=self.theme_config_dir,
                cwd=self.launch_cwd,
            )
        except ConfigError as exc:
            self._notice(f"theme was not saved — {exc}")
            return
        self._notice(f"saved theme {selected!r} to the {scope} config at {path}")

    def _perform_models(self, argument: str) -> None:
        """Route ``/models``: no argument opens or closes it, one selects.

        A **third** shape joins the two PC6-era ones (U5): ``<n> default``
        writes the row as the connected profile's default model, and
        ``<n> default confirm`` is the second, textually distinct act KTD7's
        guard requires when the first came back ``confirm_required``. This is
        the one place that decides which of the three an operator typed, so
        it is also the one place that can guarantee ``confirm_expensive_model``
        is ``True`` only for a line that spells the word ``confirm`` — see
        :meth:`set_model_default` for how that guarantee is carried through.

        The composer is cleared immediately in every case — the argument, if
        any, was consumed by Talaria and never meant for the gateway, the same
        rule the other three PC6 controls follow.
        """
        self.composer.clear()
        stripped = argument.strip()
        if not stripped:
            self._spawn_live(self._open_picker_and_discard())
            return
        words = stripped.split()
        if len(words) == 1:
            self._spawn_live(self._select_model_and_discard(words[0]))
            return
        index_text, verb, *rest = words
        if verb.lower() != "default" or len(rest) > 1 or (
            rest and rest[0].lower() != MODEL_DEFAULT_CONFIRM_HINT
        ):
            self._notice(
                f"/models {stripped!r} is not understood — "
                f"try /models <n>, /models <n> default, or "
                f"/models <n> default {MODEL_DEFAULT_CONFIRM_HINT}"
            )
            return
        confirm = bool(rest)
        self._spawn_live(self._set_model_default_and_discard(index_text, confirm=confirm))

    async def _open_picker_and_discard(self) -> None:
        await self.action_toggle_picker()

    async def _toggle_agents_and_discard(self) -> None:
        await self.action_toggle_agents()

    def _perform_profiles(self, argument: str) -> None:
        """Route ``/profiles``: no argument opens the dialog, one switches."""
        self.composer.clear()
        stripped = argument.strip()
        if not stripped:
            self._spawn_live(self._open_profiles_and_discard())
            return
        self._spawn_live(self._switch_profile_and_discard(stripped))

    async def _open_profiles_and_discard(self) -> None:
        await self.action_toggle_profiles()

    async def _switch_profile_and_discard(self, argument: str) -> None:
        await self.select_profile(argument)

    async def select_profile(self, argument: str) -> SwitchReport | EnsureReport | None:
        """Resolve ``/profiles <n>`` into a connection to that profile's gateway.

        **What selecting a profile means changed in v0.4 (KTD1).** With a
        connection set present it *ensures*: the chosen profile's gateway is
        brought up beside the connections already open and becomes the home for
        the next create or resume, and **no other connection is touched**. The
        v0.3 behaviour — retarget the one socket, dropping whatever it was on —
        is kept only for a Talaria that holds exactly one connection, because
        in a fleet the connection a switch would drop is the sole feed for that
        gateway's sessions, and dropping it to look at another gateway loses
        exactly the sessions the operator is trying to keep an eye on.

        Every refusal below happens **before anything is dialled and before the
        current connection is touched**, which is the property that matters:
        the operator who picks a profile whose gateway is not running stays
        exactly where they were and is told why, rather than being disconnected
        into a wait for a machine that is not listening.

        The order is: nothing fetched; a listing belonging to a connection
        epoch that is no longer current (KTD4); an argument that is not a row
        number; a number naming no row; a row already current; a row whose
        gateway the *gateway itself* reports as not running; a row Talaria has
        no configured endpoint for; and a session with no way to switch at all.

        Past those, :meth:`~talaria.transport.source.LiveSource.switch_to_endpoint`
        re-resolves the credential for the new endpoint (KTD6) and dials. Its
        report is rendered rather than interpreted: a credential the new
        gateway's dashboard did not mint comes back as ``credential_unavailable``
        with its reason, and a switch that closed the old connection without
        making a new one says so and names the state Talaria is now in.

        ``POST /api/profiles/active`` is not called here or anywhere (KTD5).
        """
        directory = self.profiles
        if directory is None:
            self._notice(self.profiles_failure or PROFILES_NOT_FETCHED)
            return None
        if self._profiles_epoch != self._connection_epoch:
            self._notice(PROFILES_STALE_EPOCH)
            return None
        if not argument.isdigit():
            self._notice(f"/profiles wants a row number — {argument!r} is not one")
            return None
        index = int(argument)
        rows = flatten_profiles(
            directory, self.profile_endpoints, current=self.current_profile
        )
        row = next((r for r in rows if r.index == index), None)
        if row is None:
            self._notice(f"/profiles has no row {index}")
            return None
        if row.is_current and self.connections is None:
            self._notice(f"already connected to {row.name} — nothing to switch")
            return None
        if not row.dialable:
            # The listing already marked this row. Saying it again on selection
            # is deliberate: the marker is what the operator should have read,
            # and this is what they get for not having read it — a refusal that
            # costs nothing, rather than a dropped connection.
            self._notice(
                f"{row.name} cannot be dialled: {row.undialable_reason}; nothing changed"
            )
            return None
        connections = self.connections
        if connections is not None:
            return await self._ensure_profile(row.name, row.endpoint)

        switcher = self.switcher
        if switcher is None:
            self._notice(PROFILE_SWITCH_UNAVAILABLE)
            return None

        report = await switcher.switch_to_endpoint(row.endpoint)
        if report.ok:
            self._adopt_profile(row.name)
            if self.admin_factory is not None:
                # The admin surface follows the socket. See ``admin_factory``.
                self.admin_client = self.admin_factory(row.endpoint)
            self._notice(f"switched to {row.name}")
            return report
        if report.left_disconnected:
            self._notice(
                f"{PROFILE_SWITCH_FAILED} {report.reason} · {report.detail} "
                f"— the connection to the previous gateway is closed "
                f"(state: {report.state})"
            )
        else:
            self._notice(
                f"{PROFILE_SWITCH_FAILED} {report.reason} · {report.detail} "
                f"— still connected to the previous gateway"
            )
        return report

    async def _ensure_profile(self, name: str, endpoint: str) -> EnsureReport | None:
        """The fleet path for ``/profiles <n>``: bring one connection up (KTD1).

        Nothing here can disconnect anything. Every outcome is reported by
        name, and every one of them ends with the connections that were open
        still open — which is why the failure notices say what is *now* true of
        this profile rather than, as the drop-switch's do, where the operator
        has been left instead.
        """
        connections = self.connections
        if connections is None:  # pragma: no cover - guarded by the caller
            # The pragma is on this BRANCH and not on the function, and the
            # distinction is worth stating because a review of U7 read it the
            # other way. ``_ensure_profile`` itself is exercised by the picker
            # tests, which supply a ``connections`` double, and since U7's
            # composition root it is the production path for ``/profiles``:
            # ``build_live_app`` always assembles a ``ConnectionSet``, so
            # ``self.connections`` is never ``None`` in a live run and the
            # ensure-beside branch above is the one that fires. That the
            # production assembly HAS a set is pinned by
            # ``test_the_live_app_is_assembled_on_a_connection_set``; that a
            # selection then ensures rather than drop-switches is pinned by
            # ``test_selecting_the_profile_already_connected_makes_it_home_rather_than_refusing``
            # and ``test_a_failed_ensure_names_the_profile_and_disconnects_nothing``,
            # which U2 wrote for this semantic before anything could reach it.
            #
            # What stays uncovered is this guard, and it stays uncovered because
            # the sole caller tests the same attribute before dispatching here,
            # so no reachable state enters with ``None``. Writing a test that
            # calls this method directly to colour the line would test a
            # coincidence rather than a behaviour — the failure mode the
            # corrected mutation rule in the engineering journal names.
            self._notice(PROFILE_SWITCH_UNAVAILABLE)
            return None

        report = await connections.ensure(name)
        if report.ok:
            self._adopt_profile(name)
            if self.admin_factory is not None:
                # The admin surface follows the *home* connection, which is the
                # one a new session would be created on. See ``admin_factory``.
                self.admin_client = self.admin_factory(endpoint)
            self._notice(
                f"{name} is connected and is now the home for new sessions"
                if report.reason == "connected"
                else f"already connected to {name} — it is now the home for new sessions"
            )
            return report

        detail = f" · {report.detail}" if report.detail else ""
        self._notice(
            f"{PROFILE_CONNECT_FAILED} {name}: {report.reason}{detail} "
            "— every other connection is unchanged"
        )
        return report

    # ── U7: the session picker (KTD3, KTD6) ─────────────────────────────────

    def _perform_needs(self, argument: str) -> None:
        """Route ``/needs``: always opens the list; no argument shorthand (KTD7).

        Refused rather than ignored, for ``/sessions``'s reason and one of its
        own. The queue is *derived on every render* and ordered by wait age, so a
        row's position is not a name — an index typed a moment after a glance
        would select whatever had aged past it in between. There is nothing for
        an argument to mean here that would not be a misfire waiting to happen.
        """
        self.composer.clear()
        stripped = argument.strip()
        if stripped:
            self._notice(f"/needs takes no argument — {stripped!r} is not understood")
            return
        self.open_needs_picker()

    def open_needs_picker(self) -> None:
        """Put the needs-you list up, from the queue exactly as it now stands.

        Synchronous and it sends nothing, unlike ``/sessions``: the queue is
        derived from state Talaria already holds, so there is no fetch to make
        stale and no epoch to check on the way in. The staleness that does exist
        is checked on the way *out* instead — see :meth:`_needs_dismissed`.
        """
        queue = self.needs_you
        source = NeedsYouPickerSource(
            queue,
            self.state.last_observed_at,
            inline_answerable=self._inline_answerable(queue),
        )
        self.push_screen(PickerDialog(source), self._needs_dismissed())

    def _inline_answerable(self, queue: NeedsYouQueue) -> frozenset[tuple[str, str, str]]:
        """Which rows may be answered without leaving the list (R18, KTD9).

        **Two gates, and they answer different questions.** The queue decides
        answerable *in principle* — this is the session's head approval, it
        carries a request id, its connection is up — and that decision is the
        domain's so no widget can disagree with it. This method adds the second:
        the prompt has to still be live in the focused engine's registry, because
        that registry is what :meth:`respond_live` consults before it sends, and
        offering a row the answer path would refuse is a control that does
        nothing — the inert control AE11 exists to prevent.

        It is also the boundary that keeps a credential off this surface. Only
        ``approval`` is offered, so the value a row can carry is one the gateway
        named (``once``, ``deny``); ``secret`` and ``sudo`` are answered on their
        own card, where the input is masked, and reach the list only as something
        to navigate to (R22).
        """
        answerable: set[tuple[str, str, str]] = set()
        for item in queue.items:
            if item.kind != "approval" or not item.answerable:
                continue
            request_id = item.observed_request_id or item.request_key
            if not request_id:
                continue
            prompt = self.state.prompt_for(request_id, item.session_id)
            if prompt is None or prompt.kind != "approval":
                continue
            answerable.add(item.identity)
        return frozenset(answerable)

    def _needs_dismissed(self) -> Callable[[str | None], None]:
        """Resolve the chosen identity against the queue as it stands NOW.

        Deliberately not against the queue the dialog was opened with. An
        operator reading a list takes longer than an approval takes to be
        answered from somewhere else, and the queue is rebuilt on every render —
        so the item chosen may have resolved, expired, or had its connection drop
        while the list sat open. Re-deriving and looking the identity up again is
        what turns that from a misfire into a sentence.

        This is the same shape as ``/sessions``'s epoch re-check and exists for
        the same reason, with the difference that a queue item has no epoch to
        compare: its identity either still names something or it does not.
        """

        def dismissed(chosen: str | None) -> None:
            self.composer.focus()
            if chosen is None:
                return
            selection = decode_selection(chosen)
            item = (
                self.needs_you.item_for(selection.identity)
                if selection is not None
                else None
            )
            if selection is None or item is None:
                self._notice(ITEM_NO_LONGER_WAITING)
                return
            if selection.action == "go":
                self._spawn_live(self._go_to_item(item))
                return
            self._spawn_live(self._answer_item(item, selection))

        return dismissed

    async def _answer_item(self, item: QueueItem, selection: NeedsSelection) -> None:
        """Send one inline answer down the card path's own function (KTD9).

        :meth:`_respond_and_discard` is the wrapper both prompt cards already
        use, so the queue path inherits every guard the card path has rather
        than restating any of them: the registry correlation check, the
        expected-kind check, the clear-before-send rule, and R21's
        requested-with-age rendering that never optimistically clears.

        ``declined`` distinguishes the two acts in the transcript only. The wire
        value for a decline was decided in
        :func:`~talaria.ui.needs_you.answer_choices` from
        :func:`~talaria.ui.prompts.decline_value`, which is the one place that
        knows an approval's refusal is the explicit ``deny`` choice — an empty
        approval choice is read as *approved* by the gateway's consumer, so a
        decline that sent nothing would grant the command it looked like it
        refused.

        The request id is re-read off the item here rather than carried in the
        payload: the payload names *which item* and *what answer*, and the id is
        a property of the item as it stands now.
        """
        request_id = item.observed_request_id or item.request_key
        if not request_id:  # pragma: no cover - inline rows always carry one
            self._notice(ITEM_NO_LONGER_WAITING)
            return
        await self.respond_live(
            request_id,
            selection.value,
            declined=selection.action == "decline",
            expected_kind="approval",
            session_id=item.session_id,
        )

    async def _go_to_item(self, item: QueueItem) -> None:
        """Land the operator on the session this item belongs to (R17).

        **Three steps when the item is on another connection, and the order of
        the first two is the whole of CR7's navigation finding.** A session id
        names a session only on the gateway that issued it, so resuming a
        background connection's id against the home one asks the wrong gateway
        about a session it has never heard of. So: refuse early if the switch is
        already impossible, THEN ensure the profile — brought up beside the others
        and made home, exactly what ``/profiles`` does (KTD1) — and only then
        resume. Ensuring before the refusal check moved home in service of an act
        that was already going to be refused.
        Nothing is dropped either way. Pinned by
        ``test_choosing_an_item_on_another_connection_ensures_that_profile_first``.

        ``waiting_kind`` is carried through so U4's confirm can ask before taking
        a session that is live and not ours; the queue already knows what the
        session is waiting on, and re-deriving it at the landing would be a
        second answer to a question the item has already answered.
        """
        moved_home = ""
        if item.profile != self.fleet_profile:
            # **The refusal is checked BEFORE the ensure, and that ordering is the
            # fix rather than a tidy-up.** ``switch_session`` checks this same
            # predicate first thing and returns without sending; ensuring ahead of
            # it meant taking a side effect — home moves, and the next session is
            # created somewhere else — in service of an act that was already going
            # to be refused. The operator then read "the session was not switched"
            # and concluded nothing had happened, because the ensure's own notice
            # had been overwritten by the refusal's. Reproduced before fixing:
            # home went from "" to "beta-fixture" under that exact sentence.
            #
            # Pure and cheap, so checking it twice costs nothing and the second
            # check inside ``switch_session`` stays the authoritative one.
            refusal = fleet_switch_refusal(self.fleet)
            if refusal:
                self._notice(refusal)
                return
            endpoint = self.profile_endpoints.get(item.profile, "")
            report = await self._ensure_profile(item.profile, endpoint)
            if report is None or not report.ok:
                # ``_ensure_profile`` has already written the reason, which names
                # this profile and says every other connection is unchanged.
                # Adding a second notice here would overwrite the specific
                # sentence with a vaguer one.
                return
            moved_home = item.profile
        await self.switch_session(item.session_id, waiting_kind=item.kind)
        if moved_home and self.state.focused_session_id != item.session_id:
            # The residue the pre-check cannot remove: a refusal that only becomes
            # true during the round trip, or a resume the gateway declines. The
            # ensure has already moved home by then, so the operator is told —
            # once, in one sentence carrying both facts, because the alternative
            # is a true sentence about the session that lets them conclude a false
            # thing about the connection.
            self._notice(
                f"{self.composer.notice} — {moved_home} is now home for new sessions"
                if self.composer.notice
                else f"{moved_home} is now home for new sessions"
            )
            return
        self.prompts.focus_first_unanswered()
        # The plan's own sentence for this act: "landing with
        # `focus_first_unanswered` so the caret reaches the card". CR7 finding 7
        # showed the method had no production caller at all — seven tests asserted
        # a caret jump no operator could reach, and this navigation was about to
        # be the eighth. Without it the operator arrives at the right session and
        # the caret stays in the composer, so the answer they came to give needs a
        # tab press nothing on screen asks for.
        #
        # A no-op returning False when no card is mounted, which is the ordinary
        # outcome for a clarify or a terminal read that renders no control.
        self.prompts.focus_first_unanswered()

    def _perform_sessions(self, argument: str) -> None:
        """Route ``/sessions``: always opens the picker; no argument shorthand.

        Unlike ``/models``/``/profiles`` there is no ``<n>`` form — the picker
        never caches a listing for a typed index to resolve against (see
        :meth:`open_sessions_picker`) — so an argument is refused rather than
        silently ignored, the same AE9 honesty the other unrecognized-syntax
        refusals in this module follow.
        """
        self.composer.clear()
        stripped = argument.strip()
        if stripped:
            self._notice(f"/sessions takes no argument — {stripped!r} is not understood")
            return
        self._spawn_live(self._open_sessions_and_discard())

    async def _open_sessions_and_discard(self) -> None:
        await self.open_sessions_picker()

    async def open_sessions_picker(self) -> None:
        """Read both listings fresh, fold them into the registry, and put the
        modal picker up (R7, and v0.4's R3).

        **Fetched on every open, never cached.** ``/models`` and ``/profiles``
        hold their listing in app state so a reconnect-invalidated read can be
        caught by an epoch check at *selection* time (KTD4). A session listing
        has no such shorthand to protect — there is no ``/sessions <n>`` — so
        there is nothing to cache, and the epoch is instead checked once,
        right here, between issuing the call and opening the dialog: if the
        connection moved on while the reply was travelling, the listing
        answers for a gateway Talaria is no longer attached to and is refused
        rather than shown (:data:`SESSIONS_STALE_EPOCH`).

        **Refused before anything is sent, while an answer is on the wire**
        (:func:`~talaria.domain.state.fleet_switch_refusal`): a late outcome
        resolving after a switch would mutate the newly focused session's
        transcript, so the same guard :meth:`_land_session` applies to the
        *switch* is applied here to the *listing fetch* as well — an operator
        mid-answer gets one consistent refusal instead of a picker that opens
        and then cannot be used. The refusal is fleet-scoped as of v0.4: an
        answer travelling for a session that is not the focused one holds the
        window exactly as a focused one does (R6/KTD9).

        **The connection this open belongs to is captured once**, before either
        call goes out, and every fold and every row is keyed by it. Reading it
        again per step would let a profile switch landing mid-open file one
        gateway's listing and another's roster under two different keys.
        """
        dispatcher = self.dispatcher
        if dispatcher is None:
            self._notice(SESSIONS_UNAVAILABLE)
            return
        refusal = fleet_switch_refusal(self.fleet)
        if refusal:
            self._notice(refusal)
            return
        epoch = self._connection_epoch
        profile = self.fleet_profile
        outcome = await dispatcher.call(
            LIST_SESSIONS_METHOD, {"limit": SESSIONS_LIST_LIMIT}, timeout=self.call_timeout
        )
        if not outcome.confirmed:
            self._notice(f"{SESSIONS_LIST_FAILED} {outcome.notice}")
            return
        if epoch != self._connection_epoch:
            self._notice(SESSIONS_STALE_EPOCH)
            return
        directory = decode_session_list(outcome.result)
        roster_missing = await self._seed_registry(dispatcher, directory, profile=profile)
        if epoch != self._connection_epoch:
            # Re-checked after the roster call for the reason the first check
            # exists at all: the second round trip is a second window, and a
            # listing that answered for a gateway Talaria has left must not be
            # shown as this gateway's fleet.
            self._notice(SESSIONS_STALE_EPOCH)
            return
        rows = flatten_registry_sessions(
            self.fleet, profile=profile, current=self.state.session_key or ""
        )
        if not rows:
            self._notice(NO_SESSIONS)
            return
        # The line goes in the picker's own title as well as the composer.
        # A composer notice is written and then covered by the modal pushed on
        # top of it, so the named absence never actually reached the screen the
        # operator was looking at — a surface that names an absence nobody can
        # see has not named it.
        staleness = roster_staleness(self.fleet, profile)
        title = ""
        if roster_missing:
            line = ROSTER_UNAVAILABLE if staleness == "never" else ROSTER_STALE
            self._notice(line)
            title = line
        self.push_screen(
            PickerDialog(SessionPickerSource(rows, title=title)),
            self._sessions_dismissed(epoch),
        )

    async def _seed_registry(
        self, dispatcher: LiveDispatcher, directory: SessionDirectory, *, profile: str
    ) -> bool:
        """Fold both listing sweeps into the registry; report a missing roster.

        The two sweeps share one poll epoch, which is what arms the dual-listing
        retirement rule: a row absent from *both* successful sweeps of the same
        epoch may be dropped, and a row absent from one of them may not. A
        roster call that fails therefore advances no active epoch at all, so
        nothing is retired on the strength of a sweep that never happened —
        the rows simply keep saying what they last knew, and the caller names
        the absence (R10, R24).

        Ages come from ``last_observed_at`` — the frame clock — and never from
        a wall clock, so a replayed recording reproduces every age exactly
        (KTD12/R20).
        """
        at = self.state.last_observed_at
        self._poll_epoch += 1
        epoch = self._poll_epoch
        self.fleet = seed_from_listing(
            self.fleet, directory, profile=profile, at=at, poll_epoch=epoch
        )
        outcome = await dispatcher.call(
            LIST_ACTIVE_METHOD, {}, timeout=self.call_timeout
        )
        if not outcome.confirmed:
            return True
        self.fleet = apply_active_list(
            self.fleet,
            decode_active_list(outcome.result),
            profile=profile,
            at=at,
            poll_epoch=epoch,
            # Two clocks, deliberately. ``at`` is the frame clock and stamps
            # every age; this stamps only the poll schedule, which nothing reads
            # as an age. See ``apply_active_list``'s docstring for the
            # measurement that separated them.
            polled_at=_wall_clock(),
        )
        return False

    def _sessions_dismissed(self, epoch: int) -> Callable[[str | None], None]:
        """What happens when the session dialog closes — mirrors
        :meth:`_picker_dismissed`, kept separate because a chosen row here
        dispatches through :meth:`open_session` (KTD3) rather than through a
        model selection or a profile dial.

        **``epoch`` is the connection generation the listing was fetched
        under, carried in from :meth:`open_sessions_picker` (P1, U7 round
        two).** The epoch check there only covered the window up to the
        dialog opening — a reconnect while the dialog sat open (an operator
        reading titles takes longer than a socket drop) went unnoticed, and
        selecting an old row dispatched ``session.resume`` on the *new*
        connection, contradicting the plan's stale-selection refusal.
        Compared here, immediately before the dispatch, the same way
        :func:`~talaria.ui.app.TalariaApp._lookup_model_row` compares
        ``_model_catalog_epoch`` at *its* selection time (KTD4).

        **Checked again inside the scheduled task itself (C2).** This
        closure runs synchronously at dismissal time, but the switch it
        starts is handed to :meth:`_spawn_live`, which only *schedules* it
        via ``asyncio.create_task`` — the coroutine's body does not actually
        run until the event loop gets to it. A reconnect landing in that
        window is exactly as invisible to the check here as it was to the
        one in :meth:`open_sessions_picker`, for the identical reason: the
        epoch this closure compared against is already behind by the time
        the scheduled body executes. :meth:`_switch_session_and_discard`
        re-reads and compares it once more, right before the send, which is
        the only point that is actually synchronous with the dispatch.
        """

        def dismissed(chosen: str | None) -> None:
            self.composer.focus()
            if chosen is None:
                return
            if epoch != self._connection_epoch:
                self._notice(SESSIONS_STALE_EPOCH)
                return
            self._spawn_live(self._switch_session_and_discard(chosen, epoch))

        return dismissed

    async def _switch_session_and_discard(self, session_id: str, epoch: int) -> None:
        # Re-validated here, not only in the closure that scheduled this
        # task (C2): a reconnect between scheduling and this coroutine
        # actually running would otherwise dispatch ``session.resume`` on a
        # connection the listing was never fetched under.
        if epoch != self._connection_epoch:
            self._notice(SESSIONS_STALE_EPOCH)
            return
        await self.switch_session(session_id)

    async def switch_session(
        self, session_id: str, *, waiting_kind: str = ""
    ) -> RpcOutcome | None:
        """Resolve a chosen ``/sessions`` row into a switch — KTD3, exactly.

        No new landing code: ``StartupSelection(mode="session", session_id=…)``
        is the identical selection an explicit ``--session <id>`` resolves to
        at startup, so this is the same one call into :meth:`open_session`
        that path already makes — ``session.resume`` under the same
        :meth:`_landing` barrier (KTD2), landed by the same
        :meth:`_land_session`, which refuses the switch itself
        (``switch_refusal``) if an answer started travelling in the window
        since :meth:`open_sessions_picker` returned.

        **No serialization of its own (C1, U7 round three).** Choosing B,
        reopening ``/sessions`` before B's reply lands, and choosing C makes
        two calls here — :meth:`open_session` is what refuses the second
        one outright while the first is still on the wire; see its own
        docstring and :attr:`_resume_in_flight`.

        **Two gates in front of that call now (v0.4 U4).** The switch refusal is
        asked fleet-wide rather than of the focused engine alone, so an answer
        travelling for a background session occupies the window exactly as a
        focused one does (R6/KTD9) — and it is asked *here* as well as at the
        picker's open, because this is also the queue's entry point and a queue
        navigation never went past that open. Then OP2: a session the gateway
        reports live that this run did not open is confirmed before anything is
        sent, because focusing it detaches whichever client is driving it and
        that client is told nothing (KTD8, measured in U1).

        ``waiting_kind`` is what the caller knows the session is waiting on, and
        the only value that changes anything is ``"unobserved"`` — the queue's
        word for a wait the gateway flattened. It adds the sentence saying the
        kind is unknown and may not survive the attach, which is the difference
        between an operator confirming a steal and an operator confirming a
        steal that may recover nothing.

        Returns ``None`` when the confirmation is put up: the switch has not
        happened yet and may never, and answering ``None`` is the same answer
        every other refusal on this path gives. The dialog's own callback
        starts the switch if the operator takes it.
        """
        refusal = fleet_switch_refusal(self.fleet)
        if refusal:
            self._notice(refusal)
            return None
        row = attach_confirm_row(
            self.fleet, profile=self.fleet_profile, session_id=session_id
        )
        if row is not None:
            # The epoch is captured HERE, before the dialog goes up, and
            # re-compared after it answers. Everything else on this path
            # already did that; inserting a modal between the last comparison
            # and the dispatch reopened the window those comparisons exist to
            # close — and widened it to however long an operator takes to read
            # a warning about detaching somebody. On the profile-switch flavour
            # of a reconnect the consequence is worse than a stale id: the
            # decision was made against one gateway's rows and the resume would
            # go to another gateway entirely.
            self._confirm_attach(
                session_id,
                row,
                waiting_kind=waiting_kind,
                epoch=self._connection_epoch,
                liveness_stale=roster_staleness(self.fleet, self.fleet_profile) != "current",
            )
            return None
        return await self.open_session(
            StartupSelection(mode="session", session_id=session_id)
        )

    def _confirm_attach(
        self,
        session_id: str,
        row: RegistryRow,
        *,
        waiting_kind: str = "",
        epoch: int,
        liveness_stale: bool = False,
    ) -> None:
        """Put OP2's dialog up, and send nothing until it answers (KTD8).

        Fire-and-observe applies to the *decline* as much as to the confirm:
        backing out moves no focus, marks no row, and dispatches nothing —
        the only trace is the notice saying so.
        """
        kind = waiting_kind or row.waiting_kind
        body = [ATTACH_CONFIRM_BODY]
        if kind in ("unobserved", "") and row.status == "waiting":
            body.append(ATTACH_CONFIRM_UNKNOWN_KIND)
        if liveness_stale:
            body.append(ATTACH_CONFIRM_STALE_LIVENESS)
        # Clipped, like every other place a gateway-supplied title reaches a
        # bounded surface: the dialog is a fixed-size modal, and a title is a
        # string the gateway chose the length of.
        title = clip_detail_line(row.title.strip(), CONFIRM_TITLE_CLIP) or session_id
        dialog = ConfirmDialog(
            title=f"{ATTACH_CONFIRM_TITLE}: {title}", body=tuple(body)
        )

        def answered(confirmed: bool | None) -> None:
            self.composer.focus()
            if not confirmed:
                self._notice(ATTACH_DECLINED)
                return
            if epoch != self._connection_epoch:
                # The connection this decision was made against is gone. The
                # operator agreed to detach a client on *that* gateway; sending
                # the resume anyway would act on a different one.
                self._notice(SESSIONS_STALE_EPOCH)
                return
            self._spawn_live(self._attach_and_discard(session_id, epoch))

        self.push_screen(dialog, answered)

    async def _attach_and_discard(self, session_id: str, epoch: int) -> None:
        # Re-validated here as well as in the closure that scheduled it, for
        # the same reason ``_switch_session_and_discard`` is: a reconnect
        # between scheduling and running would otherwise slip through.
        if epoch != self._connection_epoch:
            self._notice(SESSIONS_STALE_EPOCH)
            return
        await self.open_session(StartupSelection(mode="session", session_id=session_id))

    def _lookup_model_row(self, argument: str) -> SelectableRow | None:
        """Resolve a ``/models`` row number into its catalogue row, or notice why not.

        Shared by :meth:`select_model` (U2) and :meth:`set_model_default`
        (U5): both start from the identical contract — nothing fetched yet;
        the list that *was* fetched belongs to a connection epoch that is no
        longer current (KTD4); the number does not name a listed row; the
        row's provider is not authenticated, which R7's marker already told
        the operator would fail before they typed it — and diverge only in
        what they do with the row once it resolves. Keeping the four checks
        in one place means they cannot drift apart between the two acts.
        """
        catalog = self.model_catalog
        if catalog is None:
            self._notice(MODELS_NOT_FETCHED)
            return None
        if self._model_catalog_epoch != self._connection_epoch:
            self._notice(MODELS_STALE_EPOCH)
            return None
        if not argument.isdigit():
            self._notice(f"/models wants a row number — {argument!r} is not one")
            return None
        index = int(argument)
        row = next((r for r in flatten_selectable(catalog) if r.index == index), None)
        if row is None:
            self._notice(f"/models has no row {index}")
            return None
        if not row.authenticated:
            self._notice(
                f"{row.provider_name} ({row.provider_slug}) is not authenticated — "
                "a model there is a guaranteed failure; nothing sent"
            )
            return None
        return row

    async def _select_model_and_discard(self, argument: str) -> None:
        await self.select_model(argument)

    async def select_model(self, argument: str) -> RpcOutcome | None:
        """Resolve ``/models <n>`` into ``/model <name> --provider <slug>``.

        ``argument`` is a 1-based row number, never a model name typed by
        hand — :func:`~talaria.ui.picker.flatten_selectable` is the one place
        that numbering is assigned, and :meth:`_lookup_model_row` looks a row
        up in it rather than re-deriving the number. Every case that method
        refuses sends nothing; every other case composes the same text an
        operator typing the working ``/model`` command by hand would submit,
        and sends it down the identical path (R2) — so what reaches the
        transcript is the gateway's own answer, not a paraphrase of it.
        """
        row = self._lookup_model_row(argument)
        if row is None:
            return None

        text = f"/model {row.model} --provider {row.provider_slug}"
        invocation = resolve_command(text, self.catalog)
        if isinstance(invocation, GatewayInvocation):
            if self.mode == "replay" or self.dispatcher is None:
                self._refuse_mutation(COMMAND_DISPATCH_CONTROL)
                return None
            session_id = self.state.focused_session_id or ""
            outcome = await self.dispatch_command_live(invocation)
            self._remember_switch(outcome, row, session_id)
            return outcome
        if isinstance(invocation, UnsupportedInvocation):
            self._refuse_unsupported(invocation)
            return None
        if isinstance(invocation, LocalInvocation):  # pragma: no cover - defensive
            # ``/model`` is not one of PC6's four, so the catalogue would have
            # to define a local command by that name for this branch to run —
            # not reachable through this module today, but resolved the same
            # way a typed line would be rather than left unhandled.
            self.perform_local_command(invocation)
            return None
        self._notice("could not build the /model command")  # pragma: no cover - defensive
        return None

    def _remember_switch(
        self, outcome: RpcOutcome | None, row: SelectableRow, session_id: str
    ) -> None:
        """Record the switch, but only on a reply the gateway actually sent.

        ``confirmed`` is the gate rather than "not an error" because
        :class:`~talaria.transport.rpc.RpcOutcome` has a third status and it is
        the one that matters here: ``unknown`` means the call went out and no
        answer came back, so whether the model changed is not known. Marking
        the row anyway would put a claim on screen that Talaria cannot support
        — the picker would say the operator is on a model they may well not be
        on, which is worse than the stale marker this method exists to fix.

        ``session_id`` is passed in rather than re-read because it is captured
        before the await. A reconnect landing mid-call moves the focus, and
        re-reading it here would file the switch under whichever session
        happened to be in focus when the reply arrived.
        """
        if outcome is None or not outcome.confirmed or not session_id:
            return
        self.session_model = SessionModel(
            session_id=session_id,
            provider_slug=row.provider_slug,
            model=row.model,
        )

    # ── U5: the default-model write, and its two-act confirmation ──────────

    async def _set_model_default_and_discard(self, argument: str, *, confirm: bool) -> None:
        await self.set_model_default(argument, confirm=confirm)

    async def set_model_default(
        self, argument: str, *, confirm: bool = False
    ) -> ModelAssignmentResult | None:
        """Resolve ``/models <n> default`` into a profile-scoped default write.

        Writes through ``POST /api/model/set?profile=<name>`` (KTD1) for
        ``self.current_profile`` — the profile this session already switched
        to (U4) — because "a selected profile" is that one, not a profile
        named on the command line the operator would have to spell correctly.
        A session that has never switched, or was never told which profile it
        started on, has no profile to scope the write to and the write is
        refused before anything is sent.

        ``argument`` resolves through :meth:`_lookup_model_row`, the identical
        row lookup :meth:`select_model` uses, so a row that would fail to
        *select* (unauthenticated, unfetched, a stale epoch) fails to become
        a default for the same reason before either reaches the socket.

        **KTD7's two-act rule.** ``confirm`` defaults to ``False`` and this
        method's *only* caller with ``confirm=True`` is the second, textually
        distinct command ``/models <n> default confirm`` — see
        :meth:`_perform_models`, the one place that decides which act an
        operator typed. The value is passed straight through to
        :meth:`~talaria.transport.admin.AdminClient.set_default_model`, whose
        own required keyword (no default) is the second, transport-level
        guarantee that a caller cannot let it default to ``True`` by omission.
        A response carrying ``confirm_required`` shows the gateway's message
        and instructs the operator to type the second act; nothing is resent
        automatically. R4's on-screen note — that this affects new sessions
        only, never the one running — is said on both the confirmation
        request and the eventual success, because Hermes's own docstring
        names getting that backwards as the mistake operators make.
        """
        row = self._lookup_model_row(argument)
        if row is None:
            return None
        if not self.current_profile:
            self._notice(MODEL_DEFAULT_NO_PROFILE)
            return None
        writer = self.admin_client
        if not isinstance(writer, ModelDefaultWriter):
            self._notice(MODEL_DEFAULT_UNAVAILABLE)
            return None

        try:
            result = await writer.set_default_model(
                profile=self.current_profile,
                provider=row.provider_slug,
                model=row.model,
                confirm_expensive_model=confirm,
            )
        except AdminError as exc:
            self._notice(f"{MODEL_DEFAULT_FAILED} {exc}")
            return None

        if result.confirm_required:
            self._notice(
                f"{result.confirm_message} — {MODEL_DEFAULT_NEW_SESSIONS_ONLY}; "
                f"type /models {row.index} default confirm to proceed"
            )
            return result
        if result.ok:
            self._notice(
                f"set {row.model} as {self.current_profile}'s default model — "
                f"{MODEL_DEFAULT_NEW_SESSIONS_ONLY}"
            )
        else:
            self._notice(
                f"{MODEL_DEFAULT_FAILED} the gateway did not confirm the write "
                f"for {self.current_profile}"
            )
        return result

    async def _dispatch_and_discard(self, invocation: GatewayInvocation) -> None:
        await self.dispatch_command_live(invocation)

    async def dispatch_command_live(
        self, invocation: GatewayInvocation, *, followed: frozenset[str] = frozenset()
    ) -> RpcOutcome | None:
        """Send one command and render whatever shape comes back (R23, R24).

        **Nothing here reads the command's name to decide what to do with the
        answer.** :func:`~talaria.domain.commands.render_dispatch` routes the
        three text destinations U3's decoder produced, so all six shapes — and
        any seventh — take one path. A branch per command name is the design
        this unit exists to avoid.

        **``slash.exec`` first, ``command.dispatch`` only if it refuses.** This
        is the ordering Hermes's own client uses
        (``ui-tui/src/app/createSlashHandler.ts:147-166`` at ``7f4d15515``), and
        the reason is not symmetry: ``command.dispatch``'s final line is
        ``_err(rid, 4018, "not a quick/plugin/bundle/skill command: …")``
        (``methods_tools.py:1070``), so most of the registry — every ordinary
        ``/model``, ``/status``, ``/context`` — is refused by it and served by
        the slash worker instead. Calling only ``command.dispatch`` would list
        those rows as dispatchable and fail every one on use, which is the
        listing lying about a much larger set than the client-local extras it
        already marks. The fallback is kept because the reverse is also true:
        ``slash.exec`` refuses a skill command outright (``:1146``) and needs
        the session that ``command.dispatch`` does not.

        The submit half is the subtle one. A ``skill`` or ``send`` result
        carries a ``message`` that is model-facing scaffolding, and it is sent
        to the gateway **without** being written into the transcript as the
        operator's line: R24's clause is that the scaffold is never rendered,
        and ``record_submission`` would render it. What the transcript gets is
        the display projection, which the gateway built for exactly this.
        """
        dispatcher = self.dispatcher
        if dispatcher is None:  # pragma: no cover - guarded by every caller
            return None

        session_id = self.state.focused_session_id or ""
        outcome = await dispatcher.call(
            SLASH_EXEC_METHOD,
            {"command": slash_exec_command(invocation), "session_id": session_id},
            timeout=self.call_timeout,
        )
        decoded: DispatchResult | UnknownDispatchResult | SlashOutput | None = None
        if outcome.confirmed:
            decoded = decode_slash_exec(outcome.result)
        else:
            outcome = await dispatcher.call(
                DISPATCH_METHOD,
                {
                    "name": invocation.wire_name,
                    "arg": invocation.argument,
                    "session_id": session_id,
                },
                timeout=self.call_timeout,
            )
            if outcome.confirmed:
                decoded = decode_dispatch_result(outcome.result)

        if decoded is None:
            # The composer keeps the command. Unlike a message, re-running a
            # slash command that may have already run is not a second copy of
            # anything the agent has to answer — and a command the operator
            # cannot see any more is one they cannot retype. The notice is the
            # *fallback's* refusal, not the first call's: that is the one that
            # says why the command did not run.
            line = f"{invocation.name}: {outcome.notice}"
            self.state = record_command_result(
                self.state, line, at=self.state.last_observed_at
            )
            self._notice(outcome.notice)
            self._dirty = True
            return outcome

        rendering = (
            render_slash_output(invocation.name, decoded)
            if isinstance(decoded, SlashOutput)
            else render_dispatch(invocation.name, decoded)
        )
        self.state = record_command_result(
            self.state, rendering.transcript_line, at=self.state.last_observed_at
        )
        self.composer.clear()
        if rendering.prefill_text is not None:
            self.composer.text = rendering.prefill_text
        unlisted = (
            "" if invocation.listed else " · the catalogue did not list this command"
        )
        self._notice(f"{rendering.notice}{unlisted}".strip(" ·"))
        self._dirty = True

        if rendering.submit_text:
            await self._submit_dispatch_payload(invocation.name, rendering.submit_text)
        if rendering.alias_target:
            await self._follow_alias(invocation, rendering.alias_target, followed)
        return outcome

    async def _follow_alias(
        self, invocation: GatewayInvocation, target: str, followed: frozenset[str]
    ) -> None:
        """Run what an alias points at, the way Hermes's own client does.

        An ``alias`` result names a target and runs nothing
        (``methods_tools.py:600``: the handler returns ``{"type": "alias",
        "target": qc.get("target", "")}``). Hermes's client re-dispatches it —
        ``return void handler(`/${d.target}${argTail}`)``
        (``createSlashHandler.ts:100-102``) — so a client that renders the
        target and stops has turned a working quick command into a dead end that
        looks like a result. The original argument goes with it, as it does
        there.

        Unlike that client this one carries the chain it has already followed,
        because a quick command aliased to itself is a configuration typo rather
        than an impossibility, and the official handler recurses on it without a
        bound.
        """
        name = target if target.startswith("/") else f"/{target}"
        chain = followed | {invocation.name.lower()}
        if name.lower() in chain or len(chain) >= ALIAS_FOLLOW_LIMIT:
            line = f"{invocation.name}: {ALIAS_CIRCULAR} {name}"
            self.state = record_command_result(
                self.state, line, at=self.state.last_observed_at
            )
            self._notice(line)
            self._dirty = True
            return

        next_invocation = resolve_command(
            f"{name} {invocation.argument}".strip(), self.catalog
        )
        if isinstance(next_invocation, LocalInvocation):
            self.perform_local_command(next_invocation)
            return
        if isinstance(next_invocation, UnsupportedInvocation):
            self._refuse_unsupported(next_invocation)
            return
        if isinstance(next_invocation, GatewayInvocation):
            await self.dispatch_command_live(next_invocation, followed=chain)
            return
        # The target was not a command line at all — an empty or malformed
        # ``target`` in the operator's quick-commands config. Named, not guessed
        # at, and nothing further is sent.
        line = f"{invocation.name}: the alias names no command"
        self.state = record_command_result(
            self.state, line, at=self.state.last_observed_at
        )
        self._notice(line)
        self._dirty = True

    def _refuse_unsupported(self, invocation: UnsupportedInvocation) -> None:
        """Say a catalogue entry has no dispatch path, durably (AE9).

        The refusal is written into the transcript as well as the notice bar
        because a gateway refusal is, and two honest refusals that differ only
        in how long they survive is a difference the operator has to learn. The
        notice bar is overwritten by the next thing that happens; the transcript
        is what an operator scrolls back through afterwards.
        """
        line = f"{invocation.name} is unsupported — {invocation.reason}"
        self.state = record_command_result(
            self.state, line, at=self.state.last_observed_at
        )
        self._notice(line)
        self._dirty = True

    async def _submit_dispatch_payload(self, name: str, text: str) -> None:
        """Send a result's ``message`` onward, and never render it.

        Deliberately not :meth:`submit_live`: that one writes the text into the
        transcript as the operator's own line, which for a skill or a bundle is
        several kilobytes of system-prompt fragment and is precisely what R24
        forbids. Only the outcome is written down, and only when it is not a
        clean delivery.
        """
        dispatcher = self.dispatcher
        if dispatcher is None:  # pragma: no cover - guarded by the caller
            return
        outcome = await dispatcher.call(
            SUBMIT_METHOD,
            {"session_id": self.state.focused_session_id or "", "text": text},
            timeout=self.call_timeout,
        )
        delivery = delivery_of(outcome)
        if outcome.status == "error" or delivery != "confirmed":
            note = outcome.notice or DELIVERY_NOTES.get(delivery) or ""
            self.state = record_local_note(
                self.state, f"{name}: {note}", at=self.state.last_observed_at
            )
            self._notice(note)
        self._dirty = True

    # ── B1: discarded-input notice replaces the caret row (R5', KTD1-KTD5) ─

    #: Which ancestor id maps to which region word for the discard notice.
    #: Transcript and agents are always no-text; prompts is no-text only when
    #: the focused widget is NOT inside a PromptCard (KTD3, KTD5). The ancestor
    #: walk itself survives from U3, re-interpreted from naming to classifying.
    _NO_TEXT_REGION_IDS: Final[Mapping[str, str]] = {
        "transcript": "transcript",
        "agents": "agents",
        "prompts": "prompts",
    }

    #: The composer notice shown when a printable key or paste would otherwise
    #: be silently discarded. Way-back clause leads so it survives truncation at
    #: 80 columns (KTD2). House register, full lowercase.
    _DISCARD_NOTICE_BY_REGION: Final[Mapping[str, str]] = {
        "transcript": (
            "press tab to return to the message box — "
            "typing is paused while the transcript holds the focus"
        ),
        "agents": (
            "press tab to return to the message box — "
            "typing is paused while the sub-agent list holds the focus"
        ),
        "prompts": (
            "press tab to return to the message box — "
            "typing is paused while the prompts region holds the focus"
        ),
    }

    def _no_text_region(self) -> str | None:
        """Which no-text region currently holds the caret, or None.

        Transcript and agents are always no-text. Prompts is answer-affordant
        only when the focused widget is inside a PromptCard; the container
        itself (PromptRegion#prompts) holds the caret and discards keys (KTD5).
        A focused card control classifies as not-no-text, so no notice fires
        there (KTD3).
        """
        focused = self.focused
        if focused is None:
            return None
        # Card controls are not discard regions — check before the id map so
        # a Button#choice-0 inside a PromptCard does not classify as prompts.
        for widget in focused.ancestors_with_self:
            if isinstance(widget, PromptCard):
                return None
        for widget in focused.ancestors_with_self:
            rid = widget.id or ""
            location = self._NO_TEXT_REGION_IDS.get(rid)
            if location is not None:
                return location
        return None

    def _clear_discard_latch_if_needed(self) -> None:
        """Clear the discard latch when the caret has left the announced region.

        Called from the focus handlers that already fire on every focus change
        (KTD4). Clears whenever the current region differs from the latched one,
        not only when the caret returns to the composer — the composer-free
        re-entry transcript → F1 → PromptRegion → transcript is reachable via
        shift+tab without ever focusing the composer (KTD2).

        Also clears the agents-toggle latch (A4 AE12) when the focused widget
        changes, so a second toggle in a new region re-announces.
        """
        try:
            current = self._no_text_region()
        except Exception:
            # Tearing down — no screen, no region. Clear both latches safely.
            self._discard_latch = ""
            self._agents_toggle_latch = ""
            return
        if self._discard_latch and current != self._discard_latch:
            self._discard_latch = ""
        if self._agents_toggle_latch:
            try:
                focused = self.focused
                focused_id = str(focused) if focused is not None else "no-focus"
            except Exception:
                focused_id = ""
            if focused_id != self._agents_toggle_latch:
                self._agents_toggle_latch = ""

    def on_descendant_focus(self, _event: events.DescendantFocus) -> None:
        self._clear_discard_latch_if_needed()
        self._sync_focus_indication()

    def on_descendant_blur(self, _event: events.DescendantBlur) -> None:
        self._clear_discard_latch_if_needed()
        self.call_after_refresh(self._sync_focus_indication)

    # ── the caret comes home ─────────────────────────────────────────────

    def on_caret_released(self, message: CaretReleased) -> None:
        """Put the caret back in the composer when a control is taken away.

        The composer is the answer for the same reason it is focused at mount:
        it is the only widget in the interface whose whole job is to accept
        typing, and it is what the operator is reaching for in every case that
        raises this. Textual's own answer — the enclosing scroll region — is a
        widget that takes the caret and then discards every printable key,
        which is indistinguishable on screen from the app having hung.

        See :mod:`talaria.ui.focus` for why the regions announce this rather
        than focusing the composer themselves: a widget that reaches across the
        tree for a sibling is a widget that cannot be mounted anywhere else.
        """
        message.stop()
        self.composer.text_area.focus()

    # ── U9: one sub-agent's interrupt, from its own row (R15, AE14) ──────

    def on_agent_row_interrupt(self, message: AgentRow.Interrupt) -> None:
        message.stop()
        if self.mode == "replay" or self.dispatcher is None:
            self._refuse_mutation("interrupt")
            return
        self._spawn_live(self._interrupt_subagent_and_discard(message.subagent_id))

    async def _interrupt_subagent_and_discard(self, subagent_id: str) -> None:
        await self.interrupt_subagent_live(subagent_id)

    async def interrupt_subagent_live(self, subagent_id: str) -> RpcOutcome | None:
        """Stop one delegated child, and leave the parent turn alone (AE14).

        **This must not do what :meth:`interrupt_live` does.** That one calls
        ``session.interrupt`` and, on success, marks the operator's own turn
        cancelled — which is sticky and suppresses every later delta. Applying
        it here would mean stopping one of six children silently swallowed the
        rest of the parent's reply. The two calls are one keystroke apart in
        the interface and the difference between them is the whole of this
        method.

        ``found: false`` is reported as what it is. The gateway answers that
        when the child has already finished (``methods_session.py:2806-2814``),
        and calling it an interrupt would record an act that never happened.
        """
        dispatcher = self.dispatcher
        if dispatcher is None:  # pragma: no cover - guarded by every caller
            return None

        outcome = await dispatcher.call(
            SUBAGENT_INTERRUPT_METHOD,
            {"subagent_id": subagent_id},
            timeout=self.call_timeout,
        )

        if not outcome.confirmed:
            line = outcome.notice
        else:
            body = outcome.result if isinstance(outcome.result, Mapping) else {}
            found = bool(body.get("found"))
            line = (
                f"{SUBAGENT_INTERRUPTED} {subagent_id}"
                if found
                else f"{SUBAGENT_NOT_FOUND} {subagent_id}"
            )
        self.state = record_local_note(self.state, line, at=self.state.last_observed_at)
        self._notice(line)
        self._dirty = True
        return outcome

    # ── U9: large-paste collapse (KTD16, AE13) ───────────────────────────

    def on_chat_text_area_large_paste(self, message: ChatTextArea.LargePaste) -> None:
        """A paste tripped the threshold. It is already in the editor.

        In replay the collapse is refused like every other gateway-needing
        control, and the refusal is the correct end state rather than a
        degraded one: the full text is in the composer, which is what KTD4
        specifies for a paste below the threshold anyway.
        """
        message.stop()
        if self.mode == "replay" or self.dispatcher is None:
            outcome = self.controls.attempt(PASTE_COLLAPSE_CONTROL)
            # The preservation clause leads, as it does on the live path. One
            # notice row is routinely narrower than the sentence, and the half
            # an operator needs is "your paste is still here" rather than the
            # name of the control that was refused — putting the refusal first
            # is what pushed the reassurance off the end of a 60-column row.
            self._paste_notice(f"{PASTE_NOT_COLLAPSED} — {outcome.notice}")
            return
        self._collapses_in_flight += 1
        self._spawn_live(self._collapse_paste_and_discard(message.text))

    async def _collapse_paste_and_discard(self, text: str) -> None:
        try:
            await self.collapse_paste_live(text)
        finally:
            # In a ``finally`` because a cancelled or failed round trip must
            # not leave the composer permanently refusing to submit.
            self._collapses_in_flight -= 1

    async def collapse_paste_live(self, text: str) -> RpcOutcome | None:
        """Spool a large paste to the gateway and swap in its placeholder.

        Every failure path ends in the same place, and that is the requirement
        rather than a convenience: the original text stays in the composer,
        editable, and nothing partial is submitted (AE13). There is no branch
        here that clears the editor, so no future edit can make one.
        """
        dispatcher = self.dispatcher
        if dispatcher is None:  # pragma: no cover - guarded by every caller
            return None

        outcome = await dispatcher.call(
            PASTE_COLLAPSE_METHOD, {"text": text}, timeout=self.call_timeout
        )
        collapsed = (
            decode_collapsed_paste(outcome.result) if outcome.confirmed else None
        )
        if collapsed is None:
            # Which sentence is said depends on reading the composer, because
            # only one of them is a claim about it. The operator can delete or
            # submit the paste while the round trip is out.
            kept = bool(text) and text in self.composer.text
            head = PASTE_NOT_COLLAPSED if kept else PASTE_COLLAPSE_REFUSED
            self._paste_notice(f"{head} — {outcome.notice}".strip(" —"))
            return outcome

        if not self.composer.collapse_paste(text, collapsed.placeholder):
            self._paste_notice(f"{PASTE_NO_LONGER_PRESENT} {collapsed.path}".strip())
            return outcome
        self._clear_paste_notice()
        return outcome

    def _paste_notice(self, message: str) -> None:
        """Write a paste-collapse notice, remembering that this feature wrote it."""
        self._last_paste_notice = message
        self._notice(message)

    def _clear_paste_notice(self) -> None:
        """Take the bar back only if this feature is still the one holding it.

        A successful collapse used to clear the notice row unconditionally, and
        the row is shared: a ``prompt.submit`` that was refused leaves "session
        is busy" there over a message still sitting undelivered in the composer.
        Collapsing an unrelated paste a moment later wiped the only sign that
        the message had not gone. A success needs no announcement of its own —
        the placeholder in the composer is the evidence — so the only thing to
        clear is this feature's own earlier failure line.
        """
        try:
            composer = self.composer
        except NoMatches:  # pragma: no cover - teardown ordering
            return
        if self._last_paste_notice and composer.notice == self._last_paste_notice:
            self._last_paste_notice = ""
            self._notice("")

    # ── scroll anchoring ─────────────────────────────────────────────────

    def on_paste(self, event: events.Paste) -> None:
        """Paste is typing's bigger sibling (KTD2, R5').

        A paste that reaches a no-text region is silently discarded the same
        way a printable key is — TranscriptPane and PromptRegion have no
        paste handler and the app defines no other. The same latch and the
        same notice apply; the paste is not rescued and focus does not move.
        """
        if not event.text:
            return
        region = self._no_text_region()
        if region is None:
            return
        if region == self._discard_latch:
            return
        self._notice(self._DISCARD_NOTICE_BY_REGION[region])
        self._discard_latch = region

    def on_key(self, event: events.Key) -> None:
        # Reading while scrolled away must survive streaming (R38). Page keys
        # reach the app because the transcript is not focused — the composer is
        # — so the anchor is released here rather than inside the pane.
        if event.key in ("pageup", "home"):
            self.transcript.hold_anchor()
        elif event.key == "end":
            # B3: the ``end`` key shares F5's follow rule (KTD2) — one method
            # carries it, or two renderings of "already following" drift.
            self.action_follow_bottom()
        # B1: a printable key that reached the app was not consumed by the
        # focused widget, so in a no-text region it is about to be silently
        # discarded. Announce once per focus-hold (KTD2), never move focus,
        # never re-dispatch the character (the triggering input is lost and
        # that loss is accepted).
        if event.is_printable:
            region = self._no_text_region()
            if region is not None and region != self._discard_latch:
                self._notice(self._DISCARD_NOTICE_BY_REGION[region])
                self._discard_latch = region

    # ── helpers used by the gate harness ─────────────────────────────────

    async def drain(self, *, timeout: float = 120.0) -> None:
        """Wait for the source to be exhausted and one final render to land."""
        await asyncio.wait_for(self.replay_complete.wait(), timeout=timeout)
        self._dirty = True
        await self._render_tick()

    def measurements(self) -> dict[str, Any]:
        elapsed = max(1e-9, time.monotonic() - self._started_at)
        return {
            "frames_applied": self.frames_applied,
            "render_ticks": self.render_ticks,
            "elapsed_seconds": elapsed,
            "render_ticks_per_second": self.render_ticks / elapsed,
            "peak_mounted_widgets": self.transcript.peak_mounted,
            "peak_descendant_widgets": self.transcript.peak_descendants,
            "condensed_lines": self.transcript.condensed_count,
        }
