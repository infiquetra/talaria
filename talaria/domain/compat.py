"""The checked-in compatibility baseline (KTD9, R34).

The terminal gateway publishes no capability endpoint — ``gateway.ready``'s
``change_events: true`` is hardcoded, so it detects nothing — which leaves two
ways to learn whether a required method exists. One is to call it, which for a
mutating method means creating a session or submitting a prompt to find out
whether you could have. R34 forbids that. The other is to record what the method
looked like at a pinned revision and compare, which is this module.

Every entry is classified:

* ``read-only`` — proved side-effect-free by reading its handler at the pin, so
  a startup check may invoke it *and read its answer*.
* ``evidence-only`` — mutating or request-scoped. Never probed. Its evidence is
  the pinned source line plus the request fixture and response shape below,
  exercised by isolated acceptance tests rather than by a live call.
* ``presence-only`` (U5) — the method may be called **bare**, with no parameters
  at all, purely to learn whether the gateway has a handler registered under
  that name. It is never called with parameters and its answer is never read for
  data. ``approval.pending`` is the whole class and the reason it exists: calling
  it with a real session id warms that session's agent build (``_sess`` waits on
  ``_wait_agent``, ``tui_gateway/server.py:2500``), so using it as a blind probe
  would keep the read-only discipline's letter and violate its spirit (KTD11).
  Called bare, ``_sess`` cannot find a session and refuses with JSON-RPC ``4001``
  *before* anything is warmed — and that refusal is itself the proof of presence
  (the parameter-invalid distinction, R11).

The response signature is deliberately **not** a JSON Schema. A hand-written
schema is a large artifact whose own correctness nobody checks, and nested
structure is where schema maintenance cost explodes. What is recorded is the
top-level key set plus the kind of each value, which is what an attach actually
breaks on: drift detection is then a set comparison naming the method and the
specific key.

U3 owns this data. U10 owns the check that runs against it. v0.4's U5 added the
third classification above and, below the baseline, the **seam catalogue** — the
capability boundaries an operator sees the consequences of, as distinct from the
method names a probe asks about.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass, replace
from typing import Any, Final, Literal

from talaria.domain.decode import is_reply_frame
from talaria.domain.redaction import redact_probe_detail

#: Every line reference in this module is read at this Hermes revision, unless
#: the entry names its own :attr:`MethodBaseline.pin`.
BASELINE_PIN = "7f4d15515"

#: The revision the running install's checkout sits at, and the one the next
#: restart of this machine's gateway processes brings up (U1's live verification,
#: ``docs/analysis/2026-08-17-v0-4-topology-verification.md``). Entries read at
#: this revision rather than at :data:`BASELINE_PIN` say so per entry — the
#: baseline is **not** wholesale re-derived here, because re-reading twenty
#: handlers at a second revision is a different unit's work and a silent
#: half-done re-pin is worse than an honest mixed one.
RUNNING_PIN = "7095e23eb"

Classification = Literal["read-only", "evidence-only", "presence-only"]

#: The value kinds a response signature can name. Unions are written with ``|``
#: in a stable order matching this tuple.
ValueKind = Literal["str", "int", "float", "bool", "list", "object", "null"]

_KIND_ORDER: tuple[str, ...] = ("str", "int", "float", "bool", "list", "object", "null")

#: Placeholder for a credential-shaped fixture field. A named constant rather
#: than a literal so nothing in this file ever *looks* like a stored secret —
#: the shape is what the baseline records; the value never matters.
_WITHHELD = ""


@dataclass(frozen=True)
class MethodBaseline:
    """One required gateway method, pinned.

    ``optional_keys`` exists because several handlers legitimately answer with
    two shapes — ``session.most_recent`` returns ``{"session_id": null}`` when no
    eligible session exists and the full record otherwise
    (``tui_gateway/methods_session.py:234``, ``:248-256``). Without the
    distinction, drift detection would flag the documented empty answer as a
    breaking change on every quiet machine.
    """

    method: str
    classification: Classification
    evidence: str
    purpose: str
    request_fixture: Mapping[str, Any]
    response_shape: Mapping[str, str]
    optional_keys: frozenset[str] = frozenset()
    #: The Hermes revision this entry's ``evidence`` was read at. Defaults to
    #: :data:`BASELINE_PIN`; an entry for a method that does not exist at that
    #: revision names :data:`RUNNING_PIN` instead, so a reader can tell which
    #: rows are old evidence and which are new without diffing two documents.
    pin: str = BASELINE_PIN
    #: Whether this method's absence blocks the **daily-driver verdict** (v0.1
    #: R34/AE7) as opposed to merely turning a named fleet feature off (v0.4
    #: R10).
    #:
    #: ``False`` for the two methods v0.4 added to the probe set, and the
    #: distinction is not a convenience. The daily-driver verdict answers "can
    #: this gateway run Talaria at all"; a gateway with no ``approval.pending``
    #: runs Talaria perfectly well and simply cannot show foreign approval
    #: detail — which is precisely the state U1 measured on this machine's wire,
    #: and precisely what KTD4 describes as "fleet features off by name while
    #: the single-session core keeps the old-pin baseline". Blocking on it would
    #: report every gateway at the old pin as incompatible forever, which would
    #: drain the flag of the meaning it has.
    blocks_daily_driver: bool = True


COMPAT_BASELINE: tuple[MethodBaseline, ...] = (
    # ── read-only: safe to invoke at startup ─────────────────────────────
    MethodBaseline(
        method="session.most_recent",
        classification="read-only",
        evidence="tui_gateway/methods_session.py:214-260",
        purpose="KTD7's --resume target: the most recent human-facing session.",
        request_fixture={},
        response_shape={
            "session_id": "null|str",
            "title": "str",
            "started_at": "float|int",
            "source": "str",
        },
        optional_keys=frozenset({"title", "started_at", "source"}),
    ),
    MethodBaseline(
        method="spawn_tree.list",
        classification="read-only",
        evidence="tui_gateway/methods_session.py:2860-2908",
        purpose="Finished sub-agent fan-outs for the delegation view (R14).",
        request_fixture={"session_id": "", "limit": 50},
        response_shape={"entries": "list"},
    ),
    MethodBaseline(
        method="agents.list",
        classification="read-only",
        evidence="tui_gateway/methods_tools.py:1594-1616",
        purpose="Registered agent processes — a read of who is running (R17).",
        request_fixture={},
        response_shape={"processes": "list"},
    ),
    MethodBaseline(
        method="delegation.status",
        classification="read-only",
        evidence="tui_gateway/methods_session.py:2778-2795",
        purpose="Live sub-agent roster plus the configured depth/concurrency caps.",
        request_fixture={},
        response_shape={
            "active": "list",
            "paused": "bool",
            "max_spawn_depth": "int|null",
            "max_concurrent_children": "int|null",
        },
    ),
    MethodBaseline(
        method="commands.catalog",
        classification="read-only",
        evidence="tui_gateway/methods_tools.py:255-367",
        purpose="The slash-command inventory U9 dispatches from (R23).",
        request_fixture={},
        response_shape={
            "pairs": "list",
            "sub": "object",
            "canon": "object",
            "categories": "list",
            "skills": "object",
            "skill_count": "int",
            "warning": "str",
            "commands": "object",
        },
        # U1 (#119) observed the live gateway answering with a ``commands``
        # metadata map alongside ``pairs`` — a distinct per-command map, not a
        # second spelling of the rows. Pinned when present (wrong-typed still
        # drifts), tolerated when absent so gateways predating the key do not
        # report a missing-key break they never had.
        optional_keys=frozenset({"commands"}),
    ),
    MethodBaseline(
        method="session.list",
        classification="read-only",
        evidence="tui_gateway/methods_session.py:162-208",
        purpose="The /sessions picker's listing (R7, R10, U7).",
        # The handler's own default (``params.get("limit", 200)``,
        # ``methods_session.py:181``), sent explicitly rather than omitted —
        # a startup probe should not depend on a server-side default it never
        # names.
        request_fixture={"limit": 200},
        # Top-level only, per this module's own scope note above: the reply is
        # ``{"sessions": [{id, title, preview, started_at, message_count,
        # source}, ...]}`` (``methods_session.py:195-207``). The row shape is
        # not pinned here — it lives in the decoder contract test instead
        # (``talaria/domain/session_list.py``, mirroring how U6 keeps
        # ``session.resume``'s ``messages`` element shapes out of this file).
        response_shape={"sessions": "list"},
    ),
    MethodBaseline(
        method="session.active_list",
        classification="read-only",
        evidence="tui_gateway/methods_session.py:986",
        purpose=(
            "The v0.4 registry's liveness feed: every live session of the "
            "gateway process, with the lifecycle word session.list has no field "
            "for (KTD2)."
        ),
        # **Promoted from evidence-only to read-only by U5, which is the whole
        # of the change.** U4 classified it evidence-only and said why in so
        # many words: the method is a pure in-memory snapshot and safe to probe,
        # but adding a startup probe was U5's work. This is U5. The bare call
        # takes no parameters, reads a dict the gateway already holds, and warms
        # nothing — the handler never goes through ``_sess``.
        #
        # **Still not gated behind a version check, and the promotion does not
        # change that.** KTD4 names this method as one the pin above does not
        # have; U1's live enumeration found it registered at 7f4d15515, at the
        # checkout revision 7095e23eb, and on the wire this machine serves
        # (91a545ab1) — the delta from the old pin is 24 methods added and none
        # removed, and this is not among them
        # (``docs/analysis/2026-08-17-v0-4-topology-verification.md``,
        # "A correction to the plan's KTD4"). Only approval.pending is genuinely
        # new. A version gate here would refuse a roster from gateways that have
        # always had one; probing an always-present method simply always
        # succeeds, and the "roster unavailable" branch stays as defence against
        # gateways Talaria has not met.
        request_fixture={},
        # Top-level only, per this module's scope note: the row shape
        # ({current, id, last_active, message_count, model, preview,
        # session_key, started_at, status, title}, verified live 2026-08-17)
        # is pinned by the decoder contract in talaria/domain/session_list.py.
        response_shape={"sessions": "list"},
        blocks_daily_driver=False,
    ),
    # ── presence-only: called bare, never with parameters ────────────────
    MethodBaseline(
        method="approval.pending",
        classification="presence-only",
        evidence="tui_gateway/methods_prompt.py:1448-1458",
        purpose=(
            "Foreign approval detail for the needs-you queue: the pending "
            "approvals of a session Talaria does not drive (KTD11)."
        ),
        # The one entry read at RUNNING_PIN rather than BASELINE_PIN, because
        # it does not exist at BASELINE_PIN at all — and it does not exist on
        # this machine's *wire* either. U1 verified live that the serving
        # process (91a545ab1) answers `-32601 unknown method: approval.pending`
        # while the checkout (7095e23eb) registers it. "Roster available,
        # approval detail unavailable by name" is therefore a state Talaria
        # reaches today, not a defensive fiction — which is exactly why this is
        # probed rather than assumed.
        pin=RUNNING_PIN,
        # Bare, and it must stay bare. `_sess` resolves `params["session_id"]`
        # and then waits on the session's agent build; a probe carrying a real
        # id would warm a build to find out whether a method exists. With no
        # id, `_sessions.get("")` misses and the handler refuses 4001 before
        # touching anything (`tui_gateway/server.py:2496-2503`).
        request_fixture={},
        # Never compared against a live answer, because a bare call never
        # produces one — presence is proved by the *refusal*. Recorded so the
        # shape is written down somewhere for the unit that reads the data.
        response_shape={"approvals": "list"},
        blocks_daily_driver=False,
    ),
    # ── evidence-only: never probed ──────────────────────────────────────
    MethodBaseline(
        method="session.create",
        classification="evidence-only",
        evidence="tui_gateway/methods_session.py:14-158",
        purpose="KTD7's default-new startup path (R2).",
        request_fixture={"cols": 80},
        response_shape={
            "session_id": "str",
            "stored_session_id": "str",
            "message_count": "int",
            "messages": "list",
            "info": "object",
        },
    ),
    MethodBaseline(
        method="session.resume",
        classification="evidence-only",
        evidence="tui_gateway/methods_session.py:306-699",
        purpose="KTD7's --resume and --session startup paths (R2).",
        request_fixture={"session_id": "", "cols": 80},
        response_shape={
            "session_id": "str",
            "resumed": "str",
            "message_count": "int",
            "messages": "list",
            "info": "object",
            # Recorded from a live reply on 2026-08-07, not read out of the
            # source: the reply-side pass over the recording corpus reported it
            # as an unexpected key. All three of ``session.resume``'s success
            # paths set it (``methods_session.py:466``, ``:551``, ``:712``), so
            # it is required rather than optional. Talaria reads neither this
            # key nor ``messages`` — a resume renders an empty transcript
            # however much history the reply carries, which is queued as P1.
            "messages_omitted": "bool",
            "inflight": "null|object",
            "running": "bool",
            "session_key": "str",
            "started_at": "float|int",
            "status": "str",
        },
        optional_keys=frozenset({"auto_continue"}),
    ),
    MethodBaseline(
        method="prompt.submit",
        classification="evidence-only",
        evidence="tui_gateway/methods_prompt.py:67-313",
        purpose="Submitting a turn (R3).",
        request_fixture={"session_id": "", "text": ""},
        response_shape={"status": "str"},
    ),
    MethodBaseline(
        method="session.interrupt",
        classification="evidence-only",
        evidence="tui_gateway/methods_session.py:2706-2775",
        purpose="Cancelling an in-flight turn (R4).",
        request_fixture={"session_id": ""},
        response_shape={"status": "str"},
        optional_keys=frozenset({"turn_isolation"}),
    ),
    MethodBaseline(
        method="subagent.interrupt",
        classification="evidence-only",
        evidence="tui_gateway/methods_session.py:2806-2814",
        purpose="Interrupting one delegated child (R15).",
        request_fixture={"subagent_id": ""},
        response_shape={"found": "bool", "subagent_id": "str"},
    ),
    # ``slash.exec`` and ``command.dispatch`` are one path, not two. An ordinary
    # catalogue command goes out over ``slash.exec``, and only a command that
    # handler refuses falls back to ``command.dispatch``
    # (``talaria/domain/commands.py:54-66``). Both have to be pinned: a client
    # that verified only the fallback would report daily-driver ready and then
    # fail on the first command an operator typed, which is the exact failure
    # R34 exists to prevent.
    #
    # The reply is an untagged union. The slash worker answers ``{"output": …}``,
    # adding ``warning`` only when running the command had a side effect the
    # gateway had to mirror into the session (``methods_tools.py:1198-1203``). A
    # skill, a bundle, or a pending-input command is instead forwarded to
    # ``command.dispatch`` and *that* payload is returned verbatim (``:1102-1140``),
    # so the union's second arm is pinned by the entry below rather than
    # duplicated here. Talaria discriminates on a string ``type``, the same test
    # Hermes's own client uses (``talaria/domain/commands.py:644-667``).
    MethodBaseline(
        method="slash.exec",
        classification="evidence-only",
        evidence="tui_gateway/methods_tools.py:1073-1211",
        purpose="The route an ordinary catalogue command actually takes (R23, R24).",
        request_fixture={"session_id": "", "command": "help"},
        response_shape={"output": "str", "warning": "str"},
        optional_keys=frozenset({"warning"}),
    ),
    MethodBaseline(
        method="command.dispatch",
        classification="evidence-only",
        evidence="tui_gateway/methods_tools.py:432-1071",
        purpose="The fallback slash route and its six result shapes (R23, R24).",
        request_fixture={"name": "help", "arg": "", "session_id": ""},
        response_shape={"type": "str"},
        optional_keys=frozenset(
            {"output", "target", "message", "display", "notice", "name"}
        ),
    ),
    MethodBaseline(
        method="paste.collapse",
        classification="evidence-only",
        evidence="tui_gateway/methods_complete.py:14-39",
        purpose="Collapsing a large paste to a placeholder (R13, KTD16).",
        request_fixture={"text": ""},
        response_shape={"placeholder": "str", "path": "str", "lines": "int"},
    ),
    MethodBaseline(
        method="approval.respond",
        classification="evidence-only",
        evidence="tui_gateway/methods_prompt.py:886-920, tools/approval.py:2490-2505",
        purpose="Answering a dangerous-command approval (R7).",
        request_fixture={"session_id": "", "choice": "deny", "all": False},
        # ``int``, and it was pinned as ``bool`` until 2026-08-07. The handler
        # returns ``resolve_gateway_approval(...)`` verbatim, and that function
        # is typed ``-> int`` and documented "Returns the number of approvals
        # resolved (0 means nothing was pending)". Three live replies carried
        # ``0``, ``1`` and ``0`` — JSON integers, never ``true``/``false``.
        # ``talaria/ui/app.py:527-529`` already read it as a count; only this
        # record was wrong, which is why nothing misbehaved and nothing caught
        # it until the replies were compared against the baseline.
        response_shape={"resolved": "int"},
    ),
    MethodBaseline(
        method="clarify.respond",
        classification="evidence-only",
        evidence="tui_gateway/methods_prompt.py:858-864, tui_gateway/server.py:10228-10239",
        purpose="Answering a clarification bridge (R7, R8).",
        request_fixture={"request_id": "", "answer": _WITHHELD},
        response_shape={"status": "str"},
    ),
    MethodBaseline(
        method="secret.respond",
        classification="evidence-only",
        evidence="tui_gateway/methods_prompt.py:881-883, tui_gateway/server.py:10228-10239",
        purpose="Answering a secret bridge. Response value is deny-set redacted (R9).",
        request_fixture={"request_id": "", "value": _WITHHELD},
        response_shape={"status": "str"},
    ),
    MethodBaseline(
        method="sudo.respond",
        classification="evidence-only",
        evidence="tui_gateway/methods_prompt.py:876-878, tui_gateway/server.py:10228-10239",
        purpose="Answering a sudo bridge. Response value is deny-set redacted (R9).",
        request_fixture={"request_id": "", "password": _WITHHELD},
        response_shape={"status": "str"},
    ),
    MethodBaseline(
        method="terminal.read.respond",
        classification="evidence-only",
        evidence="tui_gateway/methods_prompt.py:867-873, tui_gateway/server.py:10228-10239",
        purpose="Answering the terminal-read bridge with the projection (KTD10).",
        request_fixture={"request_id": "", "text": _WITHHELD},
        response_shape={"status": "str"},
    ),
)

#: Methods a startup probe may call **with their pinned fixture**, and whose
#: answer it may compare against the recorded signature. This is not the whole
#: probe set any more — see :data:`PROBEABLE_METHODS`.
READ_ONLY_METHODS: frozenset[str] = frozenset(
    entry.method for entry in COMPAT_BASELINE if entry.classification == "read-only"
)

EVIDENCE_ONLY_METHODS: frozenset[str] = frozenset(
    entry.method for entry in COMPAT_BASELINE if entry.classification == "evidence-only"
)

#: Methods a startup probe may call **bare and only bare** (U5, KTD11). Separate
#: from :data:`READ_ONLY_METHODS` because the permission is narrower: a read-only
#: method may be called with its pinned fixture and have its answer compared,
#: while one of these may be called with nothing at all and only its *refusal*
#: read.
PRESENCE_ONLY_METHODS: frozenset[str] = frozenset(
    entry.method for entry in COMPAT_BASELINE if entry.classification == "presence-only"
)

#: Everything a startup probe is permitted to put on the wire. Anything outside
#: this set is evidence-only and must never be called to discover whether it
#: exists (R34, AE7).
PROBEABLE_METHODS: frozenset[str] = READ_ONLY_METHODS | PRESENCE_ONLY_METHODS

REQUIRED_METHODS: frozenset[str] = (
    READ_ONLY_METHODS | EVIDENCE_ONLY_METHODS | PRESENCE_ONLY_METHODS
)

#: Methods that write sub-agent state. R17 says Talaria reads sub-agent state and
#: never authors it, and this is the concrete list that rule excludes — most
#: pointedly ``spawn_tree.save``, which Hermes's own turn controller calls at the
#: end of every delegating turn (``turnController.ts:640-652``) to archive the
#: fan-out to disk. Talaria does not build that archive.
SUBAGENT_AUTHORING_METHODS: frozenset[str] = frozenset(
    {"spawn_tree.save", "delegation.pause", "delegate_task"}
)


@dataclass(frozen=True)
class ShapeDrift:
    """One named difference between the baseline and an observed response."""

    method: str
    key: str
    kind: Literal["missing-key", "unexpected-key", "value-kind-changed"]
    expected: str | None
    observed: str | None

    def describe(self) -> str:
        if self.kind == "missing-key":
            return f"{self.method}: response no longer carries '{self.key}'"
        if self.kind == "unexpected-key":
            return f"{self.method}: response carries an unrecorded key '{self.key}'"
        return (
            f"{self.method}: '{self.key}' changed kind from "
            f"{self.expected} to {self.observed}"
        )


def value_kind(value: Any) -> str:
    """Name the kind of one JSON value.

    ``bool`` is tested before ``int`` because it is an ``int`` subclass in
    Python, and reporting ``paused: true`` as an integer would make a real drift
    invisible.
    """
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "bool"
    if isinstance(value, int):
        return "int"
    if isinstance(value, float):
        return "float"
    if isinstance(value, str):
        return "str"
    if isinstance(value, list):
        return "list"
    if isinstance(value, dict):
        return "object"
    return "object"


def shape_of(payload: Mapping[str, Any]) -> dict[str, str]:
    """The top-level key set plus each value's kind — the whole signature."""
    return {key: value_kind(value) for key, value in payload.items()}


