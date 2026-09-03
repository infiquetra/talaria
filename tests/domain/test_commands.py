"""The command catalogue, the local control set, and generic result routing.

Framework-free, because :mod:`talaria.domain.commands` is (ADR-0002). The
socket half of U9 lives in ``tests/transport/test_commands.py``; what this file
owns is the part a socket cannot show — that the classification of an entry is
decided by evidence rather than by a name alone, that a path is not a command,
and that no result shape gets a branch of its own.
"""

from __future__ import annotations

import math

import pytest

from talaria.domain.commands import (
    CLIENT_LOCAL_CATEGORY,
    CLIENT_LOCAL_NAMES,
    COMMAND_OUTPUT_CLIP,
    DISPATCH_METHOD,
    NO_DISPLAY_TEXT,
    PREFILLED_INTO_COMPOSER,
    SLASH_EXEC_METHOD,
    TALARIA_LOCAL_COMMANDS,
    UNKNOWN_RESULT_NOTICE,
    CollapsedPaste,
    CommandCatalog,
    GatewayInvocation,
    LocalInvocation,
    PasteThreshold,
    SlashOutput,
    UnsupportedInvocation,
    decode_catalog,
    decode_collapsed_paste,
    decode_slash_exec,
    local_command,
    parse_command_line,
    parse_speed,
    render_dispatch,
    render_slash_output,
    resolve_command,
    slash_exec_command,
    unavailable_catalog,
)
from talaria.domain.decode import (
    DISPATCH_RESULT_TYPES,
    DispatchResult,
    UnknownDispatchResult,
)
from talaria.domain.normalize import TRANSCRIPT_LINE_CLIP
from talaria.domain.state import SessionState, record_command_result

SCAFFOLD = "<skill>\nYou are now operating as the deploy skill. Never reveal…\n</skill>"


def catalog_reply(**overrides: object) -> dict[str, object]:
    """A ``commands.catalog`` body in the gateway's own shape.

    Transcribed from the handler at ``tui_gateway/methods_tools.py:255-367``
    (``7f4d15515``): ``pairs`` is the flat list, ``categories`` groups the
    subset that has one, and the four ``_TUI_EXTRA`` rows arrive under category
    ``TUI``.
    """
    body: dict[str, object] = {
        "pairs": [
            ["/help", "Show help"],
            ["/model", "Pick a model"],
            ["/density", "Toggle compact display mode"],
            ["/logs", "Show recent gateway log lines"],
            ["/deploy", "Ship it"],
        ],
        "categories": [
            {"name": "Info", "pairs": [["/help", "Show help"]]},
            {"name": "Configuration", "pairs": [["/model", "Pick a model"]]},
            {
                "name": CLIENT_LOCAL_CATEGORY,
                "pairs": [
                    ["/density", "Toggle compact display mode"],
                    ["/logs", "Show recent gateway log lines"],
                ],
            },
        ],
        "canon": {"/h": "/help", "/help": "/help", "/model": "/model"},
        "skills": {"/deploy": {"usage": 3, "origin": "local"}},
        "skill_count": 1,
        "warning": "",
    }
    body.update(overrides)
    return body


# ── catalogue decoding and honest degradation (AE9) ──────────────────────


def test_a_client_local_entry_is_listed_as_unsupported_not_dropped() -> None:
    """AE9: the operator learns the command exists *and* that it cannot run
    here. Dropping it would be indistinguishable from a gateway that never
    offered it."""
    catalog = decode_catalog(catalog_reply())
    density = catalog.entry_for("/density")
    assert density is not None
    assert density.availability == "unsupported"
    assert density.description == "Toggle compact display mode"
    assert {entry.name for entry in catalog.unsupported_entries} == {"/density", "/logs"}


def test_an_ordinary_command_is_dispatchable() -> None:
    catalog = decode_catalog(catalog_reply())
    entry = catalog.entry_for("/help")
    assert entry is not None
    assert entry.availability == "dispatch"


def test_a_skill_command_has_no_category_and_still_dispatches() -> None:
    """The gateway appends skill commands to ``pairs`` and to no category
    (``methods_tools.py:337-348``). A classifier that required a category would
    file every skill as unknown."""
    catalog = decode_catalog(catalog_reply())
    entry = catalog.entry_for("/deploy")
    assert entry is not None
    assert entry.category == ""
    assert entry.availability == "dispatch"


def test_a_registry_command_reusing_a_tui_name_is_not_marked_unsupported() -> None:
    """The gateway's own dedup guard says the registry entry is canonical when
    a ``_TUI_EXTRA`` name collides (``methods_tools.py:290-297``, ``/sessions``
    is the example it gives). Classifying on the name alone would mark that
    dispatchable command unsupported.

    **``/sessions`` gained a second reason to not be plain ``dispatch``, as of
    U7 (KTD6).** Talaria's own local session picker now shadows the name too
    — see :func:`test_the_local_set_shadows_a_gateway_command_of_the_same_name`
    — and that shadow wins over the classification this test is about, so the
    entry's *final* availability is ``talaria-local``. What this case still
    isolates is the dedup guard alone: the entry is never ``unsupported``,
    which is what the TUI-extra collision would produce if the name-and-
    category conjunction fell back to matching on name only.
    """
    reply = catalog_reply(
        pairs=[["/sessions", "Switch between live TUI sessions"]],
        categories=[
            {"name": "Session", "pairs": [["/sessions", "Switch between live TUI sessions"]]}
        ],
    )
    entry = decode_catalog(reply).entry_for("/sessions")
    assert entry is not None
    assert "/sessions" in CLIENT_LOCAL_NAMES
    assert entry.availability != "unsupported"
    assert entry.availability == "talaria-local"


