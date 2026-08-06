"""U2's model picker (2026-08-06 model-picker plan): rendering and selection.

The pure functions in ``talaria/ui/picker.py`` are asserted directly, without a
screen, the same way ``talaria/ui/palette.py``'s ``format_entry``/``header_line``
are. Selection and focus are asserted through the real
:class:`~talaria.ui.app.TalariaApp`, mirroring ``tests/ui/test_live_wiring.py``:
the dispatcher is a double, so the outcome is chosen rather than provoked, and
the admin client is a double for the same reason — neither needs a socket to
prove what Talaria does with the answer.
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass, field
from typing import Any

import pytest

from talaria.domain.commands import CATALOG_METHOD, SLASH_EXEC_METHOD
from talaria.domain.models_catalog import (
    ModelAssignmentResult,
    ModelProvider,
    ProfileDirectory,
    ProfileEntry,
    ProviderCatalog,
)
from talaria.replay.controls import ReplayControls
from talaria.replay.source import ReplaySource
from talaria.transport.admin import AdminError
from talaria.transport.rpc import RpcOutcome
from talaria.transport.source import SwitchReport
from talaria.ui.app import (
    MODEL_DEFAULT_FAILED,
    MODEL_DEFAULT_NEW_SESSIONS_ONLY,
    MODEL_DEFAULT_NO_PROFILE,
    MODEL_DEFAULT_UNAVAILABLE,
    MODELS_NOT_FETCHED,
    MODELS_STALE_EPOCH,
    PROFILE_SWITCH_FAILED,
    PROFILE_SWITCH_UNAVAILABLE,
    PROFILES_NOT_FETCHED,
    PROFILES_STALE_EPOCH,
    PROFILES_UNAVAILABLE,
    EndpointSwitcher,
    ModelAdmin,
    ModelDefaultWriter,
    ProfileAdmin,
    TalariaApp,
)
from talaria.ui.picker import (
    CATALOG_FAILURE_PREFIX,
    CURRENT_PROFILE_MARKER,
    NO_ENDPOINT_SUFFIX,
    NO_PROFILES,
    NO_PROVIDERS,
    NOT_RUNNING_SUFFIX,
    NOT_YET_FETCHED,
    PROFILES_FAILURE_PREFIX,
    PROFILES_NOT_YET_FETCHED,
    PROVIDER_WARNING_PREFIX,
    UNAUTHENTICATED_SUFFIX,
    flatten_profiles,
    flatten_selectable,
    format_profile_row,
    format_provider_header,
    header_line,
    profiles_header_line,
    warning_line,
)
from tests.ui.conftest import event, records

# ── fixtures shared by the pure and the app-level tests ───────────────────


def provider(**overrides: Any) -> ModelProvider:
    fields: dict[str, Any] = {
        "slug": "example",
        "name": "Example",
        "models": ("small", "large"),
        "authenticated": True,
    }
    fields.update(overrides)
    return ModelProvider(**fields)


def catalog(*providers: ModelProvider, **overrides: Any) -> ProviderCatalog:
    fields: dict[str, Any] = {"providers": tuple(providers)}
    fields.update(overrides)
    return ProviderCatalog(**fields)


# ── pure functions (no screen) ─────────────────────────────────────────────


def test_flatten_numbers_every_model_across_every_provider_from_one() -> None:
    cat = catalog(
        provider(slug="alpha", name="Alpha", models=("a1", "a2")),
        provider(slug="beta", name="Beta", models=("b1",)),
    )
    rows = flatten_selectable(cat)
    assert [(r.index, r.provider_slug, r.model) for r in rows] == [
        (1, "alpha", "a1"),
        (2, "alpha", "a2"),
        (3, "beta", "b1"),
    ]


def test_flatten_marks_only_the_row_matching_current_provider_and_model() -> None:
    cat = catalog(
        provider(slug="alpha", models=("a1", "a2")),
        provider(slug="beta", models=("a1",)),
        current_provider="alpha",
        current_model="a2",
    )
    rows = flatten_selectable(cat)
    current = [r for r in rows if r.is_current]
    # Same model name under a different provider is not the current row —
    # only the (provider, model) pair the gateway actually named is.
    assert [(r.provider_slug, r.model) for r in current] == [("alpha", "a2")]


def test_flatten_carries_the_providers_own_authenticated_flag() -> None:
    cat = catalog(provider(slug="p", models=("m",), authenticated=False))
    assert flatten_selectable(cat)[0].authenticated is False


def test_provider_header_marks_an_unauthenticated_provider() -> None:
    authed = format_provider_header(provider(authenticated=True))
    unauthed = format_provider_header(provider(authenticated=False))
    assert UNAUTHENTICATED_SUFFIX not in authed
    assert UNAUTHENTICATED_SUFFIX in unauthed


def test_header_line_distinguishes_not_fetched_failed_and_empty() -> None:
    assert header_line(None, "") == NOT_YET_FETCHED
    assert header_line(None, "refused").startswith(CATALOG_FAILURE_PREFIX)
    assert header_line(catalog(), "") == NO_PROVIDERS
    populated = header_line(catalog(provider()), "")
    assert populated not in {NOT_YET_FETCHED, NO_PROVIDERS}
    assert not populated.startswith(CATALOG_FAILURE_PREFIX)


def test_failure_takes_precedence_over_a_held_catalog_in_the_header() -> None:
    """A stale ``catalog`` object must not paper over a fetch that just failed."""
    held = catalog(provider())
    assert header_line(held, "the gateway refused it").startswith(CATALOG_FAILURE_PREFIX)


def test_warning_line_is_empty_with_no_provider_warnings() -> None:
    assert warning_line(None) == ""
    assert warning_line(catalog(provider(warning=""))) == ""


def test_warning_line_names_a_providers_warning_and_is_distinct_from_failure() -> None:
    said = warning_line(catalog(provider(warning="rate limited")))
    assert said.startswith(PROVIDER_WARNING_PREFIX)
    assert "rate limited" in said
    assert not said.startswith(CATALOG_FAILURE_PREFIX)


# ── the assembled app ───────────────────────────────────────────────────────


class RecordingDispatcher:
    """A dispatcher double that records calls and returns a chosen outcome."""

    def __init__(self, outcome: RpcOutcome | None = None) -> None:
        self.outcome = outcome
        self.calls: list[tuple[str, Mapping[str, Any]]] = []

    async def call(
        self,
        method: str,
        params: Mapping[str, Any] | None = None,
        *,
        timeout: float | None = None,
    ) -> RpcOutcome:
        self.calls.append((method, dict(params or {})))
        if self.outcome is not None:
            return self.outcome
        return RpcOutcome(status="ok", method=method, request_id="1", epoch=1, result={})

    @property
    def operator_calls(self) -> list[tuple[str, Mapping[str, Any]]]:
        """Every call except the startup catalogue fetch — see the identical
        property in ``tests/ui/test_live_wiring.py``."""
        return [call for call in self.calls if call[0] != CATALOG_METHOD]


class FakeAdminClient:
    """A ``ModelAdmin`` double: one fixed catalogue, or one fixed failure."""

    def __init__(
        self, catalog: ProviderCatalog | None = None, error: AdminError | None = None
    ) -> None:
        self._catalog = catalog
        self._error = error
        self.calls = 0

    async def model_options(self, *, profile: str | None = None) -> ProviderCatalog:
        self.calls += 1
        if self._error is not None:
            raise self._error
        assert self._catalog is not None
        return self._catalog


def test_the_admin_double_satisfies_the_protocol() -> None:
    """Guard the guard: a double outside the protocol proves nothing about it."""
    assert isinstance(FakeAdminClient(catalog()), ModelAdmin)


def live_app(
    dispatcher: RecordingDispatcher, admin_client: FakeAdminClient | None
) -> TalariaApp:
    controls = ReplayControls(paused=True)
    source = ReplaySource(records([event("gateway.ready", {})]), controls=controls)
    return TalariaApp(
        source,
        mode="live",
        controls=controls,
        dispatcher=dispatcher,
        admin_client=admin_client,
    )


TWO_PROVIDER_CATALOG = catalog(
    provider(slug="anthropic", name="Anthropic", models=("sonnet", "opus")),
    provider(slug="cold", name="Cold Provider", models=("m1",), authenticated=False),
    current_provider="anthropic",
    current_model="opus",
)


@pytest.mark.asyncio
async def test_opening_the_picker_renders_providers_and_models_current_marked() -> None:
    dispatcher = RecordingDispatcher()
    admin = FakeAdminClient(TWO_PROVIDER_CATALOG)
    app = live_app(dispatcher, admin)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_model_catalog()
        app.composer.text = "/models"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        rows = app.picker.row_texts
        assert any("Anthropic" in row and "anthropic" in row for row in rows)
        assert any(row.strip().endswith("opus") and row.startswith("*") for row in rows)
        assert any(row.strip().endswith("sonnet") and not row.startswith("*") for row in rows)
        # The unauthenticated provider is visibly distinguished from the rest.
        assert any("Cold Provider" in row and UNAUTHENTICATED_SUFFIX in row for row in rows)
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_not_fetched_failed_and_warning_are_each_their_own_distinguishable_line() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher, admin_client=None)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        app.composer.text = "/models"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()
        not_fetched = app.picker.header_text
        assert not_fetched == NOT_YET_FETCHED

        app.model_catalog = None
        app.model_catalog_failure = "the gateway refused Talaria's credential"
        await app.render_model_catalog()
        failed = app.picker.header_text
        assert failed.startswith(CATALOG_FAILURE_PREFIX)

        app.model_catalog = catalog(provider(warning="skill scan failed"))
        app.model_catalog_failure = ""
        await app.render_model_catalog()
        warned = app.picker.warning_text
        assert warned.startswith(PROVIDER_WARNING_PREFIX)

        assert len({not_fetched, failed}) == 2
        assert warned not in {not_fetched, failed}
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_selection_sends_the_exact_model_command_string() -> None:
    dispatcher = RecordingDispatcher()
    admin = FakeAdminClient(TWO_PROVIDER_CATALOG)
    app = live_app(dispatcher, admin)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_model_catalog()
        app.composer.text = "/models 1"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert dispatcher.operator_calls == [
            (SLASH_EXEC_METHOD, {"command": "model sonnet --provider anthropic", "session_id": ""})
        ]
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_composer_keeps_focus_through_open_and_select() -> None:
    dispatcher = RecordingDispatcher()
    admin = FakeAdminClient(TWO_PROVIDER_CATALOG)
    app = live_app(dispatcher, admin)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_model_catalog()

        app.composer.text = "/models"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()
        assert app.focused is app.composer.text_area

        app.composer.text = "/models 1"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()
        assert app.focused is app.composer.text_area
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_selecting_an_unauthenticated_provider_is_refused_client_side() -> None:
    dispatcher = RecordingDispatcher()
    admin = FakeAdminClient(TWO_PROVIDER_CATALOG)
    app = live_app(dispatcher, admin)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_model_catalog()
        # Row 3 is "cold" provider's one model, marked unauthenticated.
        app.composer.text = "/models 3"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert dispatcher.operator_calls == []
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_selection_before_any_fetch_is_refused_and_named() -> None:
    dispatcher = RecordingDispatcher()
    app = live_app(dispatcher, admin_client=None)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        app.composer.text = "/models 1"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert dispatcher.operator_calls == []
        assert MODELS_NOT_FETCHED in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_selection_against_a_stale_epoch_is_refused_rather_than_sent() -> None:
    """KTD4: the list belongs to the connection epoch it was fetched on.

    A reconnect the picker never re-rendered for still leaves
    ``app.model_catalog`` holding the old list — this simulates exactly that,
    without needing a real socket to drop.
    """
    dispatcher = RecordingDispatcher()
    admin = FakeAdminClient(TWO_PROVIDER_CATALOG)
    app = live_app(dispatcher, admin)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_model_catalog()
        assert app.model_catalog is not None

        # A reconnect: the epoch moves on without a fresh fetch landing yet.
        app._connection_epoch += 1

        app.composer.text = "/models 1"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert dispatcher.operator_calls == []
        assert MODELS_STALE_EPOCH in app.composer.notice
        await app.shutdown_sources()


# ── U4: the profile picker ────────────────────────────────────────────────
#
# **Every profile name below is invented (R12).** This is a public repository
# and ``GET /api/profiles`` answers with the operator's own inventory — 37 rows
# of it against the gateway this surface was probed on. No name, path, or other
# operator-specific value from that answer appears here.


def profile(**overrides: Any) -> ProfileEntry:
    fields: dict[str, Any] = {
        "name": "alpha-fixture",
        "model": "example-large",
        "provider": "example-provider",
        "gateway_running": True,
    }
    fields.update(overrides)
    return ProfileEntry(**fields)


def directory(*entries: ProfileEntry) -> ProfileDirectory:
    return ProfileDirectory(profiles=tuple(entries))


#: Three synthetic profiles covering the three dialability outcomes: dialable,
#: gateway down, and running-but-unaddressable.
THREE_PROFILES = directory(
    profile(name="alpha-fixture"),
    profile(name="beta-fixture", gateway_running=False, model="", provider=""),
    profile(name="gamma-fixture"),
)

#: Talaria's own endpoint map. ``gamma-fixture`` is deliberately absent: a
#: profile whose gateway is up but whose address Talaria does not know.
FIXTURE_ENDPOINTS = {
    "alpha-fixture": "ws://127.0.0.1:9119/api/ws",
    "beta-fixture": "ws://127.0.0.1:9120/api/ws",
}


# ── pure functions (no screen) ─────────────────────────────────────────────


def test_profiles_are_numbered_from_one_in_listing_order() -> None:
    rows = flatten_profiles(THREE_PROFILES, FIXTURE_ENDPOINTS)
    assert [(r.index, r.name) for r in rows] == [
        (1, "alpha-fixture"),
        (2, "beta-fixture"),
        (3, "gamma-fixture"),
    ]


def test_dialability_needs_both_a_running_gateway_and_a_known_endpoint() -> None:
    alpha, beta, gamma = flatten_profiles(THREE_PROFILES, FIXTURE_ENDPOINTS)
    assert alpha.dialable is True
    assert alpha.undialable_reason == ""
    # Gateway down, address known.
    assert beta.dialable is False
    assert "not running" in beta.undialable_reason
    # Gateway up, address unknown — a different problem with a different fix.
    assert gamma.dialable is False
    assert "endpoint" in gamma.undialable_reason
    assert beta.undialable_reason != gamma.undialable_reason


def test_every_rendered_row_marks_whether_it_can_be_dialled() -> None:
    rendered = [
        format_profile_row(row) for row in flatten_profiles(THREE_PROFILES, FIXTURE_ENDPOINTS)
    ]
    assert NOT_RUNNING_SUFFIX not in rendered[0]
    assert NO_ENDPOINT_SUFFIX not in rendered[0]
    assert NOT_RUNNING_SUFFIX in rendered[1]
    assert NO_ENDPOINT_SUFFIX in rendered[2]
    # The configured model is shown; the endpoint never is.
    assert "example-large" in rendered[0]
    assert "9119" not in "".join(rendered)


def test_the_current_profile_is_marked_and_no_other_row_is() -> None:
    rows = flatten_profiles(THREE_PROFILES, FIXTURE_ENDPOINTS, current="beta-fixture")
    assert [r.name for r in rows if r.is_current] == ["beta-fixture"]
    assert format_profile_row(rows[1]).startswith(CURRENT_PROFILE_MARKER)


def test_profiles_header_distinguishes_not_fetched_failed_and_empty() -> None:
    assert profiles_header_line(None, "") == PROFILES_NOT_YET_FETCHED
    assert profiles_header_line(None, "refused").startswith(PROFILES_FAILURE_PREFIX)
    assert profiles_header_line(directory(), "") == NO_PROFILES
    populated = profiles_header_line(THREE_PROFILES, "")
    assert populated not in {PROFILES_NOT_YET_FETCHED, NO_PROFILES}
    # A listing held from an earlier read must not paper over a failed one.
    assert profiles_header_line(THREE_PROFILES, "refused").startswith(PROFILES_FAILURE_PREFIX)


# ── the assembled app ───────────────────────────────────────────────────────


class FakeProfileAdmin:
    """A ``ModelAdmin`` *and* ``ProfileAdmin`` double."""

    def __init__(
        self,
        catalog: ProviderCatalog | None = None,
        profiles: ProfileDirectory | None = None,
        error: AdminError | None = None,
    ) -> None:
        self._catalog = catalog if catalog is not None else TWO_PROVIDER_CATALOG
        self._profiles = profiles
        self._error = error
        self.listings = 0

    async def model_options(self, *, profile: str | None = None) -> ProviderCatalog:
        return self._catalog

    async def list_profiles(self) -> ProfileDirectory:
        self.listings += 1
        if self._error is not None:
            raise self._error
        assert self._profiles is not None
        return self._profiles


class RecordingSwitcher:
    """An ``EndpointSwitcher`` double: one chosen report, every endpoint recorded."""

    def __init__(self, report: SwitchReport | None = None) -> None:
        self.report = report if report is not None else SwitchReport("switched", "connected")
        self.dialled: list[str] = []

    async def switch_to_endpoint(self, endpoint: str) -> SwitchReport:
        self.dialled.append(endpoint)
        return self.report


def test_the_profile_doubles_satisfy_their_protocols() -> None:
    """Guard the guards: a double outside the protocol proves nothing."""
    admin = FakeProfileAdmin(profiles=THREE_PROFILES)
    assert isinstance(admin, ProfileAdmin)
    assert isinstance(admin, ModelAdmin)
    assert isinstance(RecordingSwitcher(), EndpointSwitcher)
    # And the U2-era double is deliberately *not* a ProfileAdmin — that is the
    # "this gateway cannot list profiles" state, not a broken double.
    assert not isinstance(FakeAdminClient(catalog()), ProfileAdmin)


def profile_app(
    admin: object,
    switcher: RecordingSwitcher | None = None,
    *,
    endpoints: Mapping[str, str] | None = None,
    current: str = "",
) -> TalariaApp:
    controls = ReplayControls(paused=True)
    source = ReplaySource(records([event("gateway.ready", {})]), controls=controls)
    return TalariaApp(
        source,
        mode="live",
        controls=controls,
        dispatcher=RecordingDispatcher(),
        admin_client=admin,  # type: ignore[arg-type]
        switcher=switcher,
        profile_endpoints=FIXTURE_ENDPOINTS if endpoints is None else endpoints,
        current_profile=current,
    )


@pytest.mark.asyncio
async def test_the_picker_lists_profiles_with_dialability_marked() -> None:
    admin = FakeProfileAdmin(profiles=THREE_PROFILES)
    app = profile_app(admin, RecordingSwitcher())

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_profiles()
        app.composer.text = "/profiles"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        rows = app.picker.row_texts
        assert len(rows) == 3
        assert "alpha-fixture" in rows[0] and NOT_RUNNING_SUFFIX not in rows[0]
        assert "beta-fixture" in rows[1] and NOT_RUNNING_SUFFIX in rows[1]
        assert "gamma-fixture" in rows[2] and NO_ENDPOINT_SUFFIX in rows[2]
        assert app.focused is app.composer.text_area
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_profile_whose_gateway_is_not_running_is_refused_before_any_dial() -> None:
    """The property that matters: the operator stays connected."""
    admin = FakeProfileAdmin(profiles=THREE_PROFILES)
    switcher = RecordingSwitcher()
    app = profile_app(admin, switcher)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_profiles()
        # Row 2 is ``beta-fixture``: address known, gateway down.
        app.composer.text = "/profiles 2"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert switcher.dialled == []
        assert "beta-fixture" in app.composer.notice
        assert "not running" in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_running_profile_talaria_has_no_address_for_is_also_refused_first() -> None:
    admin = FakeProfileAdmin(profiles=THREE_PROFILES)
    switcher = RecordingSwitcher()
    app = profile_app(admin, switcher)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_profiles()
        app.composer.text = "/profiles 3"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert switcher.dialled == []
        assert "endpoint" in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_dialable_profile_switches_to_its_configured_endpoint() -> None:
    admin = FakeProfileAdmin(profiles=THREE_PROFILES)
    switcher = RecordingSwitcher()
    app = profile_app(admin, switcher)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_profiles()
        app.composer.text = "/profiles 1"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert switcher.dialled == [FIXTURE_ENDPOINTS["alpha-fixture"]]
        assert app.current_profile == "alpha-fixture"
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_credential_refusal_on_the_new_endpoint_names_it_and_says_why() -> None:
    """KTD6's common case: each profile's dashboard mints its own token."""
    admin = FakeProfileAdmin(profiles=THREE_PROFILES)
    switcher = RecordingSwitcher(
        SwitchReport(
            "credential_unavailable",
            "disconnected",
            "no gateway credential: run `talaria refresh-credential`",
        )
    )
    app = profile_app(admin, switcher)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_profiles()
        app.composer.text = "/profiles 1"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        notice = app.composer.notice
        assert "credential_unavailable" in notice
        assert "refresh-credential" in notice
        # And the operator is told where they now are, not left guessing.
        assert "disconnected" in notice
        assert app.current_profile == ""
        await app.shutdown_sources()


@pytest.mark.asyncio
@pytest.mark.parametrize(
    ("report", "expects_disconnected"),
    [
        (SwitchReport("credential_unavailable", "disconnected", "no credential"), True),
        (SwitchReport("auth_failed", "auth_failed", "the gateway said no"), True),
        (SwitchReport("connect_failed", "disconnected", "nothing answered"), True),
        (SwitchReport("refused_endpoint", "connected", "not a valid URL"), False),
        (SwitchReport("unsupported", "connected", "no resolver"), False),
    ],
)
async def test_every_failed_switch_leaves_a_named_state_on_screen(
    report: SwitchReport, expects_disconnected: bool
) -> None:
    """A half-completed switch must never be silent (U4's fourth failure mode)."""
    admin = FakeProfileAdmin(profiles=THREE_PROFILES)
    app = profile_app(admin, RecordingSwitcher(report))

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_profiles()
        app.composer.text = "/profiles 1"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        notice = app.composer.notice
        assert notice.startswith(PROFILE_SWITCH_FAILED)
        assert report.reason in notice
        assert report.detail in notice
        if expects_disconnected:
            assert report.state in notice
            assert "previous gateway is closed" in notice
        else:
            assert "still connected" in notice
        # A failed switch never claims the profile was taken.
        assert app.current_profile == ""
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_session_that_cannot_switch_says_so_and_stays_connected() -> None:
    admin = FakeProfileAdmin(profiles=THREE_PROFILES)
    app = profile_app(admin, switcher=None)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_profiles()
        app.composer.text = "/profiles 1"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert app.composer.notice == PROFILE_SWITCH_UNAVAILABLE
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_empty_profile_list_renders_as_a_claim_the_gateway_made() -> None:
    admin = FakeProfileAdmin(profiles=directory())
    app = profile_app(admin, RecordingSwitcher())

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_profiles()
        app.composer.text = "/profiles"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert app.picker.header_text == NO_PROFILES
        assert app.picker.row_texts == ()
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_failed_listing_is_distinguishable_from_an_empty_one() -> None:
    admin = FakeProfileAdmin(error=AdminError("unauthorized", "the gateway refused it"))
    app = profile_app(admin, RecordingSwitcher())

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_profiles()
        app.composer.text = "/profiles"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert app.picker.header_text.startswith(PROFILES_FAILURE_PREFIX)
        assert app.picker.header_text != NO_PROFILES
        assert app.profiles is None
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_gateway_that_cannot_list_profiles_says_so_rather_than_raising() -> None:
    """A U2-era admin client has no ``list_profiles``; that is a state, not a bug."""
    app = profile_app(FakeAdminClient(TWO_PROVIDER_CATALOG), RecordingSwitcher())

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_profiles()
        assert app.profiles is None
        assert app.profiles_failure == PROFILES_UNAVAILABLE

        app.composer.text = "/profiles 1"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()
        assert app.composer.notice == PROFILES_UNAVAILABLE
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_selection_before_any_listing_is_refused_and_named() -> None:
    admin = FakeProfileAdmin(profiles=THREE_PROFILES)
    switcher = RecordingSwitcher()
    app = profile_app(admin, switcher)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        app.composer.text = "/profiles 1"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert switcher.dialled == []
        assert PROFILES_NOT_FETCHED in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_selection_against_a_stale_epoch_is_refused_rather_than_dialled() -> None:
    """KTD4 for profiles: a switch lands on a different gateway by design."""
    admin = FakeProfileAdmin(profiles=THREE_PROFILES)
    switcher = RecordingSwitcher()
    app = profile_app(admin, switcher)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_profiles()
        app._connection_epoch += 1

        app.composer.text = "/profiles 1"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert switcher.dialled == []
        assert PROFILES_STALE_EPOCH in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_switching_to_the_profile_already_connected_is_refused() -> None:
    admin = FakeProfileAdmin(profiles=THREE_PROFILES)
    switcher = RecordingSwitcher()
    app = profile_app(admin, switcher, current="alpha-fixture")

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_profiles()
        app.composer.text = "/profiles 1"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert switcher.dialled == []
        assert "already connected" in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_two_modes_share_one_region_and_replace_each_other() -> None:
    admin = FakeProfileAdmin(profiles=THREE_PROFILES)
    app = profile_app(admin, RecordingSwitcher())

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_model_catalog()
        await app.load_profiles()

        app.composer.text = "/models"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()
        assert app.picker.mode == "models"
        models_rows = app.picker.row_texts
        assert any("Anthropic" in row for row in models_rows)

        # Asking for the other mode while open switches to it rather than
        # folding the region away and making the operator type it twice.
        app.composer.text = "/profiles"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()
        assert str(app.picker.mode) == "profiles"
        assert app.picker.showing is True
        profile_rows = app.picker.row_texts
        assert any("alpha-fixture" in row for row in profile_rows)
        assert not any("Anthropic" in row for row in profile_rows)
        assert profile_rows != models_rows

        # Asking again for the mode already showing is the ordinary fold.
        app.composer.text = "/profiles"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()
        assert app.picker.showing is False
        await app.shutdown_sources()


def test_no_fixture_in_this_module_names_a_real_profile() -> None:
    """R12, asserted rather than promised.

    Every profile name this file uses is one of the three synthetic ones, and
    each carries the ``-fixture`` suffix so a real name copied in from a live
    probe cannot pass unnoticed.
    """
    names = {entry.name for entry in THREE_PROFILES.profiles} | set(FIXTURE_ENDPOINTS)
    assert names == {"alpha-fixture", "beta-fixture", "gamma-fixture"}
    assert all(name.endswith("-fixture") for name in names)
    # And no endpoint in the fixtures leaves loopback.
    assert all(url.startswith("ws://127.0.0.1:") for url in FIXTURE_ENDPOINTS.values())


# ── U5: the default-model write and its two-act confirmation (KTD7) ────────
#
# ``delta-fixture`` is invented (R12), the same discipline the U4 section
# above follows.

DELTA_FIXTURE_PROFILE = "delta-fixture"


def assignment_ok(**overrides: Any) -> ModelAssignmentResult:
    fields: dict[str, Any] = {"ok": True, "scope": "main"}
    fields.update(overrides)
    return ModelAssignmentResult(**fields)


def assignment_confirm_required(message: str = "example-large costs real money") -> (
    ModelAssignmentResult
):
    return ModelAssignmentResult(
        ok=False, scope="main", confirm_required=True, confirm_message=message
    )


@dataclass
class FakeModelWriter:
    """A ``ModelAdmin`` *and* ``ModelDefaultWriter`` double (U5).

    ``results`` is consumed one call at a time — the confirmation round trip
    needs the *second* call to answer differently from the first, which a
    single fixed return value cannot express. Running past the end repeats
    the last entry, so a test that only cares about the first call need not
    pad the list.
    """

    catalog: ProviderCatalog = field(default_factory=lambda: TWO_PROVIDER_CATALOG)
    results: list[ModelAssignmentResult | AdminError] = field(default_factory=list)
    calls: list[dict[str, Any]] = field(default_factory=list)

    async def model_options(self, *, profile: str | None = None) -> ProviderCatalog:
        return self.catalog

    async def set_default_model(
        self,
        *,
        profile: str,
        provider: str,
        model: str,
        confirm_expensive_model: bool,
    ) -> ModelAssignmentResult:
        self.calls.append(
            {
                "profile": profile,
                "provider": provider,
                "model": model,
                "confirm_expensive_model": confirm_expensive_model,
            }
        )
        index = min(len(self.calls) - 1, len(self.results) - 1)
        outcome = self.results[index]
        if isinstance(outcome, AdminError):
            raise outcome
        return outcome


def test_the_model_writer_double_satisfies_both_protocols() -> None:
    """Guard the guard: a double outside the protocols proves nothing."""
    writer = FakeModelWriter(results=[assignment_ok()])
    assert isinstance(writer, ModelAdmin)
    assert isinstance(writer, ModelDefaultWriter)
    # And the U2-era double is deliberately *not* a writer — that is the
    # "this admin client cannot write a default" state, not a broken double.
    assert not isinstance(FakeAdminClient(catalog()), ModelDefaultWriter)


def default_app(
    writer: object, *, current_profile: str = DELTA_FIXTURE_PROFILE
) -> TalariaApp:
    controls = ReplayControls(paused=True)
    source = ReplaySource(records([event("gateway.ready", {})]), controls=controls)
    return TalariaApp(
        source,
        mode="live",
        controls=controls,
        dispatcher=RecordingDispatcher(),
        admin_client=writer,  # type: ignore[arg-type]
        current_profile=current_profile,
    )


@pytest.mark.asyncio
async def test_a_bare_default_write_succeeds_and_states_new_sessions_only() -> None:
    writer = FakeModelWriter(results=[assignment_ok(provider="anthropic", model="sonnet")])
    app = default_app(writer)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_model_catalog()
        # Row 1 is anthropic's "sonnet" — authenticated, not the current model.
        app.composer.text = "/models 1 default"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert writer.calls == [
            {
                "profile": DELTA_FIXTURE_PROFILE,
                "provider": "anthropic",
                "model": "sonnet",
                "confirm_expensive_model": False,
            }
        ]
        notice = app.composer.notice
        assert DELTA_FIXTURE_PROFILE in notice
        assert "sonnet" in notice
        # R4: the on-screen note that this affects new sessions only.
        assert MODEL_DEFAULT_NEW_SESSIONS_ONLY in notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_confirm_required_shows_the_message_and_the_first_call_never_confirms() -> None:
    writer = FakeModelWriter(results=[assignment_confirm_required("sonnet costs real money")])
    app = default_app(writer)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_model_catalog()
        app.composer.text = "/models 1 default"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert len(writer.calls) == 1
        assert writer.calls[0]["confirm_expensive_model"] is False

        notice = app.composer.notice
        assert "sonnet costs real money" in notice
        assert MODEL_DEFAULT_NEW_SESSIONS_ONLY in notice
        assert "/models 1 default confirm" in notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_second_distinct_act_resends_confirmed_and_completes() -> None:
    """KTD7's whole point: two textually distinct commands, not one call twice."""
    writer = FakeModelWriter(
        results=[
            assignment_confirm_required("sonnet costs real money"),
            assignment_ok(provider="anthropic", model="sonnet"),
        ]
    )
    app = default_app(writer)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_model_catalog()

        app.composer.text = "/models 1 default"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        app.composer.text = "/models 1 default confirm"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert [call["confirm_expensive_model"] for call in writer.calls] == [False, True]
        assert MODEL_DEFAULT_NEW_SESSIONS_ONLY in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_400_is_shown_on_screen_rather_than_raised() -> None:
    error = AdminError(
        "invalid_request", "the gateway refused the request as malformed (HTTP 400)"
    )
    writer = FakeModelWriter(results=[error])
    app = default_app(writer)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_model_catalog()
        app.composer.text = "/models 1 default"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        notice = app.composer.notice
        assert notice.startswith(MODEL_DEFAULT_FAILED)
        assert "malformed" in notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_default_write_before_any_fetch_is_refused_and_named() -> None:
    writer = FakeModelWriter(results=[assignment_ok()])
    app = default_app(writer)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        app.composer.text = "/models 1 default"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert writer.calls == []
        assert MODELS_NOT_FETCHED in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_default_write_against_a_stale_epoch_is_refused_rather_than_sent() -> None:
    writer = FakeModelWriter(results=[assignment_ok()])
    app = default_app(writer)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_model_catalog()
        app._connection_epoch += 1

        app.composer.text = "/models 1 default"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert writer.calls == []
        assert MODELS_STALE_EPOCH in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_a_default_write_with_no_connected_profile_is_refused() -> None:
    writer = FakeModelWriter(results=[assignment_ok()])
    app = default_app(writer, current_profile="")

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_model_catalog()
        app.composer.text = "/models 1 default"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert writer.calls == []
        assert app.composer.notice == MODEL_DEFAULT_NO_PROFILE
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_admin_client_that_cannot_write_a_default_says_so() -> None:
    """A U2-era admin client has no ``set_default_model``; that is a state,
    not a bug — the same structural refusal ``PROFILES_UNAVAILABLE`` gives a
    ``ModelAdmin`` that is not also a ``ProfileAdmin``."""
    admin = FakeAdminClient(TWO_PROVIDER_CATALOG)
    app = default_app(admin)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_model_catalog()
        app.composer.text = "/models 1 default"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert app.composer.notice == MODEL_DEFAULT_UNAVAILABLE
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_unauthenticated_row_is_refused_before_any_write() -> None:
    writer = FakeModelWriter(results=[assignment_ok()])
    app = default_app(writer)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_model_catalog()
        # Row 3 is "cold" provider's one model, marked unauthenticated.
        app.composer.text = "/models 3 default"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert writer.calls == []
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_an_unrecognized_default_syntax_is_refused_and_explained() -> None:
    writer = FakeModelWriter(results=[assignment_ok()])
    app = default_app(writer)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_model_catalog()
        app.composer.text = "/models 1 default extra-word"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert writer.calls == []
        assert "is not understood" in app.composer.notice
        await app.shutdown_sources()


@pytest.mark.asyncio
async def test_the_composer_keeps_focus_through_a_default_write() -> None:
    writer = FakeModelWriter(results=[assignment_ok(provider="anthropic", model="sonnet")])
    app = default_app(writer)

    async with app.run_test() as pilot:
        app.composer.text_area.focus()
        await app.load_model_catalog()
        app.composer.text = "/models 1 default"
        await pilot.press("enter")
        await app.settle_live()
        await pilot.pause()

        assert app.focused is app.composer.text_area
        await app.shutdown_sources()