def _kinds(signature: str) -> frozenset[str]:
    return frozenset(signature.split("|"))


def normalize_kind_union(signature: str) -> str:
    """Canonical ordering for a union, so ``int|null`` and ``null|int`` compare."""
    present = _kinds(signature)
    ordered = [kind for kind in _KIND_ORDER if kind in present]
    return "|".join(ordered) or signature


def compare_shape(
    entry: MethodBaseline, observed: Mapping[str, Any]
) -> tuple[ShapeDrift, ...]:
    """Compare an observed response against the pinned signature.

    Nested structure is deliberately out of scope for v0.1. Top-level drift is
    what actually breaks an attach; a missing key three levels down usually
    degrades one panel, and chasing it costs more maintenance than it saves.
    """
    observed_shape = shape_of(observed)
    drifts: list[ShapeDrift] = []

    for key, expected in entry.response_shape.items():
        if key not in observed_shape:
            if key in entry.optional_keys:
                continue
            drifts.append(
                ShapeDrift(
                    method=entry.method,
                    key=key,
                    kind="missing-key",
                    expected=normalize_kind_union(expected),
                    observed=None,
                )
            )
            continue
        actual = observed_shape[key]
        if actual not in _kinds(expected):
            drifts.append(
                ShapeDrift(
                    method=entry.method,
                    key=key,
                    kind="value-kind-changed",
                    expected=normalize_kind_union(expected),
                    observed=actual,
                )
            )

    for key in observed_shape:
        if key in entry.response_shape or key in entry.optional_keys:
            continue
        drifts.append(
            ShapeDrift(
                method=entry.method,
                key=key,
                kind="unexpected-key",
                expected=None,
                observed=observed_shape[key],
            )
        )

    return tuple(drifts)


