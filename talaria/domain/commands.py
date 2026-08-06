"""Slash commands: the catalogue, the local control set, and generic dispatch.

Framework-free by ADR-0002 — the palette and the composer import this, never the
other way round.

**The design constraint is that no gateway command gets a bespoke interface.**
The gateway owns roughly a hundred commands and grows more without telling
anyone, so a client that knows what ``/model`` means has already lost: it will
be wrong the first time Hermes changes it, and silently. What Talaria knows
instead is the *shape* of an answer. ``command.dispatch`` returns one of six
result types (``exec``, ``plugin``, ``alias``, ``skill``, ``send``,
``prefill``), U3's :func:`~talaria.domain.decode.decode_dispatch_result` folds
all six into one record with three text destinations, and
:func:`render_dispatch` below routes those three destinations without ever
reading the command's name. There is no place in this module where a command
name selects behaviour, and that absence is what makes the unit finished rather
than perpetually one command behind.

Three things do get names, and each is a closed set with evidence behind it:

* **The Talaria-local control set** — ``/quit``, ``/pause``, ``/resume``,
  ``/speed`` (PC6), plus ``/models`` (U2, 2026-08-06 model-picker plan). Every
  name in this set is resolved *before* the catalogue, so a gateway command
  that later ships one of these names cannot take it away (PC6) — but only the
  first four never touch a socket at all. ``/models`` with no argument only
  opens or closes a local region; given a row number it composes and sends
  ``/model <name> --provider <slug>`` down the ordinary ``slash.exec`` path, the
  same one a typed ``/model`` line takes. What makes ``/models`` local is that
  the name ``/models`` itself is never dispatched to the gateway — the
  singular ``/model`` it composes is a real, separately-catalogued command.
  ``/profiles`` (U4) joins the set on the same terms: the gateway owns the
  singular ``/profile``, and the plural opens a local region whose selection
  dials a different gateway rather than sending anything to this one.
* **The official-client-local entries** — ``/density``, ``/logs``, ``/mouse``,
  ``/sessions``, the four ``_TUI_EXTRA`` rows at ``tui_gateway/server.py:11514``
  (``7f4d15515``). The gateway advertises them in ``commands.catalog`` and
  implements them nowhere: they are handled inside Hermes's own React terminal
  UI. Dispatching one returns an error, and inventing a local equivalent would
  be a client feature wearing a gateway command's name. They are listed as
  unsupported and refused before the call.
* **The paste-collapse threshold** — KTD16's 6 lines or 512 bytes.

**Why the unsupported four are matched on name *and* category.** The gateway's
catalogue builder skips a ``_TUI_EXTRA`` row whose name collides with a real
registry command, and says so in its own comment ("the registry entry is
canonical", ``methods_tools.py:290-297``); ``/sessions`` is the example it
gives. Matching on the name alone would therefore mark a genuinely dispatchable
``/sessions`` unsupported the day the registry adds it. The extras carry
category ``TUI`` and no command in ``hermes_cli/commands.py`` does — measured at
the pin by parsing all 90 ``CommandDef`` calls: 34 ``Session``, 19
``Configuration``, 19 ``Tools & Skills``, 17 ``Info``, 1 ``Exit``, and zero
``TUI`` — so requiring both is precise in a way either half alone is not.

**At the pin, only three of the four actually arrive as extras.** The registry
already defines ``CommandDef("sessions", "Browse and resume previous sessions",
"Session")`` (``hermes_cli/commands.py:180``), so the dedup guard above drops
the ``/sessions`` extra and a real catalogue serves ``/sessions`` under category
``Session`` — dispatchable, not unsupported. The plan's list of four is one name
out of date, and this module follows the source rather than the list: three
render unsupported and ``/sessions`` renders as an ordinary command. That is not
an open question awaiting a live run; it is settled by the pinned registry.

**Ordinary commands go out over ``slash.exec``, not ``command.dispatch``.**
``command.dispatch``'s last line is ``_err(rid, 4018, f"not a
quick/plugin/bundle/skill command: {name}")`` (``methods_tools.py:1070``), and
above it the handler serves only quick commands, plugin commands, skill
bundles, skill commands, and twelve hardcoded name groups. A real catalogue
carries 82 of the registry's 90 ``CommandDef`` rows — the builder drops only
the eight ``gateway_only`` ones, since the four hidden names are inside that
set — and only a minority of the 82 is anything ``command.dispatch`` serves.
The exact residual is not derivable here, because three of those five sets are
assembled at runtime from the operator's config and installed skills; what is
derivable is that a client which only ever calls ``command.dispatch`` lists
most of the catalogue as working and refuses it on use. Hermes's own client
calls ``slash.exec`` first and falls back to ``command.dispatch`` in the ``.catch()``
(``ui-tui/src/app/createSlashHandler.ts:147-166``); :data:`SLASH_EXEC_METHOD`
and :func:`decode_slash_exec` are that ordering re-encoded.
"""