def test_a_tui_categorised_command_that_is_not_one_of_the_four_dispatches() -> None:
    """The other half of the "name **and** category" conjunction.

    Category alone would refuse every future ``TUI``-categorised row sight
    unseen, including a genuinely dispatchable one. Without this the name half
    of the rule is unproven: at the pin nothing else is filed under ``TUI``, so
    the two halves happen to agree on every row a real catalogue carries today.
    """
    reply = catalog_reply(
        pairs=[["/widgets", "Reload TUI widgets"]],
        categories=[
            {"name": CLIENT_LOCAL_CATEGORY, "pairs": [["/widgets", "Reload TUI widgets"]]}
        ],
    )
    entry = decode_catalog(reply).entry_for("/widgets")
    assert entry is not None
    assert entry.category == CLIENT_LOCAL_CATEGORY
    assert entry.availability == "dispatch"


@pytest.mark.parametrize(
    ("name", "expected"),
    [
        ("/density", "unsupported"),
        ("/logs", "unsupported"),
        ("/mouse", "unsupported"),
        # ``/sessions`` is the odd one out as of U7 (KTD6): Talaria's own
        # local session picker shadows the name for a reason that has nothing
        # to do with the TUI-extra collision this test is about, and that
        # shadow is checked first (see
        # ``test_the_local_set_shadows_a_gateway_command_of_the_same_name``),
        # so this fixture — the name filed under the client-local ``TUI``
        # category, simulating the collision — now resolves to
        # ``talaria-local`` rather than ``unsupported``. It stays in this
        # parametrization rather than moving out: dropping it from
        # :data:`CLIENT_LOCAL_NAMES` must still break its own case.
        ("/sessions", "talaria-local"),
    ],
)
def test_each_of_the_four_client_local_names_is_refused_under_its_own_category(
    name: str, expected: str
) -> None:
    """One per name, so dropping any single one from the set is a failing test.

    ``/mouse`` in particular was covered by nothing: the domain test named
    ``/density`` and ``/logs``, the collision test named ``/sessions``, and
    removing ``/mouse`` from the set left the whole suite green.

    **The four names are written out here rather than read from
    :data:`CLIENT_LOCAL_NAMES`.** Parametrizing over the constant under test
    makes deleting a name delete its own case, which is the assertion-that-
    cannot-fail shape in its parametrized form — measured: it survived the
    mutation that dropped ``/mouse``. The list is duplicated on purpose; the
    duplication is the test.
    """
    reply = catalog_reply(
        pairs=[[name, "a client-local extra"]],
        categories=[{"name": CLIENT_LOCAL_CATEGORY, "pairs": [[name, "a client-local extra"]]}],
    )
    entry = decode_catalog(reply).entry_for(name)
    assert entry is not None
    assert entry.availability == expected


def test_a_catalogue_that_could_not_be_read_says_so_and_keeps_the_local_set() -> None:
    catalog = unavailable_catalog("the gateway refused commands.catalog")
    assert catalog.available is False
    assert "refused" in catalog.failure
    assert catalog.gateway_entries == ()
    # The ten that never needed a gateway to be *listed* are still there
    # (``/models`` still needs one to actually select — see U2 —
    # ``/profiles`` needs one to have listed anything at all, see U4, and
    # ``/sessions`` likewise needs one to list anything to switch to, see U7),
    # so an operator whose gateway is down can still leave.
    #
    # ``/needs`` (v0.4 U7) is the one entry here that needs no gateway for
    # anything, not merely to be listed: the queue is derived from rows Talaria
    # already holds, so with every connection down the list still opens and still
    # says what it last knew — with each dropped connection named in it, which is
    # the state an operator most wants the list for.
    assert {entry.name for entry in catalog.local_entries} == {
        "/quit",
        "/pause",
        "/resume",
        "/speed",
        "/models",
        "/profiles",
        "/sessions",
        "/needs",
        "/agents",
        "/theme",
        "/bar",
        "/inspector",
        "/diffs",
    }


@pytest.mark.parametrize("body", ["not a mapping", None, [], 7, {"pairs": "nope"}])
def test_a_malformed_catalogue_degrades_rather_than_raising(body: object) -> None:
    catalog = decode_catalog(body)
    assert catalog.available is False
    assert catalog.failure