def baseline_for(method: str) -> MethodBaseline:
    for entry in COMPAT_BASELINE:
        if entry.method == method:
            return entry
    raise KeyError(f"{method} is not in the compatibility baseline")


# ══ U5: seams — named presence, absence, and degradation per connection ══
#
# A *seam* is a capability boundary an operator can see the consequences of, not
# a method name. The four below are the whole set v0.4 has, and each one names
# the feature that goes off when it is absent (R10): a surface that renders an
# empty list because a data source is missing is exactly the failure R10 forbids,
# so absence is a sentence, never a zero.
#
# One seam is deliberately unprobeable. ``kanban-dispatcher`` has no route at the
# running revision, so it renders never-observed forever and **no probe URL is
# invented for it** — inventing one and reading its 404 as absence is the same
# `absent_capability` error class R11 exists to close, committed one layer
# further out.


#: The JSON-RPC dispatcher's unregistered-name code, as it appears in a recorded
#: reply. Restated here rather than imported from
#: :mod:`talaria.transport.compat_check`, because the domain does not import the
#: transport and this is one integer fixed by the JSON-RPC specification.
#: ``tests/domain/test_probe_replay.py`` asserts the two constants agree, so the
#: duplication cannot drift in silence.
RECORDED_METHOD_NOT_FOUND: Final[int] = -32601