from __future__ import annotations

import math
import re
from collections.abc import Mapping
from dataclasses import dataclass, field
from typing import Any, Final, Literal

from talaria.domain.decode import (
    DispatchResult,
    UnknownDispatchResult,
    decode_dispatch_result,
)

# ── gateway methods this unit speaks (KTD9's baseline names) ─────────────

#: Read-only, fetched once at startup (``tui_gateway/methods_tools.py:255``).
CATALOG_METHOD: Final[str] = "commands.catalog"

#: The call an ordinary catalogue command goes through first
#: (``tui_gateway/methods_tools.py:1073``). It routes a registry command to the
#: session's slash worker and returns ``{"output": …}``; for a skill, a bundle,
#: or one of the ``_PENDING_INPUT_COMMANDS`` it forwards to
#: ``command.dispatch`` itself and returns that handler's structured payload
#: instead. Both replies are decoded by :func:`decode_slash_exec`.
SLASH_EXEC_METHOD: Final[str] = "slash.exec"

#: The fallback, and the only route for a quick, plugin, bundle or skill command
#: when ``slash.exec`` refuses (``tui_gateway/methods_tools.py:432``). It is
#: **not** sufficient on its own: its last line refuses anything that is not one
#: of those four or one of twelve hardcoded names.
DISPATCH_METHOD: Final[str] = "command.dispatch"

#: The large-paste round trip (``tui_gateway/methods_complete.py:14``).
PASTE_COLLAPSE_METHOD: Final[str] = "paste.collapse"

#: Stops one delegated child (``tui_gateway/methods_session.py:2806``).
#: Deliberately distinct from ``session.interrupt``, which stops the operator's
#: own turn — the two are one keystroke apart in the interface and must never
#: be one branch apart in the code.
SUBAGENT_INTERRUPT_METHOD: Final[str] = "subagent.interrupt"


# ── the catalogue ────────────────────────────────────────────────────────

#: What a listed command can actually do, from Talaria's side.
Availability = Literal["dispatch", "talaria-local", "unsupported"]

#: The category the gateway files its four client-local extras under
#: (``tui_gateway/server.py:11514``).
CLIENT_LOCAL_CATEGORY: Final[str] = "TUI"

#: The four names those extras use.
CLIENT_LOCAL_NAMES: frozenset[str] = frozenset(
    {"/density", "/logs", "/mouse", "/sessions"}
)

#: Why a client-local entry is refused rather than attempted or faked.
CLIENT_LOCAL_REASON: Final[str] = (
    "handled inside Hermes's own terminal UI — the gateway advertises it but "
    "implements no dispatch path for it"
)

#: Shown in place of a catalogue Talaria could not read.
CATALOG_UNAVAILABLE: Final[str] = "the gateway's command catalogue is unavailable"

#: How each availability is labelled wherever commands are listed. ``dispatch``
#: is blank because it is the ordinary case and a marker on every row is a
#: marker on none.
AVAILABILITY_MARKER: Mapping[Availability, str] = {
    "dispatch": "",
    "talaria-local": "local",
    "unsupported": "unsupported",
}


@dataclass(frozen=True)
class CommandEntry:
    """One listed command: what it is called, what it does, and whether it works."""

    name: str
    description: str
    category: str = ""
    availability: Availability = "dispatch"

    @property
    def marker(self) -> str:
        return AVAILABILITY_MARKER[self.availability]