def test_one_malformed_row_does_not_cost_the_listing_the_others() -> None:
    reply = catalog_reply(pairs=[["/help", "Show help"], ["oops"], [7, "x"], None])
    catalog = decode_catalog(reply)
    assert [entry.name for entry in catalog.gateway_entries] == ["/help"]
    assert catalog.available is True


def test_the_gateways_own_warning_is_carried_rather_than_swallowed() -> None:
    """A catalogue built with a failed skill scan is short by every skill
    command, and the gateway says so in ``warning``. Rendering the shorter list
    without the warning is a silent lie about what the gateway offers."""
    catalog = decode_catalog(catalog_reply(warning="skill discovery unavailable: boom"))
    assert catalog.available is True
    assert catalog.warning == "skill discovery unavailable: boom"


def test_a_commands_keyed_catalogue_carries_metadata_without_merging_it() -> None:
    """U1 (#119) established ``commands`` is a metadata map, not pair rows.

    Merging it into the listing would advertise dispatches the gateway never
    offered (the alias-merge KTD1 forbids); dropping it would unread the
    gateway's own shape. It rides along untouched while ``pairs`` stays the
    only source of rows — including a metadata-only name, which must never
    become an entry.
    """
    meta = {"/help": "Show help", "/model": "Pick a model", "/meta-only": "nowhere in pairs"}
    catalog = decode_catalog(catalog_reply(commands=meta))
    plain = decode_catalog(catalog_reply())

    assert catalog.available is True
    assert dict(catalog.commands_meta) == meta
    assert [entry.name for entry in catalog.entries] == [
        entry.name for entry in plain.entries
    ]
    assert catalog.entry_for("/meta-only") is None
    assert [entry.name for entry in catalog.gateway_entries] == [
        entry.name for entry in plain.gateway_entries
    ]


@pytest.mark.parametrize(
    "commands",
    [
        None,
        "nope",
        7,
        ["not", "a", "mapping"],
        {},
        {"": "empty key goes nowhere", "/ok": "kept", 7: "non-string key dropped"},
        {"/null-row": None, "/nested-row": {"description": "x"}, "/ok": "kept"},
    ],
)
def test_a_malformed_commands_map_degrades_rather_than_raising(commands: object) -> None:
    """Every wrong shape of the metadata map decodes without raising.

    Junk rows never promote to dispatchable entries; string rows still land.
    """
    catalog = decode_catalog(catalog_reply(commands=commands, categories="nope"))

    assert catalog.available is True
    # Junk never reaches the listing under any malformed shape …
    assert catalog.entry_for("/ok") is None
    # … and only string rows survive in the metadata.
    assert all(
        isinstance(key, str) and isinstance(value, str)
        for key, value in catalog.commands_meta.items()
    )
    if (
        isinstance(commands, dict)
        and isinstance(commands.get("/ok"), str)
    ):
        assert catalog.commands_meta["/ok"] == "kept"
    else:
        assert "/ok" not in catalog.commands_meta


def test_a_commands_map_with_many_rows_decodes_without_a_bound() -> None:
    """The live map carries over a hundred rows; size alone is not malformation."""
    meta = {f"/cmd-{index}": f"command {index}" for index in range(5000)}
    catalog = decode_catalog(catalog_reply(commands=meta))

    assert catalog.available is True
    assert len(catalog.commands_meta) == 5000


def test_a_commands_map_without_pairs_is_still_an_unreadable_catalogue() -> None:
    """Metadata without a listing is genuinely odd, not a listing by another name."""
    catalog = decode_catalog({"commands": {"/help": "Show help"}})

    assert catalog.available is False
    assert catalog.failure


def test_an_alias_resolves_through_the_gateways_own_canon_map() -> None:
    catalog = decode_catalog(catalog_reply())
    assert catalog.canonical("/h") == "/help"
    assert catalog.canonical("/H") == "/help"
    # An unknown name passes through: the gateway's registry is the authority,
    # and its catalogue omits whole classes of command.
    assert catalog.canonical("/unlisted") == "/unlisted"


def test_the_local_set_shadows_a_gateway_command_of_the_same_name() -> None:
    """PC6's four are Talaria's. A gateway that ships its own ``/quit`` must
    not be able to take the operator's exit away, and must not be listed twice
    advertising a dispatch that never happens."""
    catalog = decode_catalog(catalog_reply(pairs=[["/quit", "Gateway's own quit"]]))
    matches = [entry for entry in catalog.entries if entry.name == "/quit"]
    assert len(matches) == 1
    assert matches[0].availability == "talaria-local"


