"""Pure decode of Hermes's admin HTTP catalogue into frozen dataclasses.

**Why this is not part of ``talaria/domain/models.py``.** That module is the
*protocol* boundary: the JSON-RPC frames the gateway pushes down the WebSocket,
whose whole argument turns on never letting a coercing validator repair a
malformation the transport must surface. This module decodes a *different wire*
— HTTP responses from the dashboard's admin API — with a different failure
vocabulary. Keeping them apart keeps ``models.py``'s determinism argument about
one thing. Do not merge them.

The same no-validation-library rule applies here for the same reason, so the
dataclasses below are hand-written and frozen, and nothing in this module reads
a clock, a file, or a socket. Feed it a decoded JSON body and it returns either
a value or :class:`CatalogError`.

**The three tolerances, stated once.** Hermes's payload is built by a function
whose flags add and remove per-row keys (``build_models_payload`` in
``hermes_cli/inventory.py``), so the shape is a moving target and a decoder that
insists on the full set would break on the next flag change. Therefore:

* An **unknown key** is ignored. New keys arrive without a Talaria release.
* A provider whose ``models`` list is **empty** is a provider, not an error. It
  is what Hermes emits for a row it appended without a catalogue
  (``_append_unconfigured_rows`` sets ``"models": []``), and R7 requires "there
  are no models here" to stay distinguishable from "the list could not be read".
* ``authenticated: false`` is a normal state, carried through to the caller
  rather than filtered out, because a provider the operator has not yet
  authenticated is exactly what a picker should show them.

**What is refused.** A provider missing ``slug`` or missing ``models``
entirely — not empty, absent. Those are the two keys every Hermes code path that
builds a row sets, so their absence means the body is not the payload this
decoder was pinned against, and guessing a slug would put an unselectable row on
screen that looks selectable.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass, field
from typing import Any

__all__ = [
    "CatalogError",
    "ModelProvider",
    "ModelSelection",
    "ProviderCatalog",
    "decode_model_selection",
    "decode_provider_catalog",
]


class CatalogError(Exception):
    """A body that did not decode into a catalogue.

    Distinct from the transport's :class:`~talaria.transport.admin.AdminError`
    on purpose: this one means "the gateway answered, and the answer was not
    the shape we pinned", which is a different thing for a caller to say on
    screen than "the gateway did not answer".

    Carries no credential, because nothing in this module ever sees one.
    """


@dataclass(frozen=True)
class ModelProvider:
    """One provider row and the models it offers.

    ``total_models`` is Hermes's own count and may exceed ``len(models)`` — the
    payload truncates long catalogues while still reporting the true size. It is
    kept as sent rather than recomputed, so a picker can honestly say "showing 8
    of 214" instead of quietly claiming the truncated list is everything.
    """

    slug: str
    name: str
    models: tuple[str, ...]
    authenticated: bool
    auth_type: str = ""
    warning: str = ""
    is_current: bool = False
    is_user_defined: bool = False
    source: str = ""
    total_models: int = 0
    featured_models: tuple[str, ...] = ()
    capabilities: Mapping[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class ProviderCatalog:
    """``GET /api/model/options`` decoded.

    ``current_model`` and ``current_provider`` are the gateway's view of what is
    selected *for the scoped profile*, which is not necessarily what the running
    session is using — the session's model is changed by ``/model`` over the
    WebSocket and this endpoint does not see it. A caller that conflates the two
    will show a stale check-mark.
    """

    providers: tuple[ModelProvider, ...]
    current_model: str = ""
    current_provider: str = ""

    @property
    def is_empty(self) -> bool:
        """True when the gateway answered with no providers at all.

        A separate property rather than ``not catalog.providers`` at each call
        site, because R7 makes this a state a picker must render distinctly and
        a truthiness test on the dataclass would be the easy thing to get wrong.
        """
        return not self.providers


@dataclass(frozen=True)
class ModelSelection:
    """``GET /api/model/info`` decoded.

    Every field is optional in the payload: Hermes returns an ``_EMPTY_MODEL_INFO``
    skeleton when no model is configured, so "absent" is a real answer and not a
    malformation. That is why this decode raises only on a body that is not a
    mapping at all.
    """

    model: str = ""
    provider: str = ""
    auto_context_length: int = 0
    config_context_length: int | None = None
    effective_context_length: int = 0
    capabilities: Mapping[str, Any] = field(default_factory=dict)


def _require_mapping(payload: object, what: str) -> Mapping[str, Any]:
    if not isinstance(payload, Mapping):
        raise CatalogError(f"{what} is {type(payload).__name__}, not a JSON object")
    return payload


def _as_str(value: object) -> str:
    """A string field, with non-strings dropped rather than coerced.

    ``str(value)`` is deliberately not used. Coercing ``{"name": 7}`` to
    ``"7"`` is precisely the repair ``models.py`` exists to refuse; here the
    field is cosmetic rather than protocol-critical, so the honest handling is
    to treat a wrong-typed cosmetic field as absent instead of inventing a
    label an operator would then try to select by.
    """
    return value if isinstance(value, str) else ""


def _as_str_tuple(value: object) -> tuple[str, ...]:
    """Every string in a sequence, in order, with non-strings skipped.

    ``str`` and ``bytes`` are rejected as containers on purpose: iterating a
    bare string yields characters, which would turn ``"gpt-5.5"`` into seven
    single-letter models — a malformation that renders as a plausible list.
    """
    if isinstance(value, (str, bytes)) or not isinstance(value, Sequence):
        return ()
    return tuple(item for item in value if isinstance(item, str))


def _as_bool(value: object, *, default: bool = False) -> bool:
    return value if isinstance(value, bool) else default


def _as_int(value: object, *, default: int = 0) -> int:
    # ``bool`` is an ``int`` subclass, so it is excluded explicitly: ``True``
    # arriving in ``total_models`` should read as absent, not as a count of 1.
    if isinstance(value, bool) or not isinstance(value, int):
        return default
    return value


def _as_mapping(value: object) -> Mapping[str, Any]:
    return value if isinstance(value, Mapping) else {}


def _decode_provider(entry: object, index: int) -> ModelProvider:
    row = _require_mapping(entry, f"providers[{index}]")

    slug = row.get("slug")
    if not isinstance(slug, str) or not slug.strip():
        raise CatalogError(f"providers[{index}] carries no usable 'slug'")

    if "models" not in row:
        raise CatalogError(f"providers[{slug}] has no 'models' key")

    return ModelProvider(
        slug=slug,
        # Falling back to the slug is a display convenience, not a repair: the
        # slug is the identifier a selection is made by, so a row without a
        # human name is still fully usable and hiding it would lose a provider.
        name=_as_str(row.get("name")) or slug,
        models=_as_str_tuple(row.get("models")),
        authenticated=_as_bool(row.get("authenticated")),
        auth_type=_as_str(row.get("auth_type")),
        warning=_as_str(row.get("warning")),
        is_current=_as_bool(row.get("is_current")),
        is_user_defined=_as_bool(row.get("is_user_defined")),
        source=_as_str(row.get("source")),
        total_models=_as_int(row.get("total_models")),
        featured_models=_as_str_tuple(row.get("featured_models")),
        capabilities=_as_mapping(row.get("capabilities")),
    )


def decode_provider_catalog(payload: object) -> ProviderCatalog:
    """Decode a ``GET /api/model/options`` body, or raise :class:`CatalogError`.

    An empty or absent ``providers`` array decodes to an empty catalogue. That
    is a deliberate asymmetry with the per-row strictness above: zero providers
    is a state the gateway really reports (nothing authenticated yet), while a
    row missing its identifier is a shape mismatch.
    """
    body = _require_mapping(payload, "the model options response")

    raw = body.get("providers", ())
    if isinstance(raw, (str, bytes)) or not isinstance(raw, Sequence):
        raise CatalogError(
            f"'providers' is {type(raw).__name__}, not a list of provider objects"
        )

    return ProviderCatalog(
        providers=tuple(_decode_provider(entry, i) for i, entry in enumerate(raw)),
        current_model=_as_str(body.get("model")),
        current_provider=_as_str(body.get("provider")),
    )


def decode_model_selection(payload: object) -> ModelSelection:
    """Decode a ``GET /api/model/info`` body, or raise :class:`CatalogError`."""
    body = _require_mapping(payload, "the model info response")

    config_ctx = body.get("config_context_length")
    return ModelSelection(
        model=_as_str(body.get("model")),
        provider=_as_str(body.get("provider")),
        auto_context_length=_as_int(body.get("auto_context_length")),
        # ``None`` is meaningful here and distinct from ``0``: it is "the
        # operator set no override", where ``0`` would claim they set one to
        # zero. Only that distinction justifies the optional type.
        config_context_length=(
            None if config_ctx is None else _as_int(config_ctx)
        ),
        effective_context_length=_as_int(body.get("effective_context_length")),
        capabilities=_as_mapping(body.get("capabilities")),
    )
