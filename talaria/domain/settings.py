"""Framework-independent settings contracts: target, schema, state, projection.

CFG v0.6.2 configuration lives on a ``(connection, profile)`` pair. An omitted,
empty, or ``current`` profile would resolve server-side to the dashboard launch
profile, so construction refuses those values before any I/O (KTD1). Nothing
here reads a clock, a file, or a socket, and the module imports no Textual
(ADR-0002).
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass, field, replace
from typing import Any, Final

__all__ = [
    "DASHBOARD_NOTE",
    "KNOWN_SCHEMA_TYPES",
    "ConfigTarget",
    "FieldRowView",
    "FieldSaveResult",
    "HermesConnection",
    "ModelPickerOption",
    "ModelPickerView",
    "ResetConfirmView",
    "RestartConfirmView",
    "RestartPlan",
    "RevealSecretView",
    "SaveSummaryGroup",
    "SaveSummaryView",
    "SecretRow",
    "SettingOwner",
    "SettingsDecodeError",
    "SettingsDocument",
    "SettingsField",
    "SettingsOwnerGroup",
    "SettingsRowGroupView",
    "SettingsSchema",
    "SettingsScopeError",
    "SettingsState",
    "SettingsWorkspaceView",
    "TargetHeaderView",
    "TargetOption",
    "TargetSwitchPrompt",
    "apply_config_patch",
    "apply_settings_response",
    "begin_settings_request",
    "build_config_patch",
    "build_reset_patch",
    "compute_restart_plan",
    "coverage_denominator",
    "decode_env_listing",
    "decode_settings_schema",
    "effect_summary_label",
    "fail_settings_save",
    "partition_settings_keys",
    "project_field_row",
    "project_save_summary",
    "project_secret_row",
    "project_secret_rows",
    "project_target_header",
    "project_target_options",
    "request_settings_switch",
    "select_settings_target",
    "stage_settings_edit",
    "validate_settings_value",
    "RevealDisplayValue",
    "GatewayLifecyclePrompt",
    "Reauthenticate",
]


KNOWN_SCHEMA_TYPES: Final[frozenset[str]] = frozenset(
    {"string", "number", "boolean", "list", "select"}
)

SettingOwner: Final[tuple[str, ...]] = (
    "talaria-user",
    "talaria-repository",
    "talaria-session",
    "hermes-host",
    "hermes-profile",
)

DASHBOARD_NOTE: Final[str] = (
    "Talaria's own connection is to the dashboard and is unaffected."
)

_EFFECT_LABELS: Final[Mapping[str, str]] = {
    "live": "saved + active",
    "next-session": "takes effect for new sessions",
    "gateway-restart": "awaiting gateway restart",
    "dashboard-restart-no-api": (
        "requires a dashboard restart; no API performs it"
    ),
    "unverified": "applies on next session or gateway restart — unverified",
}

_MISSING: Final[object] = object()


class SettingsScopeError(ValueError):
    """A ``ConfigTarget`` that would resolve to the launch profile."""


class SettingsDecodeError(ValueError):
    """A schema body that did not decode into :class:`SettingsSchema`."""


@dataclass(frozen=True)
class ConfigTarget:
    """The mandatory ``(connection, profile)`` key on every scoped request."""

    connection_id: str
    profile_name: str

    def __post_init__(self) -> None:
        if not self.connection_id.strip():
            raise SettingsScopeError("connection_id is missing")
        if not self.profile_name.strip():
            raise SettingsScopeError("profile_name is missing")
        if self.profile_name.strip() == "current":
            raise SettingsScopeError(
                "profile_name 'current' resolves to the dashboard launch profile"
            )


@dataclass(frozen=True)
class HermesConnection:
    """One dashboard origin and authentication session."""

    connection_id: str
    label: str
    base_url: str
    auth_mode: str
    current_profile: str
    version: str


@dataclass(frozen=True)
class SettingsField:
    """One live ``/api/config/schema`` field, including unknown types."""

    key: str
    type: str
    description: str
    category: str
    options: tuple[str, ...] = ()
    searchable: bool = False
    clearable: bool = False


@dataclass(frozen=True)
class SettingsSchema:
    """Decoded schema: server field order and server category order."""

    fields: tuple[SettingsField, ...]
    category_order: tuple[str, ...]


@dataclass(frozen=True)
class SettingsDocument:
    """Defaults, saved, and effective documents for one target."""

    target: ConfigTarget
    saved: Mapping[str, Any]
    effective: Mapping[str, Any]
    defaults: Mapping[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class SettingsState:
    """Target-keyed documents, pending edits, and in-flight generations."""

    selected: ConfigTarget | None = None
    documents: Mapping[ConfigTarget, SettingsDocument] = field(default_factory=dict)
    pending: Mapping[ConfigTarget, Mapping[str, Any]] = field(default_factory=dict)
    generations: Mapping[ConfigTarget, int] = field(default_factory=dict)
    awaiting_save: frozenset[ConfigTarget] = frozenset()
    pending_target: ConfigTarget | None = None

    @classmethod
    def empty(cls) -> SettingsState:
        return cls()


@dataclass(frozen=True)
class RestartPlan:
    """Computed D8 restart wording from live status facts."""

    scope: str
    title: str
    dashboard_note: str
    affected_profiles: tuple[str, ...]


@dataclass(frozen=True)
class TargetHeaderView:
    """Connection · current · selected header."""

    connection_label: str
    current_profile: str
    selected_profile: str
    auth_mode: str
    hermes_version: str
    shows_both_names: bool


@dataclass(frozen=True)
class SettingsOwnerGroup:
    """One ownership partition and the keys tagged into it."""

    owner: str
    keys: tuple[str, ...]


@dataclass(frozen=True)
class FieldRowView:
    """Projected field row: values, provenance, and validation."""

    key: str = ""
    label: str = ""
    help_text: str = ""
    type: str = ""
    provenance: str = ""
    default_value: object = None
    saved_value: object = None
    effective_value: object = None
    pending_value: object = None
    effect: str = ""
    tier: int = 2
    read_only: bool = False
    validation_message: str | None = None
    ownership: str = ""


@dataclass(frozen=True)
class SettingsRowGroupView:
    """Ownership group of projected rows."""

    owner: str
    title: str
    rows: tuple[FieldRowView, ...]


@dataclass(frozen=True)
class FieldSaveResult:
    """One field's save outcome, with optional server detail."""

    key: str
    outcome: str
    detail: str = ""


