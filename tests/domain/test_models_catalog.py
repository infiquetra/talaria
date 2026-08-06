"""The admin catalogue decode: what it tolerates, and what it refuses.

The interesting assertions here are the tolerances. Hermes builds this payload
from a function whose flags add and remove per-row keys, so a decoder that
insisted on the full set would break on the next flag change — and the way that
break presents is an empty model picker, which looks exactly like "you have no
providers configured".

No fixture in this module names a real provider slug an operator uses, a profile
name, or a filesystem path (R12: this is a public repository), and none supplies
a credential — this module never sees one.
"""

from __future__ import annotations

from dataclasses import FrozenInstanceError
from typing import Any

import pytest

from talaria.domain.models_catalog import (
    CatalogError,
    ModelProvider,
    ProviderCatalog,
    decode_model_selection,
    decode_provider_catalog,
)


def provider_row(**overrides: Any) -> dict[str, Any]:
    """A well-formed provider row, shaped after the live payload.

    The key set is the union observed from a running gateway on 2026-08-06:
    ``slug``, ``name``, ``models``, ``authenticated``, ``auth_type``,
    ``capabilities``, ``featured_models``, ``is_current``, ``is_user_defined``,
    ``source``, ``total_models``, ``warning``. The values are invented.
    """
    row: dict[str, Any] = {
        "slug": "example-provider",
        "name": "Example Provider",
        "models": ["example-small", "example-large"],
        "authenticated": True,
        "auth_type": "api_key",
        "capabilities": {"example-small": {"fast": True, "reasoning": False}},
        "featured_models": ["example-large"],
        "is_current": False,
        "is_user_defined": False,
        "source": "builtin",
        "total_models": 2,
        "warning": "",
    }
    row.update(overrides)
    return row


def options_body(*rows: dict[str, Any], **overrides: Any) -> dict[str, Any]:
    body: dict[str, Any] = {
        "providers": list(rows),
        "model": "example-large",
        "provider": "example-provider",
    }
    body.update(overrides)
    return body


# ── The three tolerances the module docstring promises ───────────────────


def test_a_provider_with_no_models_decodes_and_survives_as_a_provider() -> None:
    """``models: []`` is a state Hermes really emits, not a malformation.

    R7 turns on this distinction: "this provider offers nothing" must stay
    tellable from "the list could not be read", and dropping the row would
    collapse them.
    """
    catalog = decode_provider_catalog(
        options_body(provider_row(models=[], total_models=0))
    )

    assert len(catalog.providers) == 1
    assert catalog.providers[0].models == ()
    assert not catalog.is_empty


def test_an_unauthenticated_provider_decodes_and_is_not_filtered_out() -> None:
    """A provider the operator has not authenticated is exactly what a picker shows."""
    catalog = decode_provider_catalog(
        options_body(provider_row(authenticated=False, warning="no key configured"))
    )

    assert catalog.providers[0].authenticated is False
    assert catalog.providers[0].warning == "no key configured"


def test_an_unknown_extra_field_is_ignored_rather_than_refused() -> None:
    """New keys arrive without a Talaria release; they must not be fatal."""
    catalog = decode_provider_catalog(
        options_body(provider_row(some_field_invented_next_quarter={"nested": [1, 2]}))
    )

    assert catalog.providers[0].slug == "example-provider"


def test_an_unknown_top_level_field_is_ignored_too() -> None:
    catalog = decode_provider_catalog(options_body(provider_row(), a_new_top_level=7))

    assert catalog.current_model == "example-large"


# ── What is refused, and refused as CatalogError ─────────────────────────


@pytest.mark.parametrize(
    "row",
    [
        pytest.param({"name": "No Slug", "models": []}, id="slug-absent"),
        pytest.param({"slug": "", "models": []}, id="slug-empty"),
        pytest.param({"slug": "   ", "models": []}, id="slug-whitespace"),
        pytest.param({"slug": 7, "models": []}, id="slug-not-a-string"),
    ],
)
def test_a_provider_without_a_usable_slug_is_refused(row: dict[str, Any]) -> None:
    """The slug is the identifier a selection is made by.

    Inventing one would put a row on screen that looks selectable and is not,
    which is the coercing-validator repair ``models.py`` exists to refuse.
    """
    with pytest.raises(CatalogError, match="slug"):
        decode_provider_catalog(options_body(row))


def test_a_provider_missing_the_models_key_entirely_is_refused() -> None:
    """Absent, not empty. Every Hermes path that builds a row sets this key."""
    row = provider_row()
    del row["models"]

    with pytest.raises(CatalogError, match="models"):
        decode_provider_catalog(options_body(row))


@pytest.mark.parametrize(
    "payload",
    [
        pytest.param([], id="list"),
        pytest.param("providers", id="string"),
        pytest.param(None, id="null"),
        pytest.param(7, id="int"),
    ],
)
def test_a_body_that_is_not_a_json_object_is_refused(payload: Any) -> None:
    with pytest.raises(CatalogError):
        decode_provider_catalog(payload)


@pytest.mark.parametrize(
    "providers",
    [
        pytest.param("openai", id="string-not-list"),
        pytest.param({"slug": "x"}, id="object-not-list"),
    ],
)
def test_a_providers_value_that_is_not_a_list_is_refused(providers: Any) -> None:
    """A string is a ``Sequence``, so it must be excluded explicitly.

    Left in, iterating ``"openai"`` would yield six single-character entries and
    the failure would render as six broken providers rather than as an error.
    """
    with pytest.raises(CatalogError, match="providers"):
        decode_provider_catalog(options_body(**{"providers": providers}))


