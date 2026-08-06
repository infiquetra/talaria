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
    ModelAssignmentResult,
    ModelProvider,
    ProviderCatalog,
    decode_model_assignment_result,
    decode_model_selection,
    decode_profile_directory,
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


# ── U4: the endpoint directory ────────────────────────────────────────────
#
# Every profile name in this section is invented. R12 keeps the operator's real
# inventory out of this public repository, and the surest way to keep a value
# out of a fixture is for the fixture never to have held one.


def test_a_profile_row_decodes_its_name_model_provider_and_dialability() -> None:
    directory = decode_profile_directory(
        {
            "profiles": [
                {
                    "name": "alpha-fixture",
                    "model": "example-large",
                    "provider": "example-provider",
                    "gateway_running": True,
                    "is_default": True,
                    "description": "a synthetic profile",
                }
            ]
        }
    )
    entry = directory.profiles[0]
    assert (entry.name, entry.model, entry.provider) == (
        "alpha-fixture",
        "example-large",
        "example-provider",
    )
    assert entry.gateway_running is True
    assert entry.is_default is True


def test_a_missing_gateway_running_reads_as_not_dialable() -> None:
    """The safe direction: the alternative offers a dial nothing claimed."""
    directory = decode_profile_directory({"profiles": [{"name": "beta-fixture"}]})
    assert directory.profiles[0].gateway_running is False


def test_a_null_model_or_provider_reads_as_absent_rather_than_the_string_none() -> None:
    directory = decode_profile_directory(
        {"profiles": [{"name": "beta-fixture", "model": None, "provider": None}]}
    )
    assert directory.profiles[0].model == ""
    assert directory.profiles[0].provider == ""


def test_the_operators_filesystem_path_has_nowhere_to_land(
) -> None:
    """R12 made structural: ``ProfileEntry`` has no ``path`` field at all."""
    directory = decode_profile_directory(
        {"profiles": [{"name": "alpha-fixture", "path": "/nonexistent/fixture/alpha"}]}
    )
    entry = directory.profiles[0]
    assert not hasattr(entry, "path")
    assert "/nonexistent" not in repr(entry)


def test_an_unknown_profile_key_decodes_without_raising() -> None:
    directory = decode_profile_directory(
        {"profiles": [{"name": "alpha-fixture", "invented_next_release": True}]}
    )
    assert directory.profiles[0].name == "alpha-fixture"


def test_an_empty_or_absent_profile_list_is_an_empty_directory() -> None:
    assert decode_profile_directory({"profiles": []}).is_empty
    assert decode_profile_directory({}).is_empty


def test_a_profile_row_with_no_usable_name_is_refused() -> None:
    bodies: list[Any] = [
        {"profiles": [{}]},
        {"profiles": [{"name": "  "}]},
        {"profiles": [{"name": 7}]},
    ]
    for body in bodies:
        with pytest.raises(CatalogError):
            decode_profile_directory(body)


def test_a_profiles_field_that_is_not_a_list_is_refused() -> None:
    with pytest.raises(CatalogError):
        decode_profile_directory({"profiles": "alpha-fixture"})
    with pytest.raises(CatalogError):
        decode_profile_directory(["alpha-fixture"])


# ── U5: POST /api/model/set, the one write this module decodes ────────────


def test_a_successful_write_decodes_ok_true_with_no_confirmation_pending() -> None:
    result = decode_model_assignment_result(
        {
            "ok": True,
            "scope": "main",
            "provider": "example-provider",
            "model": "example-large",
            "base_url": "",
            "gateway_tools": [],
            "stale_aux": [],
        }
    )
    assert result.ok is True
    assert (result.scope, result.provider, result.model) == (
        "main",
        "example-provider",
        "example-large",
    )
    assert result.confirm_required is False
    assert result.confirm_message == ""


def test_a_confirm_required_refusal_decodes_distinctly_from_a_success() -> None:
    """KTD7's shape: the write did not happen, and the message is what an
    operator must see before the second, explicit act."""
    result = decode_model_assignment_result(
        {
            "ok": False,
            "scope": "main",
            "provider": "openai-codex",
            "model": "gpt-5.5",
            "confirm_required": True,
            "confirm_message": "gpt-5.5 costs real money",
        }
    )
    assert result.ok is False
    assert result.confirm_required is True
    assert result.confirm_message == "gpt-5.5 costs real money"


def test_an_unknown_extra_field_on_the_assignment_result_is_ignored() -> None:
    """The same tolerance every decoder in this module extends to new keys."""
    result = decode_model_assignment_result(
        {"ok": True, "invented_next_release": {"anything": "at all"}}
    )
    assert result.ok is True


def test_the_assignment_result_carries_no_credential_shaped_field() -> None:
    """Nothing in ``ModelAssignmentResult`` could ever hold a credential —
    checked structurally so a future field addition cannot introduce one."""
    from dataclasses import fields

    names = {f.name for f in fields(ModelAssignmentResult)}
    assert not any("token" in name or "credential" in name or "auth" in name for name in names)


@pytest.mark.parametrize(
    "payload",
    [
        {},
        {"scope": "main", "provider": "p", "model": "m"},
    ],
)
def test_a_response_missing_ok_is_refused(payload: dict[str, Any]) -> None:
    with pytest.raises(CatalogError):
        decode_model_assignment_result(payload)


def test_a_response_that_is_not_a_json_object_is_refused() -> None:
    with pytest.raises(CatalogError):
        decode_model_assignment_result(["ok"])


def test_a_wrong_typed_ok_field_reads_as_false_rather_than_raising() -> None:
    """The same cosmetic-field tolerance ``_as_bool`` applies everywhere else
    in this module — a malformed ``ok`` is not the same defect as a missing
    one, and only the latter is refused."""
    result = decode_model_assignment_result({"ok": "yes"})
    assert result.ok is False


def test_the_decoded_result_is_frozen() -> None:
    result = decode_model_assignment_result({"ok": True})
    with pytest.raises(FrozenInstanceError):
        result.ok = False  # type: ignore[misc]