@dataclass(frozen=True)
class CommandCatalog:
    """The catalogue as Talaria holds it, including what it could not read.

    ``available`` is false when ``commands.catalog`` failed. The Talaria-local
    entries are still listed in that case — they never needed the gateway — and
    :meth:`resolve` still routes them, so an operator whose gateway is refusing
    calls can still leave.
    """

    entries: tuple[CommandEntry, ...] = ()
    canon: Mapping[str, str] = field(default_factory=dict)
    warning: str = ""
    available: bool = True
    failure: str = ""

    @property
    def gateway_entries(self) -> tuple[CommandEntry, ...]:
        return tuple(e for e in self.entries if e.availability == "dispatch")

    @property
    def unsupported_entries(self) -> tuple[CommandEntry, ...]:
        return tuple(e for e in self.entries if e.availability == "unsupported")

    @property
    def local_entries(self) -> tuple[CommandEntry, ...]:
        return tuple(e for e in self.entries if e.availability == "talaria-local")

    def canonical(self, name: str) -> str:
        """Resolve an alias through the gateway's own ``canon`` map.

        Unknown names pass through unchanged rather than being rejected: the
        catalogue omits commands (skills arrive in ``pairs`` but not in
        ``categories``, quick commands appear only if configured), and refusing
        to send a name the gateway may well know would make Talaria's copy of
        the catalogue authoritative over the gateway's own registry.
        """
        return self.canon.get(name.lower(), name)

    def entry_for(self, name: str) -> CommandEntry | None:
        lowered = name.lower()
        for entry in self.entries:
            if entry.name.lower() == lowered:
                return entry
        return None


def _pair(raw: Any) -> tuple[str, str] | None:
    """One ``[name, description]`` row, or ``None`` if it is not one.

    The gateway builds these rows from a registry, a config file, and a
    filesystem scan, so a malformed row is a live possibility rather than a
    theoretical one. Dropping it is right: this is a listing, and one bad row
    must not cost the operator the other ninety.
    """
    if not isinstance(raw, (list, tuple)) or len(raw) < 2:
        return None
    name, description = raw[0], raw[1]
    if not isinstance(name, str) or not name:
        return None
    return name, description if isinstance(description, str) else ""


def _category_index(raw: Any) -> dict[str, str]:
    """Map each listed command to the category it was filed under.

    Built from ``categories`` rather than ``pairs`` because ``pairs`` is flat.
    A command absent here (every skill command is — the gateway appends those
    to ``pairs`` only) simply has no category, which is all
    :data:`CLIENT_LOCAL_CATEGORY` needs to decide against.
    """
    index: dict[str, str] = {}
    if not isinstance(raw, list):
        return index
    for group in raw:
        if not isinstance(group, Mapping):
            continue
        category = group.get("name")
        if not isinstance(category, str):
            continue
        for row in group.get("pairs") or ():
            pair = _pair(row)
            if pair is not None:
                index.setdefault(pair[0].lower(), category)
    return index


def decode_catalog(result: Any) -> CommandCatalog:
    """Turn a ``commands.catalog`` reply into a listing Talaria can render.

    Never raises. A reply that is not an object, or one whose ``pairs`` is
    missing, yields an unavailable catalogue rather than an exception — the
    same R5 discipline the frame decoder applies, for the same reason: this
    runs at startup and a client that cannot start because a listing was odd is
    worse than one that starts and says the listing was odd.
    """
    if not isinstance(result, Mapping):
        return unavailable_catalog(CATALOG_UNAVAILABLE)

    rows = result.get("pairs")
    if not isinstance(rows, list):
        return unavailable_catalog(f"{CATALOG_UNAVAILABLE}: it carried no command list")

    categories = _category_index(result.get("categories"))
    entries: list[CommandEntry] = list(_local_entries())
    seen = {entry.name.lower() for entry in entries}

    for row in rows:
        pair = _pair(row)
        if pair is None:
            continue
        name, description = pair
        lowered = name.lower()
        if lowered in seen:
            # A gateway command whose name collides with a Talaria-local one is
            # shadowed, not listed twice. The local one wins because it is the
            # one the operator's exit key depends on, and a duplicate row would
            # advertise a dispatch that :func:`resolve_command` never performs.
            continue
        seen.add(lowered)
        category = categories.get(lowered, "")
        entries.append(
            CommandEntry(
                name=name,
                description=description,
                category=category,
                availability=(
                    "unsupported" if _is_client_local(name, category) else "dispatch"
                ),
            )
        )

    canon = result.get("canon")
    warning = result.get("warning")
    return CommandCatalog(
        entries=tuple(entries),
        canon={
            key.lower(): value
            for key, value in (canon.items() if isinstance(canon, Mapping) else ())
            if isinstance(key, str) and isinstance(value, str)
        },
        warning=warning if isinstance(warning, str) else "",
        available=True,
    )