#: How often a live probe story is re-established (R12). Five minutes: long
#: enough that it is nothing like a per-render cost, short enough that "the
#: gateway lost its admin surface" is not news an operator learns tomorrow.
#: Nothing in this module reads a clock or owns a timer — the app does, exactly
#: as it does for :data:`~talaria.domain.registry.POLL_BACKSTOP_S`.
PROBE_REVALIDATION_S: Final[float] = 300.0

#: What a probe concluded about one seam. ``None`` — never-observed — is not in
#: this union on purpose: it is the *absence* of a conclusion and is represented
#: by :attr:`SeamObservation.status` being ``None``, so no code path can mistake
#: "nothing was ever probed" for a fifth verdict about a probe that ran.
#:
#: * ``present``           — the seam answered and the capability is available.
#: * ``absent``            — the gateway has no such method or route. Claimed
#:   only after a bare re-ask when the failing request carried parameters (R11).
#: * ``incompatible``      — the seam answered, in a shape the pin does not
#:   record. Present, unusable as pinned.
#: * ``degraded``          — the seam was known and its latest probe could not
#:   settle it (no answer, transport gone). The last confirmation's age is
#:   carried and rendered; the old verdict is never re-presented as current.
#: * ``parameter-invalid`` — a handler answered and refused this probe's own
#:   request. Presence proved, capability unproved. This is the classification
#:   the recorded ``absent_capability`` misdiagnosis should have produced.
SeamStatus = Literal["present", "absent", "incompatible", "degraded", "parameter-invalid"]