@dataclass(frozen=True)
class SaveSummaryGroup:
    """Fields that share one save-outcome class."""

    effect: str
    keys: tuple[str, ...]
    details: tuple[str, ...] = ()


@dataclass(frozen=True)
class SaveSummaryView:
    """Per-field save results grouped by outcome."""

    groups: tuple[SaveSummaryGroup, ...]


@dataclass(frozen=True)
class SecretRow:
    """Write-only secret row: is-set and the server's masked value only."""

    key: str
    is_set: bool
    masked: str
    provenance: str
    read_only: bool = False


@dataclass(frozen=True)
class TargetOption:
    """One picker row: a configured (connection, profile) pair."""

    connection_id: str
    profile_name: str
    is_current: bool = False
    is_selected: bool = False


@dataclass(frozen=True)
class TargetSwitchPrompt:
    """Save / Discard / Stay prompt when leaving a target with edits."""

    old_target: ConfigTarget
    new_target: ConfigTarget
    pending_count: int


@dataclass(frozen=True)
class ResetConfirmView:
    """Profile-explicit Reset confirmation."""

    target: ConfigTarget
    patch: Mapping[str, Any]


@dataclass(frozen=True)
class RestartConfirmView:
    """Restart confirmation carrying the computed plan."""

    plan: RestartPlan


@dataclass(frozen=True)
class RevealSecretView:
    """One-shot reveal overlay: key and masked value, never plaintext."""

    key: str
    masked: str


@dataclass(frozen=True)
class ModelPickerOption:
    """One selectable model row."""

    provider: str
    model: str


@dataclass(frozen=True)
class ModelPickerView:
    """Model overlay contents for one target."""

    target: ConfigTarget
    options: tuple[ModelPickerOption, ...]
    current_provider: str
    current_model: str