def unavailable_catalog(reason: str) -> CommandCatalog:
    """A catalogue that names why it is empty, still carrying the local set."""
    return CommandCatalog(
        entries=_local_entries(), canon={}, available=False, failure=reason
    )


def _is_client_local(name: str, category: str) -> bool:
    return name.lower() in CLIENT_LOCAL_NAMES and category == CLIENT_LOCAL_CATEGORY


# ── the Talaria-local control set (PC6) ──────────────────────────────────

LocalAction = Literal["quit", "pause", "resume", "speed", "models", "profiles"]


@dataclass(frozen=True)
class LocalCommand:
    """One control Talaria performs itself, with no socket involved."""

    name: str
    action: LocalAction
    description: str
    argument_hint: str = ""
    #: True for the three that scale a *recorded* clock. A live session has no
    #: such clock, so performing them there would be an inert control wearing a
    #: working one's name — the failure AE11 exists to prevent, arrived at from
    #: the other direction.
    replay_only: bool = False


TALARIA_LOCAL_COMMANDS: tuple[LocalCommand, ...] = (
    LocalCommand("/quit", "quit", "Leave Talaria; the gateway session keeps running"),
    LocalCommand("/pause", "pause", "Hold the replay clock", replay_only=True),
    LocalCommand("/resume", "resume", "Release the replay clock", replay_only=True),
    LocalCommand(
        "/speed",
        "speed",
        "Set the replay rate",
        argument_hint="<multiplier|max>",
        replay_only=True,
    ),
    # Plural, and deliberately so (U2, 2026-08-06 model-picker plan): the
    # gateway's own registry already owns the singular ``/model`` — probed
    # live on 2026-08-06, one of 114 catalogue names — so a Talaria-local
    # ``/model`` would shadow a working gateway command instead of filling a
    # gap. With no argument this opens or closes the picker; with one, it
    # selects the row that index names (``talaria/ui/picker.py``) and sends
    # it down the same ``/model <name> --provider <slug>`` path an operator
    # typing that line by hand would take (R2). A third shape, ``<n> default``
    # (and its confirmation resend ``<n> default confirm``), writes that row
    # as the connected profile's default model instead of switching the
    # running session (U5, KTD7) — still no new command name, per the plan's
    # "U5 adds no command" design note.
    LocalCommand(
        "/models",
        "models",
        "Open the model picker, select a row, or set a row as the profile's default",
        argument_hint="[index [default [confirm]]]",
    ),
    # Plural for the same reason and with the same hazard (U4). The gateway
    # owns the singular ``/profile``; ``/profiles`` is free and is Talaria's.
    # An operator who types ``/profile`` reaches Hermes and one who types
    # ``/profiles`` reaches this picker — a one-character difference between
    # two different destinations, which is why both names carry the ``local``
    # availability marker in the listing.
    #
    # Selecting a profile here never mutates the gateway's own active-profile
    # setting (KTD5): it dials that profile's gateway instead.
    LocalCommand(
        "/profiles",
        "profiles",
        "Open the profile picker, or switch to a listed profile by its number",
        argument_hint="[index]",
    ),
)

#: Said when a pacing control is used against a live session.
LIVE_HAS_NO_REPLAY_CLOCK: Final[str] = (
    "governs a recorded replay's pacing, and this session is live — nothing changed"
)

#: What ``/speed`` accepts for "no inter-frame delay at all".
UNBOUNDED_SPEED_WORDS: frozenset[str] = frozenset({"max", "inf", "infinity", "unbounded"})


def _local_entries() -> tuple[CommandEntry, ...]:
    return tuple(
        CommandEntry(
            name=command.name,
            description=(
                f"{command.description} {command.argument_hint}".strip()
                if command.argument_hint
                else command.description
            ),
            category="Talaria",
            availability="talaria-local",
        )
        for command in TALARIA_LOCAL_COMMANDS
    )


def local_command(name: str) -> LocalCommand | None:
    lowered = name.lower()
    for command in TALARIA_LOCAL_COMMANDS:
        if command.name == lowered:
            return command
    return None


