"""A1-1, A1-2, A1-3, A1-6 — target-scope correctness at the domain seam.

CFG-A1 D1/D2 make the ``(connection, profile)`` pair part of every request's
identity: an omitted, empty, or ``current`` profile would resolve server-side
to the dashboard's launch profile (``hermes_cli/web_server_profiles.py:31-34``
at Hermes ``3236f440``), which is exactly the misdirected-write hazard this
file pins.

Interface under test (unimplemented until CFG B2, ``dev-1-3``):

* ``talaria.domain.settings.ConfigTarget`` — frozen
  ``(connection_id, profile_name)``; refuses blank parts and the literal
  ``"current"`` with ``SettingsScopeError``.
* ``talaria.domain.settings`` pure state — ``SettingsState``,
  ``select_settings_target``, ``begin_settings_request``,
  ``apply_settings_response``, ``stage_settings_edit``,
  ``request_settings_switch`` (choices ``save``/``discard``/``stay``).
* ``talaria.domain.settings.build_config_patch`` /
  ``apply_config_patch`` — the sparse deep-merge pair behind A1-6.
* ``talaria.domain.settings_commands`` — frozen typed commands plus the pure
  ``rest_request`` / ``rpc_request`` spec builders. No I/O happens here; the
  builders return frozen request specs so A1-1's "fails before I/O" is
  observable without a socket.

Every test fails through :func:`_require_module` / :func:`_require_attr`
while the interface is absent, so the T1 log reads as missing behavior
rather than a broken harness. No test in this module touches the network,
the filesystem, or Textual (ADR-0002).
"""

from __future__ import annotations

import importlib
from typing import Any

import pytest


def _require_module(name: str) -> Any:
    """Import a B2 production module, failing as missing behavior when absent."""
    try:
        return importlib.import_module(name)
    except ImportError as exc:
        pytest.fail(f"unimplemented interface {name} (CFG v0.6.2 B2): {exc}")


def _require_attr(module: Any, name: str) -> Any:
    """Read a B2 production attribute, failing as missing behavior when absent."""
    try:
        return getattr(module, name)
    except AttributeError:
        pytest.fail(
            f"unimplemented interface {module.__name__}.{name} (CFG v0.6.2 B2)"
        )


def _settings() -> Any:
    return _require_module("talaria.domain.settings")


def _commands() -> Any:
    return _require_module("talaria.domain.settings_commands")


def _target(settings: Any, connection: str, profile: str) -> Any:
    return _require_attr(settings, "ConfigTarget")(
        connection_id=connection, profile_name=profile
    )


# ── A1-1: ConfigTarget refuses invalid scope at construction ─────────────


def test_a_selected_profile_survives_construction_verbatim() -> None:
    settings = _settings()

    target = _target(settings, "local-fixture", "alpha-fixture")

    assert target.connection_id == "local-fixture"
    assert target.profile_name == "alpha-fixture"


@pytest.mark.parametrize("profile", ["", "   ", "\t\n"])
def test_a_missing_or_blank_profile_is_refused_before_any_io(profile: str) -> None:
    """KTD1: the safest failure is local refusal before network I/O."""
    settings = _settings()
    scope_error = _require_attr(settings, "SettingsScopeError")

    with pytest.raises(scope_error):
        _target(settings, "local-fixture", profile)


@pytest.mark.parametrize("connection", ["", "   "])
def test_a_missing_connection_is_refused_before_any_io(connection: str) -> None:
    settings = _settings()
    scope_error = _require_attr(settings, "SettingsScopeError")

    with pytest.raises(scope_error):
        _target(settings, connection, "alpha-fixture")


def test_the_literal_current_profile_is_refused() -> None:
    """The server resolves ``current`` to its launch profile, so sending it
    would target whatever the dashboard started with — the null/default
    hazard D1 exists to forbid. The UI shows ``current`` separately (A1-4);
    it must never travel as the request scope."""
    settings = _settings()
    scope_error = _require_attr(settings, "SettingsScopeError")

    with pytest.raises(scope_error):
        _target(settings, "local-fixture", "current")