#: R24's three display classes, for seams. Identical vocabulary to
#: :data:`~talaria.domain.registry.ObservationClass` and deliberately not
#: imported from it: that one is defined over a session row's observation, this
#: one over a probe's age, and a shared alias would suggest one rule governs
#: both.
SeamObservationClass = Literal["live", "stale", "never-observed"]

#: Why a probe ran. There is no ``render`` member, and that is the enforcement:
#: :func:`seam_probe_due` takes a trigger, so a render path has nothing to pass
#: (R12 — "never per-render").
ProbeTrigger = Literal["attach", "reconnect", "revalidation"]


@dataclass(frozen=True)
class Seam:
    """One capability boundary, with the feature its absence disables.

    ``method`` and ``http_path`` are the two probe shapes v0.4 has. A seam with
    neither is unprobeable by construction — see :attr:`probeable` — which is
    how ``kanban-dispatcher`` is prevented from acquiring an invented URL by a
    later well-meaning edit: there is no field to put one in that would be read.
    """

    name: str
    #: What the seam provides, in plain words, for the present line.
    capability: str
    #: The named absence (R10). A whole clause, because "off" with no name is
    #: what an empty board with no dispatcher already looks like.
    disabled_feature: str
    #: What to say when nothing has ever been probed (R24). Never a count.
    unobserved_feature: str
    method: str | None = None
    http_path: str | None = None

    @property
    def probeable(self) -> bool:
        return self.method is not None or self.http_path is not None


SEAM_CATALOGUE: tuple[Seam, ...] = (
    Seam(
        name="roster",
        capability="fleet roster",
        disabled_feature="roster unavailable: session.active_list absent",
        unobserved_feature="roster not observed: no probe has answered yet",
        method="session.active_list",
    ),
    Seam(
        name="approval-detail",
        capability="foreign approval detail",
        disabled_feature=(
            "approval detail unavailable: approval.pending absent — "
            "waiting rows are shown without their prompts"
        ),
        unobserved_feature="approval detail not observed: no probe has answered yet",
        method="approval.pending",
    ),
    Seam(
        name="http-runner",
        capability="admin catalogue",
        disabled_feature="admin catalogue unavailable: GET /api/health absent",
        unobserved_feature="admin catalogue not observed: no HTTP probe has answered yet",
        http_path="/api/health",
    ),
    Seam(
        # No route exists at the running revision, so this seam is never probed
        # and never will be until one does. It renders "board queue source off",
        # which is a named absence of a *source* — never "zero cards", which is
        # a claim about work.
        name="kanban-dispatcher",
        capability="board queue source",
        disabled_feature="board queue source off",
        unobserved_feature="board queue source off: no dispatcher route to probe",
    ),
)