def parse_speed(argument: str) -> float | None:
    """Read ``/speed``'s argument, or ``None`` if it is not a rate.

    ``None`` rather than a default, because silently substituting 1x for a typo
    changes the replay rate to something the operator did not ask for and says
    nothing about it.
    """
    word = argument.strip().lower()
    if not word:
        return None
    if word in UNBOUNDED_SPEED_WORDS:
        return math.inf
    try:
        value = float(word)
    except ValueError:
        return None
    if math.isnan(value) or value <= 0:
        return None
    return value


# ── parsing an entry line ────────────────────────────────────────────────

#: A command is a slash, a name, and then either end-of-input or whitespace.
#:
#: The trailing boundary is what keeps ``/usr/bin/env is missing`` a message
#: rather than a call to a command named ``usr``. Names are restricted to the
#: characters the gateway's own registry uses, so a path, a URL, or a regex
#: pasted into the composer is never mistaken for a command.
_COMMAND_LINE = re.compile(r"^/([A-Za-z][A-Za-z0-9_-]*)(?:\s+(.*))?$", re.DOTALL)


@dataclass(frozen=True)
class ParsedCommand:
    """A command line split into its canonical-cased name and its argument."""

    name: str
    argument: str


def parse_command_line(text: str) -> ParsedCommand | None:
    """Split ``/name arg`` out of composed text, or return ``None``.

    **Multi-line text is never a command.** A pasted script whose first line
    starts with a slash is a message, and dispatching its first word while
    discarding the rest would be the worst available reading of it.
    """
    body = text.strip()
    if "\n" in body:
        return None
    match = _COMMAND_LINE.match(body)
    if match is None:
        return None
    return ParsedCommand(
        name=f"/{match.group(1).lower()}", argument=(match.group(2) or "").strip()
    )


# ── resolving a parsed line against the catalogue ────────────────────────


@dataclass(frozen=True)
class LocalInvocation:
    """A Talaria-local control the UI performs itself."""

    command: LocalCommand
    argument: str


@dataclass(frozen=True)
class GatewayInvocation:
    """A command to send through ``command.dispatch``.

    ``listed`` records whether the catalogue knew the name. It is *not* a gate:
    an unlisted name is still sent, because the gateway's registry is the
    authority and its catalogue omits whole classes of command. It exists so
    the UI can say "the catalogue did not list this" alongside whatever the
    gateway answers, instead of the operator wondering which of the two was
    confused.
    """

    name: str
    argument: str
    listed: bool = True

    @property
    def wire_name(self) -> str:
        """What ``command.dispatch`` wants: the name with no leading slash
        (``methods_tools.py:434`` strips it itself, and sending it stripped
        keeps the request identical to the one the compat fixture records)."""
        return self.name.lstrip("/")


@dataclass(frozen=True)
class UnsupportedInvocation:
    """A catalogue entry with no dispatch path. Named, never attempted."""

    name: str
    reason: str = CLIENT_LOCAL_REASON


Invocation = LocalInvocation | GatewayInvocation | UnsupportedInvocation


def resolve_command(text: str, catalog: CommandCatalog | None = None) -> Invocation | None:
    """Decide what composed text is, without deciding what any command means.

    Returns ``None`` for ordinary prose, which is the common case and must stay
    the cheapest one. The local set is consulted **before** the catalogue
    (PC6), so a gateway command that later takes one of those four names cannot
    shadow the control it is named after.

    **It is consulted a second time, after alias resolution, and that is not
    belt-and-braces.** The registry defines ``/quit`` with the alias ``exit``
    (``hermes_cli/commands.py:330-331`` at ``7f4d15515``), so a real catalogue
    carries ``canon["/exit"] = "/quit"``. Checking only the typed name let
    ``/exit`` resolve to ``/quit`` and then leave over the socket, where the
    gateway does not implement it — the operator typed the exit command and did
    not exit. Anything the gateway's own ``canon`` map resolves *onto* a local
    name is that local control.
    """
    parsed = parse_command_line(text)
    if parsed is None:
        return None

    command = local_command(parsed.name)
    if command is not None:
        return LocalInvocation(command=command, argument=parsed.argument)

    catalog = catalog if catalog is not None else CommandCatalog()
    canonical = catalog.canonical(parsed.name)
    command = local_command(canonical)
    if command is not None:
        return LocalInvocation(command=command, argument=parsed.argument)

    entry = catalog.entry_for(canonical)
    if entry is not None and entry.availability == "unsupported":
        return UnsupportedInvocation(name=entry.name)
    return GatewayInvocation(
        name=canonical, argument=parsed.argument, listed=entry is not None
    )