def test_a_target_is_frozen_and_hashable_for_use_as_a_cache_key() -> None:
    settings = _settings()
    from dataclasses import FrozenInstanceError

    target = _target(settings, "local-fixture", "alpha-fixture")

    with pytest.raises(FrozenInstanceError):
        target.profile_name = "beta-fixture"
    assert hash(target) == hash(_target(settings, "local-fixture", "alpha-fixture"))
    assert target != _target(settings, "local-fixture", "beta-fixture")
    assert target != _target(settings, "remote-fixture", "alpha-fixture")


def test_scope_refusal_is_a_value_error() -> None:
    """Callers may catch the narrow type or the builtin; both hold."""
    settings = _settings()

    assert issubclass(_require_attr(settings, "SettingsScopeError"), ValueError)


# ── A1-1: every profile-scoped command carries its scope ─────────────────
#
# The table below is the finite command inventory from CFG-A1 §12.2 plus the
# profile-explicit Reset (D1) and wake-word verbs (CFG-A1 §10.1). Each row
# names the scope mechanism the real Hermes route honors at 3236f440:
# query ``?profile=``, body identity, path identity, RPC ``params.profile``,
# or host scope (no profile by design).


def _command_cases() -> tuple[tuple[str, str, str, str], ...]:
    """(command, transport, scope mechanism, expected method/path)."""
    return (
        ("LoadTarget", "rest", "query", "GET /api/config"),
        ("SaveConfig", "rest", "query", "PUT /api/config"),
        ("SetEnv", "rest", "query", "PUT /api/env"),
        ("ClearEnv", "rest", "query", "DELETE /api/env"),
        ("RevealEnv", "rest", "query", "POST /api/env/reveal"),
        ("SetModel", "rest", "query", "POST /api/model/set"),
        ("ResetConfig", "rest", "query", "PUT /api/config"),
        ("RestartGateway", "rest", "query", "POST /api/gateway/restart"),
        ("CloneProfile", "rest", "body", "POST /api/profiles"),
        ("RenameProfile", "rest", "path", "PATCH /api/profiles/{name}"),
        ("DeleteProfile", "rest", "path", "DELETE /api/profiles/{name}"),
        ("ProbeConnection", "rest", "host", "GET /api/health"),
        ("WakeWord", "rpc", "params", "wake.start"),
        ("LiveApply", "rpc", "params", "config.set"),
    )


def _build_command(commands: Any, settings: Any, name: str) -> Any:
    """One well-formed instance of each inventoried command."""
    target = _target(settings, "local-fixture", "alpha-fixture")
    factory = _require_attr(commands, name)
    if name == "LoadTarget":
        return factory(target=target)
    if name == "SaveConfig":
        return factory(target=target, patch={"agent": {"max_turns": 40}})
    if name == "SetEnv":
        return factory(target=target, key="EXAMPLE_API_KEY", value="canary")
    if name == "ClearEnv":
        return factory(target=target, key="EXAMPLE_API_KEY")
    if name == "RevealEnv":
        return factory(target=target, key="EXAMPLE_API_KEY")
    if name == "SetModel":
        return factory(target=target, provider="example-provider", model="example-large")
    if name == "ResetConfig":
        return factory(target=target, patch={"agent": {"max_turns": 25}})
    if name == "RestartGateway":
        plan = _require_attr(settings, "RestartPlan")(
            scope="own-gateway",
            title="Restart the gateway for profile alpha-fixture",
            dashboard_note=(
                "Talaria's own connection is to the dashboard and is unaffected."
            ),
            affected_profiles=("alpha-fixture",),
        )
        return factory(target=target, plan=plan)
    if name == "CloneProfile":
        return factory(
            connection_id="local-fixture",
            source="alpha-fixture",
            name="gamma-fixture",
            clone_all=False,
            clone_channels=False,
        )
    if name == "RenameProfile":
        return factory(
            connection_id="local-fixture", name="gamma-fixture", new_name="delta-fixture"
        )
    if name == "DeleteProfile":
        return factory(connection_id="local-fixture", name="delta-fixture")
    if name == "ProbeConnection":
        return factory(connection_id="local-fixture")
    if name == "WakeWord":
        return factory(target=target, action="start")
    if name == "LiveApply":
        return factory(target=target, key="approvals.mode", value="smart")
    raise AssertionError(f"unlisted command in test inventory: {name}")