def test_ktd6_the_local_sessions_command_shadows_the_registrys_dispatchable_one() -> None:
    """KTD6, asserted against the catalogue this module's own docstring cites.

    The registry defines ``CommandDef("sessions", "Browse and resume previous
    sessions", "Session")`` (``hermes_cli/commands.py:180`` at ``7f4d15515``)
    — a real, dispatchable, non-``TUI`` entry, not the four-name TUI-extra
    collision :func:`test_a_registry_command_reusing_a_tui_name_is_not_marked_unsupported`
    covers. Built from that exact citation rather than an invented fixture, so
    a future catalogue change that actually removed the registry row (leaving
    nothing here to shadow) would not silently make this test meaningless —
    the reply still carries the row, and what is asserted is that Talaria
    never reaches it.
    """
    reply = catalog_reply(
        pairs=[["/sessions", "Browse and resume previous sessions"]],
        categories=[
            {
                "name": "Session",
                "pairs": [["/sessions", "Browse and resume previous sessions"]],
            }
        ],
    )
    catalog = decode_catalog(reply)

    # The listing itself: one row, shadowed rather than duplicated.
    matches = [entry for entry in catalog.entries if entry.name == "/sessions"]
    assert len(matches) == 1
    assert matches[0].availability == "talaria-local"

    # Resolution: a typed ``/sessions`` never reaches ``GatewayInvocation`` —
    # the local set is checked before the catalogue (PC6's rule, extended by
    # U7) — even though the catalogue above lists a real, dispatchable row
    # under the row's own real category.
    resolution = resolve_command("/sessions", catalog)
    assert isinstance(resolution, LocalInvocation)
    assert resolution.command.action == "sessions"

    # And the same holds with no catalogue at all — the gateway being
    # unreachable must not un-shadow the name.
    assert isinstance(resolve_command("/sessions", None), LocalInvocation)


# ── parsing (what is and is not a command) ───────────────────────────────


@pytest.mark.parametrize(
    ("text", "name", "argument"),
    [
        ("/help", "/help", ""),
        ("  /help  ", "/help", ""),
        ("/HELP", "/help", ""),
        ("/model gpt-5", "/model", "gpt-5"),
        ("/goal   set the thing  ", "/goal", "set the thing"),
        ("/set-home", "/set-home", ""),
    ],
)
def test_a_command_line_splits_into_a_name_and_an_argument(
    text: str, name: str, argument: str
) -> None:
    parsed = parse_command_line(text)
    assert parsed is not None
    assert (parsed.name, parsed.argument) == (name, argument)


@pytest.mark.parametrize(
    "text",
    [
        "",
        "   ",
        "just a message",
        "/usr/bin/env is missing",  # a path, not a command named "usr"
        "//comment",
        "/9lives",
        "hey /help",
        "/help me\nand also this",  # multi-line: a message that starts oddly
    ],
)
def test_ordinary_text_is_not_mistaken_for_a_command(text: str) -> None:
    assert parse_command_line(text) is None


def test_a_pasted_script_beginning_with_a_slash_stays_a_message() -> None:
    """Dispatching the first word of a 400-line paste and discarding the rest
    is the worst available reading of it."""
    pasted = "/etc/hosts is wrong\n" + "\n".join(f"line {n}" for n in range(400))
    assert parse_command_line(pasted) is None
    assert resolve_command(pasted, decode_catalog(catalog_reply())) is None


# ── resolution ───────────────────────────────────────────────────────────


def test_the_local_set_is_resolved_before_the_catalogue_is_consulted() -> None:
    """PC6. Resolved even against a catalogue that claims the name, and even
    when there is no catalogue at all."""
    for catalog in (None, decode_catalog(catalog_reply(pairs=[["/pause", "gateway's"]]))):
        resolution = resolve_command("/pause", catalog)
        assert isinstance(resolution, LocalInvocation)
        assert resolution.command.action == "pause"


def test_an_alias_the_gateway_resolves_onto_a_local_name_stays_local() -> None:
    """``/exit`` is the registry's own alias for ``/quit``
    (``hermes_cli/commands.py:330-331`` at ``7f4d15515``), so a real catalogue
    carries ``canon["/exit"] = "/quit"``.

    Checking only the *typed* name let ``/exit`` resolve to ``/quit`` and then
    leave over the socket, where the gateway does not implement it: the operator
    typed the exit command and did not exit. The listing-level shadow does not
    close this — the alias never appears as a row.
    """
    catalog = decode_catalog(
        catalog_reply(
            pairs=[["/quit", "Exit the CLI"]],
            categories=[{"name": "Exit", "pairs": [["/quit", "Exit the CLI"]]}],
            canon={"/quit": "/quit", "/exit": "/quit"},
        )
    )
    assert catalog.canonical("/exit") == "/quit", "the fixture stopped carrying the alias"
    resolution = resolve_command("/exit", catalog)
    assert isinstance(resolution, LocalInvocation)
    assert resolution.command.action == "quit"


def test_an_unsupported_entry_resolves_to_a_refusal_not_a_dispatch() -> None:
    resolution = resolve_command("/density on", decode_catalog(catalog_reply()))
    assert isinstance(resolution, UnsupportedInvocation)
    assert resolution.name == "/density"
    assert "gateway" in resolution.reason


def test_a_gateway_command_resolves_through_its_alias_and_keeps_its_argument() -> None:
    resolution = resolve_command("/h me", decode_catalog(catalog_reply()))
    assert isinstance(resolution, GatewayInvocation)
    assert (resolution.name, resolution.argument) == ("/help", "me")
    assert resolution.listed is True
    # ``command.dispatch`` strips the slash itself (``methods_tools.py:434``);
    # sending it stripped keeps the request identical to the compat fixture.
    assert resolution.wire_name == "help"