# ── rendering a dispatch result, generically (R24) ───────────────────────

#: Said about a result shape that did not exist at ``7f4d15515``.
UNKNOWN_RESULT_NOTICE: Final[str] = (
    "the gateway answered with a result shape Talaria does not know"
)

#: Said when a shape Talaria *does* know carried nothing to show.
NO_DISPLAY_TEXT: Final[str] = "(the gateway returned no output)"

#: Said instead of repeating text that has just been put in the composer.
PREFILLED_INTO_COMPOSER: Final[str] = "(prefilled into the composer)"

#: How much of one command's output the transcript keeps. The gateway already
#: truncates a quick command's output at 4000 characters
#: (``methods_tools.py:471``); this bounds every other shape at the same place
#: so one command cannot displace the conversation it was run inside.
COMMAND_OUTPUT_CLIP: Final[int] = 4000


@dataclass(frozen=True)
class DispatchRendering:
    """Where one dispatch result's text goes. Three destinations, no shape names.

    Nothing in :func:`render_dispatch` reads ``result.type``, and that is the
    point rather than an economy: the six shapes differ only in which of
    :class:`~talaria.domain.decode.DispatchResult`'s three text fields they
    populate, so routing the fields routes the shapes — including any seventh
    shape that populates them.
    """

    transcript_line: str
    submit_text: str | None = None
    prefill_text: str | None = None
    notice: str = ""
    #: The name an ``alias`` result points at, so the caller can run it. Carried
    #: rather than acted on here, because running it is an RPC and this module
    #: is framework-free and I/O-free.
    alias_target: str | None = None


def _clip(text: str) -> str:
    if len(text) <= COMMAND_OUTPUT_CLIP:
        return text
    return text[:COMMAND_OUTPUT_CLIP] + "…"


def render_dispatch(
    name: str, result: DispatchResult | UnknownDispatchResult
) -> DispatchRendering:
    """Route one result's text to the transcript, the model, and the composer.

    Only ``display_text`` may reach the transcript — R24's clause that
    model-facing skill and bundle scaffolding is never rendered. ``submit_text``
    goes to the gateway and is deliberately *not* written down as the
    operator's own line: the whole reason ``display`` exists is that ``message``
    is several kilobytes of system-prompt fragment, and echoing it into the
    transcript would defeat the projection by a different route.

    An unrecognized shape is surfaced by name and carries nothing onward (R5's
    discipline applied to results). Guessing at a new shape's fields is how a
    client renders a payload it does not understand as though it did.
    """
    if isinstance(result, UnknownDispatchResult):
        shape = result.type.strip() or "(unnamed)"
        line = f"{name}: {UNKNOWN_RESULT_NOTICE} — {shape}"
        return DispatchRendering(transcript_line=line, notice=line)

    notice = _clip(result.notice.strip()) if result.notice else ""

    if result.prefill_text is not None:
        # The one place a destination changes the transcript line, and it is a
        # question about destinations rather than about shapes: text that is
        # now sitting in the composer, editable, does not also belong in the
        # transcript. A ``/goal`` prefill is routinely a couple of thousand
        # characters, and printing it twice makes the copy the operator cannot
        # edit the larger of the two.
        body = PREFILLED_INTO_COMPOSER
    else:
        body = _clip(result.display_text.strip()) or NO_DISPLAY_TEXT

    return DispatchRendering(
        transcript_line=f"{name}: {body}",
        submit_text=result.submit_text,
        prefill_text=result.prefill_text,
        notice=notice,
        alias_target=result.alias_target or None,
    )


# ── ``slash.exec``: the route an ordinary catalogue command takes ────────


@dataclass(frozen=True)
class SlashOutput:
    """A ``slash.exec`` reply that is plain command output rather than a shape.

    The handler answers ``{"output": …}`` — plus ``"warning"`` when running the
    command had a side effect the gateway had to mirror into the session
    (``methods_tools.py:1198-1203``) — for every command it runs in the slash
    worker, which is most of the registry.
    """

    text: str
    warning: str = ""