@pytest.mark.parametrize(
    ("name", "transport", "mechanism", "method_path"), _command_cases()
)
def test_every_command_builds_a_spec_carrying_explicit_scope(
    name: str, transport: str, mechanism: str, method_path: str
) -> None:
    settings = _settings()
    commands = _commands()
    command = _build_command(commands, settings, name)

    if transport == "rest":
        spec = _require_attr(commands, "rest_request")(command)
        expected_method, expected_path = method_path.split(" ", 1)
        assert spec.method == expected_method
        if "{" in expected_path:
            prefix = expected_path.split("{")[0]
            assert spec.path.startswith(prefix)
            assert spec.path != prefix, f"{name} embeds no profile name"
        else:
            assert spec.path == expected_path, f"{name} targets the wrong route"
        if mechanism == "query":
            assert spec.query["profile"] == "alpha-fixture"
        elif mechanism == "body":
            assert spec.body["name"] == "gamma-fixture"
            assert spec.body["clone_from"] == "alpha-fixture"
        elif mechanism == "path":
            assert "gamma-fixture" in spec.path or "delta-fixture" in spec.path
        elif mechanism == "host":
            assert "profile" not in spec.query
    else:
        spec = _require_attr(commands, "rpc_request")(command)
        assert spec.method == method_path
        assert spec.params["profile"] == "alpha-fixture"


@pytest.mark.parametrize(
    "name",
    [case[0] for case in _command_cases() if case[2] in ("query", "params")],
)
def test_every_profile_scoped_command_exposes_its_selected_target(
    name: str,
) -> None:
    """The scope a spec builder sends is the scope the caller selected: the
    command carries the ``ConfigTarget`` itself, not a string that could be
    re-resolved against another target later."""
    settings = _settings()
    commands = _commands()

    command = _build_command(commands, settings, name)

    assert command.target == _target(settings, "local-fixture", "alpha-fixture")


def test_wake_word_accepts_only_start_stop_and_status() -> None:
    settings = _settings()
    commands = _commands()
    target = _target(settings, "local-fixture", "alpha-fixture")
    factory = _require_attr(commands, "WakeWord")

    for action, method in (
        ("start", "wake.start"),
        ("stop", "wake.stop"),
        ("status", "wake.status"),
    ):
        spec = _require_attr(commands, "rpc_request")(
            factory(target=target, action=action)
        )
        assert spec.method == method
        assert spec.params["profile"] == "alpha-fixture"
    with pytest.raises(ValueError):
        factory(target=target, action="launch")


def test_live_apply_names_the_config_set_key_and_value() -> None:
    """``config.set`` takes ``params.key``/``params.value`` at 3236f440
    (``tui_gateway/methods_config_set.py:472``); the profile rides alongside."""
    settings = _settings()
    commands = _commands()

    spec = _require_attr(commands, "rpc_request")(
        _build_command(commands, settings, "LiveApply")
    )

    assert spec.method == "config.set"
    assert spec.params["key"] == "approvals.mode"
    assert spec.params["value"] == "smart"
    assert spec.params["profile"] == "alpha-fixture"


def test_save_put_body_is_the_sparse_patch_under_config() -> None:
    """``PUT /api/config`` takes ``{config: {...}}`` at 3236f440
    (``config_env.py:113-150``); the body holds only changed keys (A1-6)."""
    settings = _settings()
    commands = _commands()

    spec = _require_attr(commands, "rest_request")(
        _build_command(commands, settings, "SaveConfig")
    )

    assert spec.query["profile"] == "alpha-fixture"
    assert spec.body == {"config": {"agent": {"max_turns": 40}}}


# ── A1-2: stale and foreign replies never land on the selected target ─────