@dataclass(frozen=True)
class SettingsWorkspaceView:
    """Bounded workspace snapshot consumed by the presentation layer."""

    header: TargetHeaderView
    groups: tuple[SettingsRowGroupView, ...]
    pending_count: int = 0
    save_enabled: bool = False
    notice: str = ""
    summary: SaveSummaryView | None = None
    search_text: str = ""
    wake_state: str | None = None
    reset_patch: Mapping[str, Any] = field(default_factory=dict)
    secrets: Mapping[str, tuple[bool, str]] = field(default_factory=dict)
    model_picker: ModelPickerView | None = None
    target_options: tuple[TargetOption, ...] = ()
    gateway_running: bool | None = None
    auth_state: str = ""


@dataclass(frozen=True)
class GatewayLifecyclePrompt:
    """Start/Stop confirmation. Action is observed, never inferred from wake."""

    target: ConfigTarget
    action: str
    running: bool

    def __post_init__(self) -> None:
        if self.action not in {"start", "stop"}:
            raise ValueError(f"gateway action must be start or stop; got {self.action!r}")


@dataclass(frozen=True)
class Reauthenticate:
    """Ask the app to run U1's gated re-auth. Never carries a credential."""

    target: ConfigTarget


def decode_settings_schema(body: object) -> SettingsSchema:
    """Decode ``{fields, category_order}``, or raise :class:`SettingsDecodeError`.

    Unknown *types* are kept: they render read-only. A missing type, description,
    or category is a shape mismatch and is refused.
    """
    if not isinstance(body, Mapping):
        raise SettingsDecodeError(
            f"schema body is {type(body).__name__}, not a JSON object"
        )

    raw_fields = body.get("fields")
    if not isinstance(raw_fields, Mapping):
        raise SettingsDecodeError("'fields' is not a JSON object")

    raw_order = body.get("category_order")
    if isinstance(raw_order, (str, bytes)) or not isinstance(raw_order, Sequence):
        raise SettingsDecodeError("'category_order' is missing or not a list")

    fields = tuple(
        _decode_schema_field(key, entry) for key, entry in raw_fields.items()
    )
    category_order = tuple(item for item in raw_order if isinstance(item, str))
    return SettingsSchema(fields=fields, category_order=category_order)


def _decode_schema_field(key: object, entry: object) -> SettingsField:
    if not isinstance(key, str) or not key:
        raise SettingsDecodeError("schema field key is not a string")
    if not isinstance(entry, Mapping):
        raise SettingsDecodeError(f"{key} is not a field object")

    field_type = entry.get("type")
    description = entry.get("description")
    category = entry.get("category")
    if (
        not isinstance(field_type, str)
        or not isinstance(description, str)
        or not isinstance(category, str)
    ):
        raise SettingsDecodeError(
            f"{key} is missing type, description, or category"
        )

    raw_options = entry.get("options", ())
    if isinstance(raw_options, (str, bytes)) or not isinstance(raw_options, Sequence):
        options: tuple[str, ...] = ()
    else:
        options = tuple(item for item in raw_options if isinstance(item, str))

    searchable = entry.get("searchable")
    clearable = entry.get("clearable")
    return SettingsField(
        key=key,
        type=field_type,
        description=description,
        category=category,
        options=options,
        searchable=searchable if isinstance(searchable, bool) else False,
        clearable=clearable if isinstance(clearable, bool) else False,
    )


def validate_settings_value(schema_field: SettingsField, value: object) -> str | None:
    """Return ``None`` when ``value`` matches the field type, else a message."""
    kind = schema_field.type
    if kind not in KNOWN_SCHEMA_TYPES:
        return f"unsupported type {kind}: this field is read-only"
    if kind == "string":
        if isinstance(value, str):
            return None
        return f"{value!r} is not a string"
    if kind == "number":
        if isinstance(value, bool):
            return f"{value!r} is not a number"
        if isinstance(value, (int, float)):
            return None
        if isinstance(value, str) and value.strip():
            try:
                float(value)
            except ValueError:
                return f"{value!r} is not a number"
            return None
        return f"{value!r} is not a number"
    if kind == "boolean":
        if isinstance(value, bool):
            return None
        return f"{value!r} is not a boolean"
    if kind == "list":
        if isinstance(value, Sequence) and not isinstance(value, (str, bytes)):
            return None
        return f"{value!r} is not a list"
    if value in schema_field.options:
        return None
    return f"{value!r} is not one of the allowed options"