SEAM_NAMES: tuple[str, ...] = tuple(seam.name for seam in SEAM_CATALOGUE)


def seam_for(name: str) -> Seam:
    for seam in SEAM_CATALOGUE:
        if seam.name == name:
            return seam
    raise KeyError(f"{name} is not a seam in the catalogue")


# ── One seam's observation, and the per-connection board ─────────────────


@dataclass(frozen=True)
class SeamObservation:
    """What is known about one seam right now, and how old that knowledge is.

    ``status is None`` is R24's never-observed class and is the initial state of
    every seam. It is distinct from every probe verdict, and in particular it is
    distinct from ``absent``: "we did not look" and "it is not there" are
    different sentences and this type refuses to let them share one.
    """

    seam: str
    status: SeamStatus | None = None
    #: Where the knowledge came from (R20's source half), as a bounded phrase
    #: this module writes — never gateway text. ``""`` while never-observed.
    source: str = ""
    #: Frame-clock stamp of the probe that produced ``status``. Meaningless and
    #: ignored while ``status is None``; no age is fabricated from ``0.0``.
    observed_at: float = 0.0
    trigger: ProbeTrigger | None = None
    #: The classification's own words, already bounded and de-credentialled.
    detail: str = ""
    #: The status this observation replaced, when it replaced one. Carried so a
    #: mid-session change is nameable as a change (R12).
    previous_status: SeamStatus | None = None
    #: Frame-clock stamp of the most recent probe that read ``present``. Kept
    #: across a later failure, so a degraded line can say how old the last good
    #: answer is instead of presenting it as current.
    confirmed_at: float | None = None

    def observation(self, clock: float) -> SeamObservationClass:
        """R24's three classes, over the age of the probe rather than a row.

        A probe older than one whole revalidation interval is ``stale``: the
        story it tells was true and has not been re-established, which is
        precisely R12's "not a startup banner that goes stale the moment it
        scrolls".
        """
        if self.status is None:
            return "never-observed"
        if clock - self.observed_at > PROBE_REVALIDATION_S:
            return "stale"
        return "live"


def format_probe_age(seconds: float) -> str:
    """Whole seconds, from the frame clock, so a replay renders identically."""
    return f"{max(0.0, seconds):.0f}s"


def seam_line(observation: SeamObservation, clock: float) -> str:
    """The one operator-facing row for one seam (R10, R12, R20, R24).

    Every branch carries the seam's name and, once anything has been probed, the
    source and the age — R20's two obligations. A never-observed seam carries
    neither, because it has neither, and says so with the feature named rather
    than with a number.
    """
    seam = seam_for(observation.seam)
    if observation.status is None:
        return f"{seam.name}: not observed — {seam.unobserved_feature}"

    age = format_probe_age(clock - observation.observed_at)
    provenance = f"{observation.source}, {age} ago"
    if observation.observation(clock) == "stale":
        provenance = f"{observation.source}, last probed {age} ago, stale"

    if observation.status == "present":
        body = f"present — {seam.capability} available"
    elif observation.status == "absent":
        body = f"absent — {seam.disabled_feature}"
    elif observation.status == "incompatible":
        body = f"incompatible — {seam.disabled_feature}"
    elif observation.status == "degraded":
        body = f"degraded — {seam.disabled_feature}"
    else:
        body = f"parameter-invalid — {seam.capability} present, unproved"

    if observation.detail:
        body = f"{body}: {observation.detail}"
    if observation.status != "present" and observation.confirmed_at is not None:
        was = format_probe_age(clock - observation.confirmed_at)
        body = f"{body}; last confirmed present {was} ago"
    return f"{seam.name}: {body} ({provenance})"


@dataclass(frozen=True)
class SeamBoard:
    """One connection's seam observations, in catalogue order.

    Per connection, because a fleet dials several gateways and a capability one
    install has is not a capability another has (KTD3's profile half). The board
    is data only: it holds no clock, opens nothing, and every function over it is
    pure, so a replayed recording reproduces every line and every age exactly
    (R20, AE8).
    """

    profile: str = ""
    observations: tuple[SeamObservation, ...] = ()
    #: Frame-clock stamp of the last probe *round*, whatever it concluded.
    #: ``None`` before the first round; :func:`seam_probe_due` reads it.
    last_round_at: float | None = None

    def observation_for(self, seam: str) -> SeamObservation:
        for observation in self.observations:
            if observation.seam == seam:
                return observation
        raise KeyError(f"{seam} is not on this board")


def empty_board(profile: str = "") -> SeamBoard:
    """Every seam never-observed — the honest state before the first probe."""
    return SeamBoard(
        profile=profile,
        observations=tuple(SeamObservation(seam=seam.name) for seam in SEAM_CATALOGUE),
    )


def apply_probe_round(
    board: SeamBoard, results: Sequence[SeamObservation], *, at: float
) -> SeamBoard:
    """Fold one round of probe results into the board (R12).

    Seams absent from ``results`` keep the observation they had — a round that
    could not reach one seam does not erase what is known about the others.

    Two transitions are named rather than merely recorded:

    * A seam that **was** known and whose fresh probe settled nothing becomes
      ``degraded``, keeping the age of its last confirmation. A frozen "present"
      re-presented as current is what R20 forbids in so many words.
    * Any seam leaving ``present`` keeps ``confirmed_at``, so its line can say
      how old the last good answer was. That is the same honesty, generalized to
      the transitions that *did* settle.
    """
    merged: list[SeamObservation] = []
    fresh = {result.seam: result for result in results}
    for previous in board.observations:
        result = fresh.get(previous.seam)
        if result is None:
            merged.append(previous)
            continue
        if result.status is None:
            if previous.status is None:
                # Never observed, still never observed. The failed probe's
                # detail is not kept: a never-observed seam has no source and
                # no age, and attaching a diagnostic to it would imply one.
                merged.append(previous)
                continue
            merged.append(
                replace(
                    result,
                    status="degraded",
                    previous_status=previous.status,
                    confirmed_at=previous.confirmed_at,
                    observed_at=at,
                )
            )
            continue
        merged.append(
            replace(
                result,
                previous_status=previous.status,
                confirmed_at=(at if result.status == "present" else previous.confirmed_at),
                observed_at=at,
            )
        )
    return replace(board, observations=tuple(merged), last_round_at=at)