def _state_helpers(settings: Any) -> tuple[Any, Any, Any, Any, Any]:
    return (
        _require_attr(settings, "SettingsState"),
        _require_attr(settings, "select_settings_target"),
        _require_attr(settings, "begin_settings_request"),
        _require_attr(settings, "apply_settings_response"),
        _require_attr(settings, "stage_settings_edit"),
    )


def test_a_reply_for_the_previous_profile_is_dropped_after_a_switch() -> None:
    settings = _settings()
    state_type, select, begin, apply, _ = _state_helpers(settings)
    old = _target(settings, "local-fixture", "alpha-fixture")
    new = _target(settings, "local-fixture", "beta-fixture")

    state = select(state_type.empty(), old)
    state, generation = begin(state, old)
    state = select(state, new)
    after = apply(
        state,
        target=old,
        generation=generation,
        saved={"agent": {"max_turns": 40}},
        effective={"agent": {"max_turns": 40}},
    )

    assert after is state or after == state
    # The new target has no document from the old target's reply.
    documents = after.documents
    assert new not in documents


def test_a_reply_for_the_same_profile_on_another_connection_is_dropped() -> None:
    """``(c2, p1)`` after selection moved: the key is the whole pair, not the
    profile name — two connections may serve same-named profiles."""
    settings = _settings()
    state_type, select, begin, apply, _ = _state_helpers(settings)
    local = _target(settings, "local-fixture", "alpha-fixture")
    remote = _target(settings, "remote-fixture", "alpha-fixture")

    state = select(state_type.empty(), local)
    state, generation = begin(state, local)
    state = select(state, remote)
    after = apply(
        state,
        target=local,
        generation=generation,
        saved={"agent": {"max_turns": 40}},
        effective={"agent": {"max_turns": 40}},
    )

    assert after.documents.get(remote) is None
    assert after.documents.get(local) is None


def test_a_late_first_reply_is_dropped_once_a_second_request_began() -> None:
    """Generations disambiguate two in-flight requests for one target."""
    settings = _settings()
    state_type, select, begin, apply, _ = _state_helpers(settings)
    target = _target(settings, "local-fixture", "alpha-fixture")

    state = select(state_type.empty(), target)
    state, first = begin(state, target)
    state, _second = begin(state, target)
    after = apply(
        state,
        target=target,
        generation=first,
        saved={"agent": {"max_turns": 40}},
        effective={"agent": {"max_turns": 40}},
    )

    assert after.documents.get(target) is None


def test_a_current_reply_for_the_selected_target_lands() -> None:
    """The positive control: the drop rules above must not swallow the live
    response the workspace is actually waiting for."""
    settings = _settings()
    state_type, select, begin, apply, _ = _state_helpers(settings)
    target = _target(settings, "local-fixture", "alpha-fixture")

    state = select(state_type.empty(), target)
    state, generation = begin(state, target)
    after = apply(
        state,
        target=target,
        generation=generation,
        saved={"agent": {"max_turns": 40}},
        effective={"agent": {"max_turns": 40}},
    )

    document = after.documents[target]
    assert document.saved == {"agent": {"max_turns": 40}}
    assert document.effective == {"agent": {"max_turns": 40}}
    assert document.target == target


# ── A1-3: Save to old, Discard, or Stay — never carried over ───────────────


def test_stay_keeps_edits_on_the_old_target_and_does_not_switch() -> None:
    settings = _settings()
    state_type, select, _, _, stage = _state_helpers(settings)
    switch = _require_attr(settings, "request_settings_switch")
    old = _target(settings, "local-fixture", "alpha-fixture")
    new = _target(settings, "local-fixture", "beta-fixture")

    state = select(state_type.empty(), old)
    state = stage(state, target=old, key="agent.max_turns", value=40)
    after, issued = switch(state, new_target=new, choice="stay")

    assert after.selected == old
    assert after.pending[old] != {}
    assert after.pending.get(new) is None
    assert issued == ()