def _lookup(document: Mapping[str, Any], dotted: str) -> object:
    current: object = document
    for part in dotted.split("."):
        if not isinstance(current, Mapping) or part not in current:
            return _MISSING
        current = current[part]
    return current


def _assign(tree: dict[str, Any], dotted: str, value: object) -> None:
    parts = dotted.split(".")
    node = tree
    for part in parts[:-1]:
        existing = node.get(part)
        if not isinstance(existing, dict):
            existing = {}
            node[part] = existing
        node = existing
    node[parts[-1]] = value


def build_config_patch(
    *, saved: Mapping[str, Any], edits: Mapping[str, Any]
) -> dict[str, Any]:
    """Sparse nested patch of dotted edits that differ from ``saved``."""
    patch: dict[str, Any] = {}
    for key, value in edits.items():
        if _lookup(saved, key) == value:
            continue
        _assign(patch, key, value)
    return patch


def apply_config_patch(
    *, saved: Mapping[str, Any], patch: Mapping[str, Any]
) -> dict[str, Any]:
    """Deep-merge ``patch`` into a copy of ``saved`` without replacing tables."""
    return _deep_merge(saved, patch)


def _deep_merge(base: Mapping[str, Any], patch: Mapping[str, Any]) -> dict[str, Any]:
    merged = dict(base)
    for key, value in patch.items():
        existing = merged.get(key)
        if isinstance(existing, Mapping) and isinstance(value, Mapping):
            merged[key] = _deep_merge(existing, value)
        else:
            merged[key] = value
    return merged


def build_reset_patch(
    *,
    defaults: Mapping[str, Any],
    saved: Mapping[str, Any],
    keys: Sequence[str],
) -> dict[str, Any]:
    """Sparse patch that restores named keys to server defaults."""
    edits: dict[str, Any] = {}
    for key in keys:
        default_value = _lookup(defaults, key)
        if default_value is _MISSING:
            continue
        if _lookup(saved, key) == default_value:
            continue
        edits[key] = default_value
    return build_config_patch(saved=saved, edits=edits)


def select_settings_target(state: SettingsState, target: ConfigTarget) -> SettingsState:
    return replace(state, selected=target)


def begin_settings_request(
    state: SettingsState, target: ConfigTarget
) -> tuple[SettingsState, int]:
    generation = state.generations.get(target, 0) + 1
    generations = {**dict(state.generations), target: generation}
    return replace(state, generations=generations), generation


def apply_settings_response(
    state: SettingsState,
    *,
    target: ConfigTarget,
    generation: int,
    saved: Mapping[str, Any],
    effective: Mapping[str, Any],
    defaults: Mapping[str, Any] | None = None,
) -> SettingsState:
    """Land a reply whose generation is still current.

    A reply for the selected target lands as a load. A reply for a target
    marked ``awaiting_save`` lands even after selection moved — that is the
    Save-then-switch re-read (R9). Any other unselected reply is dropped so
    A1-2 stale/foreign responses cannot mutate another target.
    """
    if state.generations.get(target) != generation:
        return state
    selected = state.selected == target
    save_reread = target in state.awaiting_save
    if not selected and not save_reread:
        return state
    document = SettingsDocument(
        target=target,
        saved=dict(saved),
        effective=dict(effective),
        defaults=dict(defaults or {}),
    )
    documents = {**dict(state.documents), target: document}
    pending = dict(state.pending)
    awaiting = set(state.awaiting_save)
    if save_reread:
        pending.pop(target, None)
        awaiting.discard(target)
    selected_target = state.selected
    pending_target = state.pending_target
    if save_reread and pending_target is not None:
        selected_target = pending_target
        pending_target = None
    return replace(
        state,
        selected=selected_target,
        documents=documents,
        pending=pending,
        awaiting_save=frozenset(awaiting),
        pending_target=pending_target,
    )


def stage_settings_edit(
    state: SettingsState, *, target: ConfigTarget, key: str, value: object
) -> SettingsState:
    staged = dict(state.pending.get(target, {}))
    staged[key] = value
    pending = {**dict(state.pending), target: staged}
    return replace(state, pending=pending)