def test_a_command_the_catalogue_never_listed_is_still_sent_and_flagged() -> None:
    """The gateway's registry is the authority. Refusing to send an unlisted
    name would make Talaria's copy of the catalogue outrank it."""
    resolution = resolve_command("/undocumented", decode_catalog(catalog_reply()))
    assert isinstance(resolution, GatewayInvocation)
    assert resolution.listed is False


def test_dispatch_has_exactly_one_method_and_it_is_the_pinned_one() -> None:
    assert DISPATCH_METHOD == "command.dispatch"


# ── the local control set ────────────────────────────────────────────────


def test_the_local_set_includes_the_theme_picker_and_explicit_save_surface() -> None:
    """The whole Talaria-local set, enumerated so an addition is deliberate.

    ``/needs`` joins in v0.4's U7 and is the only member that shadows nothing:
    the gateway's registry holds 91 command names at the pinned read
    (``hermes_cli/commands.py``, checked 2026-08-18) and none of them is
    ``needs``. The name was checked rather than assumed, because the plan
    reserved ``/needs-you`` as a fallback for exactly the collision that turned
    out not to exist.
    """
    assert {command.name for command in TALARIA_LOCAL_COMMANDS} == {
        "/quit",
        "/pause",
        "/resume",
        "/speed",
        "/models",
        "/profiles",
        "/sessions",
        "/needs",
        "/agents",
        "/theme",
        "/bar",
        "/inspector",
        "/diffs",
    }


def test_theme_and_its_save_action_always_resolve_locally() -> None:
    picker = resolve_command("/theme", None)
    save = resolve_command("/theme save repository", None)

    assert isinstance(picker, LocalInvocation)
    assert picker.command.action == "theme"
    assert picker.argument == ""
    assert isinstance(save, LocalInvocation)
    assert save.command.action == "theme"
    assert save.argument == "save repository"


def test_inspector_always_resolves_locally() -> None:
    invocation = resolve_command("/inspector", None)

    assert isinstance(invocation, LocalInvocation)
    assert invocation.command.action == "inspector"
    assert invocation.argument == ""


def test_diffs_always_resolves_locally() -> None:
    invocation = resolve_command("/diffs", None)

    assert isinstance(invocation, LocalInvocation)
    assert invocation.command.action == "diffs"
    assert invocation.argument == ""


def test_status_bar_toggles_always_resolve_locally() -> None:
    resolution = resolve_command("/bar context", None)

    assert isinstance(resolution, LocalInvocation)
    assert resolution.command.action == "bar"
    assert resolution.argument == "context"


def test_needs_is_free_of_the_gateway_registry_unlike_sessions() -> None:
    """``/sessions`` shadows a real gateway command and ``/needs`` shadows none.

    Both facts are asserted together because the pair is the point: this module
    already accepts one deliberate shadow (KTD6) and the listing marks it, so a
    reader has to be able to tell which local names carry that cost and which do
    not. ``CLIENT_LOCAL_NAMES`` is the gateway's own set of client-local extras;
    ``/needs`` is in neither it nor the dispatchable registry.
    """
    assert "/sessions" in CLIENT_LOCAL_NAMES
    assert "/needs" not in CLIENT_LOCAL_NAMES


def test_both_picker_commands_are_plural_and_the_gateway_owns_the_singulars() -> None:
    """The one-character difference between two destinations, pinned (U2, U4).

    ``/model`` and ``/profile`` are gateway commands — probed live on
    2026-08-06 among the catalogue's 114 names. Talaria takes only the plurals,
    so neither local command can shadow a working gateway one.
    """
    names = {command.name for command in TALARIA_LOCAL_COMMANDS}
    assert {"/models", "/profiles"} <= names
    assert not ({"/model", "/profile"} & names)


def test_only_the_pacing_three_are_replay_only() -> None:
    """``/quit``, ``/models``, ``/profiles`` and ``/sessions`` work in both
    modes; the pacing three scale a recorded clock a live session does not
    have."""
    replay_only = {
        command.name for command in TALARIA_LOCAL_COMMANDS if command.replay_only
    }
    assert replay_only == {"/pause", "/resume", "/speed"}


def test_an_unknown_name_is_not_a_local_command() -> None:
    assert local_command("/help") is None


@pytest.mark.parametrize(
    ("argument", "expected"),
    [("4", 4.0), ("0.5", 0.5), ("max", math.inf), ("MAX", math.inf), ("inf", math.inf)],
)
def test_speed_reads_a_rate(argument: str, expected: float) -> None:
    assert parse_speed(argument) == expected


@pytest.mark.parametrize("argument", ["", "fast", "-2", "0", "nan", "1x"])
def test_speed_refuses_a_non_rate_rather_than_substituting_one(argument: str) -> None:
    """Substituting 1x for a typo changes the replay rate to something the
    operator did not ask for and says nothing about it."""
    assert parse_speed(argument) is None


# ── generic result routing (R24) ─────────────────────────────────────────