def test_discard_removes_old_edits_and_switches() -> None:
    settings = _settings()
    state_type, select, _, _, stage = _state_helpers(settings)
    switch = _require_attr(settings, "request_settings_switch")
    old = _target(settings, "local-fixture", "alpha-fixture")
    new = _target(settings, "local-fixture", "beta-fixture")

    state = select(state_type.empty(), old)
    state = stage(state, target=old, key="agent.max_turns", value=40)
    after, issued = switch(state, new_target=new, choice="discard")

    assert after.selected == new
    assert after.pending.get(old) is None
    assert after.pending.get(new) is None
    assert issued == ()


def test_save_issues_a_write_to_the_old_target_only() -> None:
    settings = _settings()
    commands = _commands()
    state_type, select, _, _, stage = _state_helpers(settings)
    switch = _require_attr(settings, "request_settings_switch")
    save_type = _require_attr(commands, "SaveConfig")
    old = _target(settings, "local-fixture", "alpha-fixture")
    new = _target(settings, "local-fixture", "beta-fixture")

    state = select(state_type.empty(), old)
    state = stage(state, target=old, key="agent.max_turns", value=40)
    after, issued = switch(state, new_target=new, choice="save")

    # P2-1 KTD1: Save completes before selection changes. The switch is a
    # pending continuation, committed only when the old target re-read
    # succeeds — the header must not show the new target while the old
    # target's write is unresolved.
    assert after.selected == old
    assert hasattr(after, "pending_target"), (
        "SettingsState carries no pending_target continuation (P2-1)"
    )
    assert after.pending_target == new
    assert old in after.awaiting_save
    assert len(issued) == 1
    assert isinstance(issued[0], save_type)
    assert issued[0].target == old
    assert issued[0].patch == {"agent": {"max_turns": 40}}


def test_save_reread_commits_the_pending_switch() -> None:
    """P2-1: the old target re-read lands, clears awaiting/pending, and only
    then moves selection to the pending target."""
    settings = _settings()
    state_type, select, begin, apply, stage = _state_helpers(settings)
    switch = _require_attr(settings, "request_settings_switch")
    old = _target(settings, "local-fixture", "alpha-fixture")
    new = _target(settings, "local-fixture", "beta-fixture")

    state = select(state_type.empty(), old)
    state = stage(state, target=old, key="agent.max_turns", value=40)
    state, _issued = switch(state, new_target=new, choice="save")
    assert hasattr(state, "pending_target"), (
        "SettingsState carries no pending_target continuation (P2-1)"
    )
    state, generation = begin(state, old)
    after = apply(
        state,
        target=old,
        generation=generation,
        saved={"agent": {"max_turns": 40}},
        effective={"agent": {"max_turns": 40}},
    )

    assert after.selected == new
    assert after.pending_target is None
    assert old not in after.awaiting_save
    assert after.pending.get(old) is None
    assert after.documents[old].saved == {"agent": {"max_turns": 40}}


def test_failed_save_abandons_the_switch_and_retains_edits() -> None:
    """P2-1: a failed Save clears awaiting/pending, keeps selection and the
    staged edits — the operator sees the old target with work intact."""
    settings = _settings()
    state_type, select, _, _, stage = _state_helpers(settings)
    switch = _require_attr(settings, "request_settings_switch")
    fail = _require_attr(settings, "fail_settings_save")
    old = _target(settings, "local-fixture", "alpha-fixture")
    new = _target(settings, "local-fixture", "beta-fixture")

    state = select(state_type.empty(), old)
    state = stage(state, target=old, key="agent.max_turns", value=40)
    state, _issued = switch(state, new_target=new, choice="save")
    after = fail(state, target=old)

    assert after.selected == old
    assert getattr(after, "pending_target", None) is None
    assert old not in after.awaiting_save
    assert after.pending.get(old) == {"agent.max_turns": 40}