def request_settings_switch(
    state: SettingsState, *, new_target: ConfigTarget, choice: str
) -> tuple[SettingsState, tuple[object, ...]]:
    """Stay keeps edits and does not switch. Save writes only the old target."""
    if choice not in {"save", "discard", "stay"}:
        raise ValueError(f"unknown switch choice {choice!r}")

    old = state.selected
    if old is None or choice == "stay":
        return state, ()

    pending = {key: value for key, value in state.pending.items() if key != new_target}
    if choice == "discard":
        pending = {key: value for key, value in pending.items() if key != old}
        return replace(state, selected=new_target, pending=pending), ()

    from talaria.domain.settings_commands import SaveConfig

    document = state.documents.get(old)
    saved = document.saved if document is not None else {}
    patch = build_config_patch(saved=saved, edits=state.pending.get(old, {}))
    issued = (SaveConfig(target=old, patch=patch),)
    awaiting = frozenset(state.awaiting_save | {old})
    return replace(
        state,
        selected=old,
        pending=pending,
        awaiting_save=awaiting,
        pending_target=new_target,
    ), issued


def fail_settings_save(state: SettingsState, *, target: ConfigTarget) -> SettingsState:
    """Abandon an in-flight Save: keep selection and edits, drop the switch."""
    awaiting = set(state.awaiting_save)
    awaiting.discard(target)
    return replace(
        state,
        awaiting_save=frozenset(awaiting),
        pending_target=None,
    )


def project_target_options(
    targets: Sequence[ConfigTarget],
    *,
    selected: ConfigTarget,
    current: ConfigTarget,
) -> tuple[TargetOption, ...]:
    """Stable-order picker rows with current/selected flags, no reordering."""
    return tuple(
        TargetOption(
            connection_id=item.connection_id,
            profile_name=item.profile_name,
            is_current=item == current,
            is_selected=item == selected,
        )
        for item in targets
    )


def project_target_header(
    *, connection: HermesConnection, selected: ConfigTarget
) -> TargetHeaderView:
    return TargetHeaderView(
        connection_label=connection.label,
        current_profile=connection.current_profile,
        selected_profile=selected.profile_name,
        auth_mode=connection.auth_mode,
        hermes_version=connection.version,
        shows_both_names=connection.current_profile != selected.profile_name,
    )


def partition_settings_keys(
    *,
    talaria_user: Sequence[str],
    talaria_repository: Sequence[str],
    talaria_session: Sequence[str],
    hermes_host: Sequence[str],
    hermes_profile: Sequence[str],
) -> tuple[SettingsOwnerGroup, ...]:
    """Place each caller-tagged key once; ownership is never inferred by prefix."""
    tagged = {
        "talaria-user": tuple(talaria_user),
        "talaria-repository": tuple(talaria_repository),
        "talaria-session": tuple(talaria_session),
        "hermes-host": tuple(hermes_host),
        "hermes-profile": tuple(hermes_profile),
    }
    return tuple(
        SettingsOwnerGroup(owner=owner, keys=tagged[owner]) for owner in SettingOwner
    )


def project_field_row(
    *,
    field: SettingsField,
    default_value: object,
    saved_value: object,
    effective_value: object,
    pending_value: object,
    ownership: str,
    effect: str,
    tier: int,
) -> FieldRowView:
    unknown = field.type not in KNOWN_SCHEMA_TYPES
    if pending_value is not None:
        provenance = "changed"
        validation = validate_settings_value(field, pending_value)
    elif unknown:
        provenance = "default" if saved_value == default_value else "saved"
        validation = validate_settings_value(field, saved_value)
    elif saved_value != default_value:
        provenance = "saved"
        validation = None
    else:
        provenance = "default"
        validation = None

    label = field.key.rsplit(".", 1)[-1].replace("_", " ")
    return FieldRowView(
        key=field.key,
        label=label,
        help_text=field.description,
        type=field.type,
        provenance=provenance,
        default_value=default_value,
        saved_value=saved_value,
        effective_value=effective_value,
        pending_value=pending_value,
        effect=effect,
        tier=tier,
        read_only=unknown,
        validation_message=validation,
        ownership=ownership,
    )


