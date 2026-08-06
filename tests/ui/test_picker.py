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
from typing import Any

import pytest

from talaria.domain.commands import CATALOG_METHOD, SLASH_EXEC_METHOD
from talaria.domain.models_catalog import ModelProvider, ProviderCatalog
from talaria.replay.controls import ReplayControls
from talaria.replay.source import ReplaySource
from talaria.transport.admin import AdminError
from talaria.transport.rpc import RpcOutcome
from talaria.ui.app import (
    MODELS_NOT_FETCHED,
    MODELS_STALE_EPOCH,
    ModelAdmin,
    TalariaApp,
)
from talaria.ui.picker import (
    CATALOG_FAILURE_PREFIX,
    NO_PROVIDERS,
    NOT_YET_FETCHED,
    PROVIDER_WARNING_PREFIX,
    UNAUTHENTICATED_SUFFIX,
    flatten_selectable,
    format_provider_header,
    header_line,
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
