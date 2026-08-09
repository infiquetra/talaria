"""The checked-in compatibility baseline (KTD9, R34).

The terminal gateway publishes no capability endpoint — ``gateway.ready``'s
``change_events: true`` is hardcoded, so it detects nothing — which leaves two
ways to learn whether a required method exists. One is to call it, which for a
mutating method means creating a session or submitting a prompt to find out
whether you could have. R34 forbids that. The other is to record what the method
looked like at a pinned revision and compare, which is this module.

Every entry is classified:

* ``read-only`` — proved side-effect-free by reading its handler at the pin, so
  a startup check may invoke it.
* ``evidence-only`` — mutating or request-scoped. Never probed. Its evidence is
  the pinned source line plus the request fixture and response shape below,
  exercised by isolated acceptance tests rather than by a live call.

The response signature is deliberately **not** a JSON Schema. A hand-written
schema is a large artifact whose own correctness nobody checks, and nested
structure is where schema maintenance cost explodes. What is recorded is the
top-level key set plus the kind of each value, which is what an attach actually
breaks on: drift detection is then a set comparison naming the method and the
specific key.

U3 owns this data. U10 owns the check that runs against it.
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from typing import Any, Literal

#: Every line reference in this module is read at this Hermes revision.
BASELINE_PIN = "7f4d15515"

Classification = Literal["read-only", "evidence-only"]

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
        },
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

#: Methods whose classification permits a startup invocation. Anything not in
#: this set is evidence-only and must never be called to discover whether it
#: exists (R34, AE7).
READ_ONLY_METHODS: frozenset[str] = frozenset(
    entry.method for entry in COMPAT_BASELINE if entry.classification == "read-only"
)

EVIDENCE_ONLY_METHODS: frozenset[str] = frozenset(
    entry.method for entry in COMPAT_BASELINE if entry.classification == "evidence-only"
)

REQUIRED_METHODS: frozenset[str] = READ_ONLY_METHODS | EVIDENCE_ONLY_METHODS

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