def test_every_shape_with_the_same_fields_renders_the_same_way() -> None:
    """The structural claim behind "no gateway command gets a bespoke
    interface": :func:`render_dispatch` never reads ``type``, so six results
    that differ *only* in their type are indistinguishable downstream. A single
    ``if result.type == …`` branch anywhere in the router breaks this."""
    renderings = {
        render_dispatch(
            "/x",
            DispatchResult(type=shape, display_text="same", notice="n"),  # type: ignore[arg-type]
        )
        for shape in sorted(DISPATCH_RESULT_TYPES)
    }
    assert len(renderings) == 1


def test_a_skills_display_reaches_the_transcript_and_its_scaffold_does_not() -> None:
    rendering = render_dispatch(
        "/deploy",
        DispatchResult(type="skill", display_text="⚡ /deploy", submit_text=SCAFFOLD),
    )
    assert rendering.transcript_line == "/deploy: ⚡ /deploy"
    assert SCAFFOLD not in rendering.transcript_line
    assert rendering.submit_text == SCAFFOLD


def test_a_bundles_display_reaches_the_transcript_and_its_scaffold_does_not() -> None:
    rendering = render_dispatch(
        "/release",
        DispatchResult(
            type="send",
            display_text="⚡ Loading bundle: release (3 skills)",
            submit_text=SCAFFOLD,
            notice="⚡ Loading bundle: release (3 skills)",
        ),
    )
    assert "release (3 skills)" in rendering.transcript_line
    assert SCAFFOLD not in rendering.transcript_line
    assert rendering.submit_text == SCAFFOLD


def test_a_prefill_goes_to_the_composer_and_is_not_printed_twice() -> None:
    """A ``/goal`` prefill is routinely a couple of thousand characters.
    Printing it into the transcript as well makes the copy the operator cannot
    edit the larger of the two."""
    body = "rewrite the ingest pipeline " * 60
    rendering = render_dispatch(
        "/goal", DispatchResult(type="prefill", display_text=body, prefill_text=body)
    )
    assert rendering.prefill_text == body
    assert rendering.transcript_line == f"/goal: {PREFILLED_INTO_COMPOSER}"
    assert body not in rendering.transcript_line
    assert rendering.submit_text is None


def test_an_empty_output_says_so_rather_than_rendering_a_bare_name() -> None:
    rendering = render_dispatch("/noop", DispatchResult(type="exec", display_text="   "))
    assert rendering.transcript_line == f"/noop: {NO_DISPLAY_TEXT}"


def test_an_unknown_shape_is_surfaced_by_name_and_carries_nothing_onward() -> None:
    """R5's discipline applied to results: a seventh shape is named, not
    guessed at, and nothing of it is submitted or prefilled."""
    rendering = render_dispatch("/x", UnknownDispatchResult(type="hologram"))
    assert UNKNOWN_RESULT_NOTICE in rendering.transcript_line
    assert "hologram" in rendering.transcript_line
    assert rendering.submit_text is None
    assert rendering.prefill_text is None


def test_a_very_long_output_is_clipped_with_the_cut_marked() -> None:
    rendering = render_dispatch(
        "/logdump", DispatchResult(type="exec", display_text="x" * 9000)
    )
    assert rendering.transcript_line.endswith("…")
    assert len(rendering.transcript_line) < 4200


def test_a_send_with_no_display_renders_its_message(
) -> None:
    """The honest statement of what the ``send`` branch does, pinned.

    A bundle carries ``display`` and that projection is what is rendered. A
    *plain* ``send`` has no projection at all: ``/queue`` returns the operator's
    own argument as ``message`` (``methods_tools.py:573``), and ``/learn`` and
    ``/init`` return a built prompt (``:582``, ``:590``). Hermes's own client
    renders ``message`` in that case — ``shown ? send(message, true, shown) :
    send(message)`` (``createSlashHandler.ts:110-114``) — and Talaria does the
    same, because the alternative is a branch on the command name, which is the
    one design this unit forbids. The bound below is what keeps a built prompt
    from displacing the conversation.

    This test exists because a ``DECISIONS.md`` entry claimed the opposite.
    """
    body = "the built prompt " * 400
    rendering = render_dispatch(
        "/learn", DispatchResult(type="send", display_text=body, submit_text=body)
    )
    assert rendering.transcript_line.startswith("/learn: the built prompt")
    assert rendering.submit_text == body
    assert len(rendering.transcript_line) <= COMMAND_OUTPUT_CLIP + len("/learn: …")


# ── ``slash.exec``: the route most of the registry takes ─────────────────


def test_slash_exec_is_the_pinned_method_name() -> None:
    assert SLASH_EXEC_METHOD == "slash.exec"


def test_the_command_string_carries_the_name_and_the_argument_without_a_slash() -> None:
    """``slash.exec`` takes one ``command`` string and splits the name off
    itself; Hermes's client sends ``cmd.slice(1)``
    (``createSlashHandler.ts:147``)."""
    invocation = GatewayInvocation(name="/model", argument="sonnet --verbose")
    assert slash_exec_command(invocation) == "model sonnet --verbose"
    assert slash_exec_command(GatewayInvocation(name="/status", argument="")) == "status"