def test_a_stale_reread_neither_lands_nor_commits() -> None:
    """P2-1: an older-generation re-read is dropped with awaiting retained,
    so the newer in-flight re-read still completes the switch."""
    settings = _settings()
    state_type, select, begin, apply, stage = _state_helpers(settings)
    switch = _require_attr(settings, "request_settings_switch")
    old = _target(settings, "local-fixture", "alpha-fixture")
    new = _target(settings, "local-fixture", "beta-fixture")

    state = select(state_type.empty(), old)
    state = stage(state, target=old, key="agent.max_turns", value=40)
    state, _issued = switch(state, new_target=new, choice="save")
    assert hasattr(state, "pending_target"), (
        "SettingsState carries no pending_target continuation (P2-1)"
    )
    state, first = begin(state, old)
    state, _second = begin(state, old)
    after = apply(
        state,
        target=old,
        generation=first,
        saved={"agent": {"max_turns": 40}},
        effective={"agent": {"max_turns": 40}},
    )

    assert after.selected == old
    assert after.pending_target == new
    assert old in after.awaiting_save
    assert after.documents.get(old) is None


def test_a_reread_without_a_pending_switch_lands_in_place() -> None:
    """P2-1: same-target footer saves await a re-read with no pending
    target; it lands, clears awaiting, and selection never moves."""
    from dataclasses import replace

    settings = _settings()
    state_type, select, begin, apply, stage = _state_helpers(settings)
    old = _target(settings, "local-fixture", "alpha-fixture")

    state = select(state_type.empty(), old)
    state = stage(state, target=old, key="agent.max_turns", value=40)
    state = replace(state, awaiting_save=frozenset({old}))
    assert getattr(state, "pending_target", None) is None
    state, generation = begin(state, old)
    after = apply(
        state,
        target=old,
        generation=generation,
        saved={"agent": {"max_turns": 40}},
        effective={"agent": {"max_turns": 40}},
    )

    assert after.selected == old
    assert getattr(after, "pending_target", None) is None
    assert old not in after.awaiting_save
    assert after.documents[old].saved == {"agent": {"max_turns": 40}}


def test_an_unknown_switch_choice_is_refused() -> None:
    settings = _settings()
    state_type, select, _, _, _ = _state_helpers(settings)
    switch = _require_attr(settings, "request_settings_switch")

    state = select(
        state_type.empty(), _target(settings, "local-fixture", "alpha-fixture")
    )

    with pytest.raises(ValueError):
        switch(
            state,
            new_target=_target(settings, "local-fixture", "beta-fixture"),
            choice="carry",
        )


# ── A1-6: sparse patches preserve unrelated saved values ──────────────────


def test_building_a_patch_keeps_only_changed_keys() -> None:
    settings = _settings()
    build = _require_attr(settings, "build_config_patch")

    patch = build(
        saved={"agent": {"max_turns": 25, "api_max_retries": 3}},
        edits={"agent.max_turns": 40},
    )

    assert patch == {"agent": {"max_turns": 40}}


def test_an_edit_equal_to_the_saved_value_produces_no_patch_entry() -> None:
    """Re-typing the current value is not a change; the PUT stays sparse."""
    settings = _settings()
    build = _require_attr(settings, "build_config_patch")

    patch = build(
        saved={"agent": {"max_turns": 25}},
        edits={"agent.max_turns": 25},
    )

    assert patch == {}


def test_applying_a_patch_preserves_every_unchanged_saved_value() -> None:
    """The sparse on-disk case ``config_env.py:120-135`` documents: defaults
    fill the GET, but the PUT must not materialize them into the file."""
    settings = _settings()
    build = _require_attr(settings, "build_config_patch")
    apply = _require_attr(settings, "apply_config_patch")
    saved = {"agent": {"max_turns": 25}, "browser": {"use_real_profile": True}}

    patch = build(saved=saved, edits={"agent.max_turns": 40})
    reread = apply(saved=saved, patch=patch)

    assert reread == {
        "agent": {"max_turns": 40},
        "browser": {"use_real_profile": True},
    }
    assert saved == {
        "agent": {"max_turns": 25},
        "browser": {"use_real_profile": True},
    }


def test_a_nested_patch_merges_rather_than_replacing_the_parent_table() -> None:
    settings = _settings()
    apply = _require_attr(settings, "apply_config_patch")

    reread = apply(
        saved={"agent": {"max_turns": 25, "api_max_retries": 3}},
        patch={"agent": {"max_turns": 40}},
    )

    assert reread == {"agent": {"max_turns": 40, "api_max_retries": 3}}