def compute_restart_plan(
    *,
    target: ConfigTarget,
    target_gateway_running: bool,
    multiplexer_running: bool,
    multiplexed_siblings: Sequence[str],
) -> RestartPlan:
    """Map live facts onto exactly one D8 sentence."""
    name = target.profile_name
    if target_gateway_running:
        return RestartPlan(
            scope="own-gateway",
            title=f"Restart the gateway for profile {name}",
            dashboard_note=DASHBOARD_NOTE,
            affected_profiles=(name,),
        )
    if multiplexer_running:
        siblings = tuple(multiplexed_siblings)
        served = ", ".join(siblings)
        return RestartPlan(
            scope="shared-multiplexer",
            title=(
                "Restart the shared default gateway — this also restarts the "
                f"gateways serving {served}"
            ),
            dashboard_note=DASHBOARD_NOTE,
            affected_profiles=(name, *siblings),
        )
    return RestartPlan(
        scope="none",
        title=f"No gateway is running for {name}; start it instead",
        dashboard_note=DASHBOARD_NOTE,
        affected_profiles=(),
    )


def effect_summary_label(effect: str) -> str:
    try:
        return _EFFECT_LABELS[effect]
    except KeyError:
        raise ValueError(f"unknown effect timing {effect!r}") from None


def project_save_summary(*, results: Sequence[FieldSaveResult]) -> SaveSummaryView:
    grouped: dict[str, list[FieldSaveResult]] = {}
    order: list[str] = []
    for result in results:
        if result.outcome not in grouped:
            grouped[result.outcome] = []
            order.append(result.outcome)
        grouped[result.outcome].append(result)
    groups = tuple(
        SaveSummaryGroup(
            effect=outcome,
            keys=tuple(item.key for item in grouped[outcome]),
            details=tuple(item.detail for item in grouped[outcome] if item.detail),
        )
        for outcome in order
    )
    return SaveSummaryView(groups=groups)


def decode_env_listing(body: object) -> dict[str, tuple[bool, str]]:
    """Decode ``GET /api/env`` to ``{key: (is_set, masked)}``. Never a value."""
    if not isinstance(body, Mapping):
        raise SettingsDecodeError(
            f"env listing is {type(body).__name__}, not a JSON object"
        )
    listing: dict[str, tuple[bool, str]] = {}
    for raw_key, entry in body.items():
        key = str(raw_key)
        if not isinstance(entry, Mapping):
            listing[key] = (False, "")
            continue
        is_set = entry.get("is_set") is True
        masked = entry.get("redacted_value")
        listing[key] = (
            True,
            masked if isinstance(masked, str) else "",
        ) if is_set else (False, "")
    return listing


def project_secret_row(
    *, key: str, is_set: bool, redacted_value: str
) -> SecretRow:
    return SecretRow(
        key=key,
        is_set=is_set,
        masked=redacted_value if is_set else "",
        provenance="saved" if is_set else "default",
    )


def project_secret_rows(
    listing: Mapping[str, tuple[bool, str]],
) -> tuple[SecretRow, ...]:
    return tuple(
        project_secret_row(key=key, is_set=is_set, redacted_value=masked)
        for key, (is_set, masked) in sorted(listing.items())
    )


class RevealDisplayValue:
    """Presentation-ephemeral plaintext. Take once; never appear in ``repr``."""

    def __init__(self, key: str, value: str) -> None:
        self.key = key
        self._value: str | None = value
        self._cleared = False

    @property
    def is_cleared(self) -> bool:
        return self._cleared

    def take(self) -> str | None:
        if self._cleared:
            return None
        value = self._value
        self.clear()
        return value

    def clear(self) -> None:
        self._value = None
        self._cleared = True

    def __repr__(self) -> str:
        return f"RevealDisplayValue(key={self.key!r}, cleared={self._cleared})"


def coverage_denominator(schema: SettingsSchema) -> frozenset[str]:
    """Curated manifest keys plus every field in the supplied live schema."""
    from talaria.domain.settings_catalog import TIER1

    return frozenset(entry.key for entry in TIER1) | frozenset(
        item.key for item in schema.fields
    )