def slash_exec_command(invocation: GatewayInvocation) -> str:
    """The ``command`` string ``slash.exec`` wants: the whole line, no slash.

    Hermes's client sends ``cmd.slice(1)`` — the composed line with its leading
    slash removed, argument and all (``createSlashHandler.ts:147``) — and the
    handler splits the name off itself.
    """
    return f"{invocation.wire_name} {invocation.argument}".strip()


def decode_slash_exec(
    result: Any,
) -> DispatchResult | UnknownDispatchResult | SlashOutput:
    """Read a ``slash.exec`` reply, which comes in two forms.

    ``slash.exec`` forwards a skill, a bundle, or one of the pending-input
    commands to ``command.dispatch`` and returns *that* handler's structured
    payload verbatim (``methods_tools.py:1102-1140``), so a reply carrying a
    string ``type`` is a dispatch result and is decoded as one. Anything else is
    slash-worker output. The discriminator is the same one Hermes's own client
    uses — ``typeof o.type !== 'string'`` (``ui-tui/src/lib/rpc.ts:11``) — and
    the reason to copy it rather than invent one is that the two clients are
    reading the same untagged union off the same handler.
    """
    if not isinstance(result, Mapping):
        return SlashOutput(text="")
    if isinstance(result.get("type"), str):
        return decode_dispatch_result(result)
    output = result.get("output")
    warning = result.get("warning")
    return SlashOutput(
        text=output if isinstance(output, str) else "",
        warning=warning if isinstance(warning, str) else "",
    )


def render_slash_output(name: str, output: SlashOutput) -> DispatchRendering:
    """Route slash-worker output to the transcript, bounded like every shape.

    A ``warning`` goes to the notice bar rather than into the line: it is the
    gateway saying it had to mirror a side effect of the command into the
    session, which is about the session and not about what the command printed.
    """
    body = _clip(output.text.strip()) or NO_DISPLAY_TEXT
    return DispatchRendering(
        transcript_line=f"{name}: {body}",
        notice=_clip(output.warning.strip()) if output.warning else "",
    )


# ── the paste-collapse threshold (KTD16) ─────────────────────────────────

#: KTD16's defaults, mirrored from :data:`talaria.config.DEFAULTS` so a caller
#: with no configuration still gets the documented behaviour.
DEFAULT_COLLAPSE_LINES: Final[int] = 6
DEFAULT_COLLAPSE_BYTES: Final[int] = 512


@dataclass(frozen=True)
class PasteThreshold:
    """When a paste stops being inserted literally and starts being collapsed.

    Both bounds are needed and KTD16 says why: lines alone lets a single
    enormous line through, bytes alone collapses a short paste that merely
    happened to be wide. Whichever trips first wins.

    **A non-positive bound disables that half rather than firing on
    everything**, which re-encodes the gateway client's own guard
    (``useComposerState.ts:277-280``: ``pasteCollapseLines > 0 && …``). Without
    it, ``TALARIA_COMPOSER_PASTE_COLLAPSE_LINES=0`` reads as "collapse at zero
    lines" and sends every one-word paste on a round trip — the reading
    ``QUEUED.md``'s unbounded-configuration item flagged.
    """

    lines: int = DEFAULT_COLLAPSE_LINES
    byte_limit: int = DEFAULT_COLLAPSE_BYTES

    def trips(self, text: str) -> bool:
        if self.lines > 0 and text.count("\n") + 1 >= self.lines:
            return True
        return self.byte_limit > 0 and len(text.encode("utf-8")) >= self.byte_limit


@dataclass(frozen=True)
class CollapsedPaste:
    """What ``paste.collapse`` gave back: a one-line stand-in and where it went."""

    placeholder: str
    path: str
    lines: int


def decode_collapsed_paste(result: Any) -> CollapsedPaste | None:
    """Read a ``paste.collapse`` reply, or ``None`` if it is not usable.

    A reply with no ``placeholder`` is not a partial success to paper over:
    without the stand-in there is nothing to put in the composer, and the
    caller's failure path — keep the original text, say the capability is
    missing — is the correct handling of it (AE13).
    """
    if not isinstance(result, Mapping):
        return None
    placeholder = result.get("placeholder")
    if not isinstance(placeholder, str) or not placeholder:
        return None
    path = result.get("path")
    lines = result.get("lines")
    return CollapsedPaste(
        placeholder=placeholder,
        path=path if isinstance(path, str) else "",
        lines=lines if isinstance(lines, int) and not isinstance(lines, bool) else 0,
    )
