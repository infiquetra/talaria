"""Typed settings commands and the request specs they build.

No I/O happens here. ``rest_request`` / ``rpc_request`` return frozen specs so
a missing or ``current`` profile fails at construction, before a socket exists.
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass, field
from typing import Any

from talaria.domain.settings import ConfigTarget, RestartPlan

__all__ = [
    "ClearEnv",
    "CloneProfile",
    "DeleteProfile",
    "LiveApply",
    "LoadTarget",
    "ProbeConnection",
    "RenameProfile",
    "ResetConfig",
    "RestartGateway",
    "RevealEnv",
    "RpcRequestSpec",
    "RestRequestSpec",
    "SaveConfig",
    "SetEnv",
    "SetModel",
    "WakeWord",
    "rest_request",
    "rpc_request",
]

_WAKE_ACTIONS: frozenset[str] = frozenset({"start", "stop", "status"})


@dataclass(frozen=True)
class RestRequestSpec:
    """A profile-scoped (or host-scoped) HTTP request that has not been sent."""

    method: str
    path: str
    query: Mapping[str, str] = field(default_factory=dict)
    body: Mapping[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class RpcRequestSpec:
    """A profile-scoped JSON-RPC request that has not been sent."""

    method: str
    params: Mapping[str, Any]


@dataclass(frozen=True)
class LoadTarget:
    target: ConfigTarget


@dataclass(frozen=True)
class SaveConfig:
    target: ConfigTarget
    patch: Mapping[str, Any]


@dataclass(frozen=True)
class SetEnv:
    target: ConfigTarget
    key: str
    value: str


@dataclass(frozen=True)
class ClearEnv:
    target: ConfigTarget
    key: str


@dataclass(frozen=True)
class RevealEnv:
    target: ConfigTarget
    key: str


@dataclass(frozen=True)
class SetModel:
    target: ConfigTarget
    provider: str
    model: str


@dataclass(frozen=True)
class ResetConfig:
    target: ConfigTarget
    patch: Mapping[str, Any]


@dataclass(frozen=True)
class RestartGateway:
    target: ConfigTarget
    plan: RestartPlan


@dataclass(frozen=True)
class CloneProfile:
    connection_id: str
    source: str
    name: str
    clone_all: bool
    clone_channels: bool


@dataclass(frozen=True)
class RenameProfile:
    connection_id: str
    name: str
    new_name: str


@dataclass(frozen=True)
class DeleteProfile:
    connection_id: str
    name: str


@dataclass(frozen=True)
class ProbeConnection:
    connection_id: str


@dataclass(frozen=True)
class WakeWord:
    target: ConfigTarget
    action: str

    def __post_init__(self) -> None:
        if self.action not in _WAKE_ACTIONS:
            raise ValueError(
                f"wake action must be start, stop, or status; got {self.action!r}"
            )


@dataclass(frozen=True)
class LiveApply:
    target: ConfigTarget
    key: str
    value: object


def _profile_query(target: ConfigTarget) -> dict[str, str]:
    return {"profile": target.profile_name}


def rest_request(command: object) -> RestRequestSpec:
    """Build the HTTP spec for a command. Profile-scoped routes always query."""
    if isinstance(command, LoadTarget):
        return RestRequestSpec(
            method="GET", path="/api/config", query=_profile_query(command.target)
        )
    if isinstance(command, (SaveConfig, ResetConfig)):
        return RestRequestSpec(
            method="PUT",
            path="/api/config",
            query=_profile_query(command.target),
            body={"config": dict(command.patch)},
        )
    if isinstance(command, SetEnv):
        return RestRequestSpec(
            method="PUT",
            path="/api/env",
            query=_profile_query(command.target),
            body={"key": command.key, "value": command.value},
        )
    if isinstance(command, ClearEnv):
        return RestRequestSpec(
            method="DELETE",
            path="/api/env",
            query={**_profile_query(command.target), "key": command.key},
        )
    if isinstance(command, RevealEnv):
        return RestRequestSpec(
            method="POST",
            path="/api/env/reveal",
            query=_profile_query(command.target),
            body={"key": command.key},
        )
    if isinstance(command, SetModel):
        return RestRequestSpec(
            method="POST",
            path="/api/model/set",
            query=_profile_query(command.target),
            body={
                "scope": "main",
                "provider": command.provider,
                "model": command.model,
            },
        )
    if isinstance(command, RestartGateway):
        return RestRequestSpec(
            method="POST",
            path="/api/gateway/restart",
            query=_profile_query(command.target),
        )
    if isinstance(command, CloneProfile):
        return RestRequestSpec(
            method="POST",
            path="/api/profiles",
            body={
                "name": command.name,
                "clone_from": command.source,
                "clone_all": command.clone_all,
                "clone_channels": command.clone_channels,
            },
        )
    if isinstance(command, RenameProfile):
        return RestRequestSpec(
            method="PATCH",
            path=f"/api/profiles/{command.name}",
            body={"new_name": command.new_name},
        )
    if isinstance(command, DeleteProfile):
        return RestRequestSpec(method="DELETE", path=f"/api/profiles/{command.name}")
    if isinstance(command, ProbeConnection):
        return RestRequestSpec(method="GET", path="/api/health")
    raise TypeError(f"command {type(command).__name__} has no REST spec")


def rpc_request(command: object) -> RpcRequestSpec:
    """Build the JSON-RPC spec. Profile always rides in ``params``."""
    if isinstance(command, WakeWord):
        params: dict[str, Any] = {"profile": command.target.profile_name}
        if command.action in {"start", "stop"}:
            params["persist"] = True
        return RpcRequestSpec(method=f"wake.{command.action}", params=params)
    if isinstance(command, LiveApply):
        return RpcRequestSpec(
            method="config.set",
            params={
                "profile": command.target.profile_name,
                "key": command.key,
                "value": command.value,
            },
        )
    raise TypeError(f"command {type(command).__name__} has no RPC spec")