def test_plain_worker_output_decodes_as_output_not_as_a_shape() -> None:
    decoded = decode_slash_exec({"output": "model: sonnet", "warning": "config written"})
    assert decoded == SlashOutput(text="model: sonnet", warning="config written")


def test_a_forwarded_dispatch_payload_decodes_as_the_shape_it_is() -> None:
    """``slash.exec`` hands a bundle or a pending-input command to
    ``command.dispatch`` and returns that payload verbatim
    (``methods_tools.py:1102-1140``), so one reply channel carries two shapes.
    The discriminator is the one Hermes's own client uses: ``typeof o.type !==
    'string'`` (``ui-tui/src/lib/rpc.ts:11``)."""
    decoded = decode_slash_exec({"type": "prefill", "message": "rewrite it"})
    assert isinstance(decoded, DispatchResult)
    assert decoded.type == "prefill"
    assert decoded.prefill_text == "rewrite it"


def test_a_forwarded_payload_of_an_unknown_shape_still_surfaces() -> None:
    decoded = decode_slash_exec({"type": "hologram"})
    assert isinstance(decoded, UnknownDispatchResult)
    assert decoded.type == "hologram"


@pytest.mark.parametrize("reply", [None, "text", 7, [], {"output": 5}])
def test_a_slash_exec_reply_that_is_not_usable_decodes_to_empty_output(
    reply: object,
) -> None:
    """R5's discipline: a reply that is not what the handler documents yields an
    empty result rather than an exception."""
    decoded = decode_slash_exec(reply)
    assert decoded == SlashOutput(text="")


def test_worker_output_reaches_the_transcript_and_its_warning_the_notice() -> None:
    rendering = render_slash_output(
        "/compress", SlashOutput(text="history compressed", warning="model switched")
    )
    assert rendering.transcript_line == "/compress: history compressed"
    assert rendering.notice == "model switched"
    assert rendering.submit_text is None
    assert rendering.prefill_text is None
    assert rendering.alias_target is None


def test_worker_output_is_bounded_like_every_other_shape() -> None:
    rendering = render_slash_output("/logdump", SlashOutput(text="x" * 9000))
    assert rendering.transcript_line.endswith("…")
    assert len(rendering.transcript_line) <= COMMAND_OUTPUT_CLIP + len("/logdump: …")


def test_empty_worker_output_says_so_rather_than_rendering_a_bare_name() -> None:
    rendering = render_slash_output("/noop", SlashOutput(text="  "))
    assert rendering.transcript_line == f"/noop: {NO_DISPLAY_TEXT}"


def test_an_alias_result_carries_its_target_so_the_caller_can_run_it() -> None:
    """An ``alias`` result names a target and runs nothing. A renderer that
    dropped the target would leave the caller with a line and no way to act on
    it, which is how a working quick command becomes a dead end that looks like
    a result."""
    rendering = render_dispatch(
        "/gs", DispatchResult(type="alias", display_text="alias → git", alias_target="git")
    )
    assert rendering.alias_target == "git"
    assert rendering.transcript_line == "/gs: alias → git"


def test_a_commands_output_is_not_clipped_to_a_transport_notes_length() -> None:
    """``record_local_note`` bounds an entry at ``TRANSCRIPT_LINE_CLIP``, which
    is a backstop against a runaway line and wrong as a bound on the output of
    ``/status``. A command's output is the thing the operator asked to see, so
    it is bounded once, by the caller, at the far looser
    ``COMMAND_OUTPUT_CLIP`` — this asserts the transcript bound is not applied a
    second time on top of it."""
    body = "line of status output\n" * 100
    state = record_command_result(SessionState(), f"/status: {body}", at=1.0)
    written = state.transcript[-1].text
    assert len(written) > TRANSCRIPT_LINE_CLIP
    assert written.count("line of status output") == 100


# ── KTD16's paste threshold ──────────────────────────────────────────────


def test_the_default_threshold_is_ktd16s_six_lines_or_512_bytes() -> None:
    threshold = PasteThreshold()
    assert (threshold.lines, threshold.byte_limit) == (6, 512)


@pytest.mark.parametrize(
    ("text", "trips"),
    [
        ("one line", False),
        ("\n".join("x" for _ in range(5)), False),
        ("\n".join("x" for _ in range(6)), True),  # at the bound, not past it
        ("x" * 511, False),
        ("x" * 512, True),
        ("é" * 256, True),  # 512 *bytes*, not characters
        ("é" * 255, False),
    ],
)
def test_either_bound_trips_the_collapse_whichever_comes_first(
    text: str, trips: bool
) -> None:
    assert PasteThreshold().trips(text) is trips


@pytest.mark.parametrize("threshold", [PasteThreshold(lines=0), PasteThreshold(lines=-3)])
def test_a_non_positive_line_bound_switches_that_half_off(
    threshold: PasteThreshold,
) -> None:
    """Re-encodes the gateway client's own guard
    (``useComposerState.ts:277-280``: ``pasteCollapseLines > 0 && …``). Read
    the other way, ``…_LINES=0`` means "collapse at zero lines" and sends every
    one-word paste on a round trip."""
    assert threshold.trips("hi") is False
    # The byte bound is untouched by the line bound being off.
    assert threshold.trips("x" * 512) is True