def test_a_provider_entry_that_is_not_an_object_is_refused() -> None:
    with pytest.raises(CatalogError, match=r"providers\[0\]"):
        decode_provider_catalog(options_body(**{"providers": ["not-an-object"]}))


# ── The empty catalogue is a value, not an error ─────────────────────────


def test_an_empty_providers_array_decodes_to_an_empty_catalogue() -> None:
    """Zero providers is a state the gateway really reports.

    Deliberately asymmetric with the per-row strictness above: nothing
    authenticated yet is a real answer, while a row missing its identifier is a
    shape mismatch.
    """
    catalog = decode_provider_catalog(options_body())

    assert catalog.providers == ()
    assert catalog.is_empty


def test_an_absent_providers_key_decodes_to_an_empty_catalogue() -> None:
    catalog = decode_provider_catalog({"model": "", "provider": ""})

    assert catalog.is_empty


# ── Field handling that would otherwise be silently wrong ────────────────


def test_a_model_list_is_never_built_by_iterating_a_string() -> None:
    """``"example-large"`` must not become seven single-letter models."""
    catalog = decode_provider_catalog(options_body(provider_row(models="example-large")))

    assert catalog.providers[0].models == ()


def test_non_string_entries_inside_a_model_list_are_skipped() -> None:
    catalog = decode_provider_catalog(
        options_body(provider_row(models=["example-small", None, 7, "example-large"]))
    )

    assert catalog.providers[0].models == ("example-small", "example-large")


def test_total_models_is_kept_as_sent_and_may_exceed_the_listed_models() -> None:
    """The payload truncates long catalogues while reporting the true size.

    Recomputing it would make the picker claim the truncated list is everything.
    """
    catalog = decode_provider_catalog(
        options_body(provider_row(models=["a", "b"], total_models=214))
    )

    assert catalog.providers[0].total_models == 214


def test_a_boolean_in_total_models_reads_as_absent_rather_than_as_one() -> None:
    """``bool`` is an ``int`` subclass, so it needs an explicit exclusion."""
    catalog = decode_provider_catalog(options_body(provider_row(total_models=True)))

    assert catalog.providers[0].total_models == 0


def test_a_row_without_a_name_falls_back_to_its_slug() -> None:
    """A display convenience, not a repair: the slug is what selection uses."""
    row = provider_row()
    del row["name"]

    catalog = decode_provider_catalog(options_body(row))

    assert catalog.providers[0].name == "example-provider"


def test_a_wrong_typed_cosmetic_field_reads_as_absent_and_is_not_coerced() -> None:
    """``{"name": 7}`` must not become ``"7"`` — that is the repair we refuse."""
    catalog = decode_provider_catalog(options_body(provider_row(name=7)))

    assert catalog.providers[0].name == "example-provider"


def test_provider_order_is_preserved() -> None:
    """Hermes sorts these into a display order; re-sorting would discard it."""
    catalog = decode_provider_catalog(
        options_body(
            provider_row(slug="first"),
            provider_row(slug="second"),
            provider_row(slug="third"),
        )
    )

    assert [p.slug for p in catalog.providers] == ["first", "second", "third"]


def test_the_decoded_catalogue_is_frozen() -> None:
    catalog = decode_provider_catalog(options_body(provider_row()))

    with pytest.raises(FrozenInstanceError):
        catalog.providers[0].slug = "mutated"  # type: ignore[misc]
    with pytest.raises(FrozenInstanceError):
        catalog.current_model = "mutated"  # type: ignore[misc]


def test_a_catalog_and_a_provider_are_constructible_directly() -> None:
    """The dataclasses are usable without a wire payload, for U2's test doubles."""
    catalog = ProviderCatalog(
        providers=(ModelProvider(slug="s", name="S", models=("m",), authenticated=True),)
    )

    assert catalog.providers[0].auth_type == ""
    assert catalog.providers[0].capabilities == {}


# ── GET /api/model/info ──────────────────────────────────────────────────


def test_model_info_decodes_the_live_key_set() -> None:
    selection = decode_model_selection(
        {
            "model": "example-large",
            "provider": "example-provider",
            "auto_context_length": 200000,
            "config_context_length": None,
            "effective_context_length": 200000,
            "capabilities": {"vision": True},
        }
    )

    assert selection.model == "example-large"
    assert selection.config_context_length is None
    assert selection.effective_context_length == 200000


def test_model_info_with_no_model_configured_decodes_rather_than_raising() -> None:
    """Hermes returns an empty skeleton when no model is set; that is an answer."""
    selection = decode_model_selection(
        {
            "model": "",
            "provider": "",
            "auto_context_length": 0,
            "config_context_length": None,
            "effective_context_length": 0,
            "capabilities": {},
        }
    )

    assert selection.model == ""
    assert selection.provider == ""


def test_a_configured_context_override_is_distinguishable_from_no_override() -> None:
    """``None`` means "no override set"; ``0`` would claim one was set to zero."""
    assert decode_model_selection({"config_context_length": 0}).config_context_length == 0
    assert decode_model_selection({}).config_context_length is None


def test_model_info_refuses_a_body_that_is_not_a_json_object() -> None:
    with pytest.raises(CatalogError):
        decode_model_selection(["model"])