def next_probe_due_at(board: SeamBoard) -> float:
    """When the revalidation round is next due, on the frame clock.

    A board that has never probed is due immediately. Pure arithmetic — the app
    owns the timer, exactly as it does for the registry's poll cadence.
    """
    if board.last_round_at is None:
        return 0.0
    return board.last_round_at + PROBE_REVALIDATION_S


def seam_probe_due(board: SeamBoard, clock: float, trigger: ProbeTrigger) -> bool:
    """Whether a probe round may run now, for this reason (R12's cadence).

    ``attach`` and ``reconnect`` are events, not schedule points: a connection
    that just came up is re-probed whatever the clock says, because its install
    may be a different install. ``revalidation`` is the only trigger the
    interval governs, and it is the only one a periodic timer can pass — which
    is what keeps a five-minute cadence from becoming a per-render one.
    """
    if trigger in ("attach", "reconnect"):
        return True
    return clock >= next_probe_due_at(board)


def board_lines(board: SeamBoard, clock: float) -> tuple[str, ...]:
    """Every seam's row, catalogue order, ready for literal rendering."""
    return tuple(seam_line(observation, clock) for observation in board.observations)


def seam_row_stays_visible(observation: SeamObservation, clock: float) -> bool:
    """Whether one seam's row stays in the status region after the #122 move.

    U1's operator-action-required rule (infiquetra/talaria#122), decided row by
    row — origin alone never keeps a row, because origin does not predict
    actionability. The inspector always holds every row; this answers only what
    the region keeps.

    Classification (verdict, and the operator action it does or does not need):

    * live ``present`` → MOVE. Informational steady state; nothing to act on.
    * ``absent`` / ``incompatible`` / ``degraded`` / ``parameter-invalid`` →
      STAY. Each names a lost or unproved capability the operator must stop
      relying on, reconfigure, or investigate.
    * ``present`` but ``stale`` → STAY. The age makes the verdict's currency
      ambiguous, and the safe direction is visible. Reviewer: confirm that a
      stale-present row should keep surfacing rather than ageing quietly in
      the inspector.
    * never-observed (any seam) → MOVE. The line carries no source and no age,
      so there is nothing to act on yet; ``kanban-dispatcher`` is permanently
      in this state by construction. A transport that is actually down still
      surfaces through the connection notice, the fleet rows, and the queue's
      per-connection lines — and a seam that was ever known degrades rather
      than returning here. Reviewer: confirm that a probeable seam the rounds
      could never settle should stay moved while those backstops carry it.
    """
    if observation.status is None:
        return False
    if observation.observation(clock) == "stale":
        return True
    return observation.status != "present"


def split_seam_lines(
    board: SeamBoard, clock: float
) -> tuple[tuple[str, ...], tuple[str, ...]]:
    """Split one board snapshot into ``(region_lines, inspector_lines)`` (#122).

    Both surfaces derive from this single snapshot, so a failure arriving while
    its row data is mid-move is classified once and reaches both surfaces
    together — the move never races the alert path. The inspector half is
    :func:`board_lines` unchanged, so the move relocates rows without
    rewording them; the region half keeps only the rows
    :func:`seam_row_stays_visible` names, in catalogue order.
    """
    inspector = board_lines(board, clock)
    region = tuple(
        line
        for observation, line in zip(board.observations, inspector, strict=True)
        if seam_row_stays_visible(observation, clock)
    )
    return region, inspector


# ══ Replay: seam observations rebuilt from a recording, no socket ════════
#
# The replay gate's whole claim is that the interface runs from a file with no
# socket open in the process. A seam board is part of that interface, so it has
# to reconstruct the same way everything else does: from recorded frames.
#
# This is a *reader*, not a dispatcher. It takes no connection, no callable, and
# no origin — there is nothing here a socket could be threaded through, which is
# how "replay opens no socket" is a property of the type signature rather than a
# claim about behavior somebody has to keep true.


@dataclass(frozen=True)
class RecordedFrame:
    """One frame-log record, in the only three fields this reader reads.

    Deliberately not :class:`~talaria.recorder.reader.FrameLogEntry`: the caller
    adapts, and this module stays free of the recorder's file format. ``at`` is
    the recorded frame-clock stamp, which is why a replayed seam line reproduces
    its age exactly (R20, AE8).
    """

    direction: Literal["out", "in"]
    at: float
    frame: Mapping[str, Any]