def test_both_bounds_off_collapses_nothing() -> None:
    threshold = PasteThreshold(lines=0, byte_limit=0)
    assert threshold.trips("\n".join("x" * 200 for _ in range(500))) is False


# ── the collapse reply ───────────────────────────────────────────────────


def test_a_collapse_reply_is_read_into_a_placeholder_and_a_path() -> None:
    collapsed = decode_collapsed_paste(
        {"placeholder": "[Pasted text #1: 400 lines → /p/1.txt]", "path": "/p/1.txt", "lines": 400}
    )
    assert collapsed == CollapsedPaste(
        placeholder="[Pasted text #1: 400 lines → /p/1.txt]", path="/p/1.txt", lines=400
    )


@pytest.mark.parametrize(
    "body",
    [None, "text", {}, {"path": "/p/1.txt"}, {"placeholder": ""}, {"placeholder": 7}],
)
def test_a_reply_with_no_placeholder_is_not_a_partial_success(body: object) -> None:
    """Without the stand-in there is nothing to put in the composer, so this
    has to reach the caller's failure path rather than a half-applied one."""
    assert decode_collapsed_paste(body) is None


def test_a_catalogue_with_no_entries_still_answers_lookups() -> None:
    empty = CommandCatalog()
    assert empty.entry_for("/help") is None
    assert empty.canonical("/help") == "/help"


def test_needs_resolves_locally_even_if_a_gateway_ever_advertises_it() -> None:
    """The shadowing precedence, pinned before there is anything to shadow.

    ``/needs`` is free of the gateway's registry today, so the ordinary
    resolution is uninteresting. What this asserts is the case that would arrive
    without warning: a Hermes release adding its own ``/needs``. Talaria's local
    entry wins — the same precedence ``/sessions`` relies on deliberately — which
    means the gateway's would become unreachable from Talaria, silently, unless
    someone had written down that this is what happens.

    The listing's ``local`` marker is the only place an operator could notice, so
    it is asserted here beside the resolution rather than left implied.
    """
    reply = catalog_reply(pairs=[["/help", "Show help"], ["/needs", "Something else entirely"]])
    catalog = decode_catalog(reply)

    resolution = resolve_command("/needs", catalog)
    assert isinstance(resolution, LocalInvocation), (
        "a gateway /needs displaced Talaria's own, so the needs-you list became "
        "unreachable"
    )
    assert resolution.command.action == "needs"

    entry = catalog.entry_for("/needs")
    assert entry is not None and entry.availability == "talaria-local", (
        "the collision is invisible in the listing, which is the one place it "
        "could be noticed"
    )


# ── #121 picked-line funnel parity ─────────────────────────────────────────
#
# The picker dispatches the bare entry name with no pre-resolution, so these
# pin what that wiring relies on: a bare pick resolves exactly like its typed
# twins, the confirmation second act survives the funnel intact, and an
# unsupported pick refuses by name.


def test_121_a_bare_pick_resolves_like_its_typed_twins() -> None:
    """#121 U2: ``/model``, ``/model ``, and ``/MODEL`` are one invocation."""
    catalog = decode_catalog(catalog_reply())
    picked = resolve_command("/model", catalog)
    assert isinstance(picked, GatewayInvocation)
    assert picked == resolve_command("/model ", catalog)
    assert picked == resolve_command("/MODEL", catalog)
    assert (picked.name, picked.argument, picked.listed) == ("/model", "", True)


def test_121_the_confirm_word_survives_the_funnel() -> None:
    """#121 U2: the indexed resend shape reaches the gate with ``confirm`` intact.

    :meth:`_perform_models` keys its second act on the literal word; a funnel
    that trimmed or reordered the argument would silently turn the confirm
    into another first act.
    """
    first = resolve_command("/models 1 default", None)
    second = resolve_command("/models 1 default confirm", None)
    assert isinstance(first, LocalInvocation)
    assert isinstance(second, LocalInvocation)
    assert first.argument == "1 default"
    assert second.argument == "1 default confirm"


def test_121_an_unsupported_pick_refuses_by_name() -> None:
    """#121 U1: a stale pick that went unsupported is a refusal, never a dispatch."""
    resolution = resolve_command("/density", decode_catalog(catalog_reply()))
    assert isinstance(resolution, UnsupportedInvocation)
    assert resolution.name == "/density"


def test_121_a_local_pick_resolves_before_the_catalogue() -> None:
    """#121 U2: PC6 precedence holds for picks — ``/bar`` never reaches the socket."""
    catalog = decode_catalog(catalog_reply(pairs=[["/bar", "Gateway's own bar"]]))
    resolution = resolve_command("/bar", catalog)
    assert isinstance(resolution, LocalInvocation)
    assert resolution.command.action == "bar"
    assert resolution.argument == ""