def _probe_exchanges(
    frames: Sequence[RecordedFrame],
) -> list[tuple[RecordedFrame, str, bool]]:
    """Recorded probe replies, each paired with the request it actually answers.

    Returned in recording order, as ``(reply, method, carried_parameters)``. The
    boolean is what :func:`seam_board_from_recording` needs to apply R11's re-ask
    rule to a recording exactly as the live prober applies it to a wire.

    **A JSON-RPC id is not a durable name, and matching on it alone was a real
    defect.** The correlator restarts its id counter at every reconnect —
    deliberately, so that the epoch guard it relies on stays exercised
    (``talaria/transport/rpc.py``'s ``open_epoch``) — and a frame-log record
    carries no epoch of its own to disambiguate with. The first version of this
    built one id-keyed map over the *whole* recording with last-write-wins, then
    graded replies against it, so a reply could be attributed to a request that
    had not been sent yet. A corpus spanning a reconnect really did render the
    roster seam absent on the strength of a refusal that named a different
    method — the mis-attribution was legible in the rendered sentence.

    So the walk is in order and an id is only ever outstanding for one request at
    a time: an outbound frame reusing an id supersedes whatever that id meant
    before, whether or not the new call is a probe, and a reply consumes the
    request it answers. An unanswered probe at the end of a recording therefore
    claims nothing, which is the honest reading of a log that stops mid-exchange.

    **And an inbound frame carrying an id is not thereby a reply.** The gateway
    sends events with ids of their own, and reading one as a probe's answer
    graded a ``tool.complete`` payload against the roster's pinned signature —
    condemning the seam and discarding the genuine reply, which arrived to find
    its pairing already consumed. :func:`~talaria.domain.decode.is_reply_frame`
    is the live correlator's own envelope rule, imported rather than restated so
    the two readings of one wire cannot drift apart again.
    """
    outstanding: dict[Any, tuple[str, bool]] = {}
    exchanges: list[tuple[RecordedFrame, str, bool]] = []
    for record in frames:
        rid = record.frame.get("id")
        if rid is None:
            continue
        if record.direction == "out":
            outstanding.pop(rid, None)
            method = record.frame.get("method")
            if isinstance(method, str) and method in PROBEABLE_METHODS:
                outstanding[rid] = (method, bool(record.frame.get("params")))
            continue
        if not is_reply_frame(record.frame):
            continue
        answered = outstanding.pop(rid, None)
        if answered is not None:
            exchanges.append((record, answered[0], answered[1]))
    return exchanges


def seam_board_from_recording(
    frames: Sequence[RecordedFrame], *, profile: str = ""
) -> SeamBoard:
    """Rebuild a connection's seam board from recorded replies alone (R24, AE8).

    Every probe reply in the corpus is graded with the same rules the live
    prober uses, and a seam the recording never observed stays never-observed.
    That second half is the one worth stating: the ``http-runner`` seam is an
    HTTP request and frame logs record WebSocket frames, so a recording *never*
    carries it — and the honest rendering of that is "not observed", not
    "absent". Inventing an absence from a surface the format cannot record is
    the same error as inventing a probe URL for ``kanban-dispatcher``.

    A method probed several times in one recording keeps its **last** reply, so
    a corpus that recorded a reconnect reproduces the story as it ended rather
    than as it began. Which reply belongs to which request is decided by
    :func:`_probe_exchanges` walking the log in order, not by a JSON-RPC id
    looked up in a map built over the whole file — see its docstring for the
    reconnect that broke the second reading.

    **Its consumer is the determinism gate, which U8 owns.** This unit's file
    scope stops at the domain, and `talaria/replay/gate.py` is named in U8's,
    where the registry, queue and rendered-age checkpoints are added together —
    a seam checkpoint bolted on here would be a second, parallel gate of exactly
    the kind U8's mechanism note rejects. What is settled now is the
    reconstruction and its rules, tested in ``tests/domain/test_probe_replay.py``
    per this unit's own test scenario; adding a checkpoint that calls it is a
    line of gate code, not a design question left open.
    """
    board = empty_board(profile)
    seams_by_method = {seam.method: seam for seam in SEAM_CATALOGUE if seam.method}
    for record, method, parameterized in _probe_exchanges(frames):
        seam = seams_by_method.get(method)
        if seam is None:
            continue
        status, detail = classify_recorded_reply(
            baseline_for(method), record.frame, parameterized=parameterized
        )
        board = apply_probe_round(
            board,
            (
                SeamObservation(
                    seam=seam.name,
                    status=status,
                    source=f"recording {method}",
                    trigger="attach",
                    detail=detail,
                ),
            ),
            at=record.at,
        )
    return board


def classify_recorded_reply(
    entry: MethodBaseline, frame: Mapping[str, Any], *, parameterized: bool
) -> tuple[SeamStatus | None, str]:
    """Grade one recorded reply by the same rules the live prober applies.

    Two rules carry the weight, and both are the replay half of a rule that
    exists on the live path:

    * ``-32601`` on a request that carried parameters is
      ``parameter-invalid``, not ``absent``. A recording holds whatever calls
      were actually made, and if no bare re-ask was recorded then absence was
      never established — claiming it here would put R11's misdiagnosis back
      into the replay path, where nobody would think to look for it.
    * A refusal of a ``presence-only`` method **proves presence** (KTD11), while
      the same refusal of a ``read-only`` method means the probe's own request
      was rejected and the capability is unproved. One reply, two readings,
      decided by the pinned classification rather than by the reply.
    """
    error = frame.get("error")
    if isinstance(error, Mapping):
        code = error.get("code")
        message = redact_probe_detail(error.get("message"))
        if code == RECORDED_METHOD_NOT_FOUND:
            if parameterized:
                return (
                    "parameter-invalid",
                    f"recorded method-not-found for a parameterized call: {message}",
                )
            return "absent", message
        settled = f"code {code}: {message}" if code is not None else message
        if entry.classification == "presence-only":
            return "present", f"proved by refusal — {settled}"
        return "parameter-invalid", settled
    if "result" not in frame:
        return None, ""
    result = frame.get("result")
    if entry.classification == "presence-only":
        # A presence probe's answer is never shape-compared: the bare call is
        # not the pinned fixture, so grading it against the pinned signature
        # would report drift that the probe's own request caused.
        return "present", ""
    if not isinstance(result, Mapping):
        # No key set to compare against at all — the same basic drift the live
        # prober reports as ``drifted``, and for the same reason.
        return "incompatible", f"{entry.method}: the result is not an object"
    drifts = compare_shape(entry, result)
    if drifts:
        return "incompatible", "; ".join(drift.describe() for drift in drifts)
    return "present", ""
