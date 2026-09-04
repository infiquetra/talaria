"""Framework-independent record of where each imported theme came from.

``/theme reload`` re-runs the import pipeline for a known source without a
file watcher, so the source behind every successful import is recorded here.
The document lives at ``<config>/theme-sources.json`` — beside, never
inside, the ``themes/`` library — so the stored-theme loader never sees it.
"""

from __future__ import annotations

import json
import os
import tempfile
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path
from types import MappingProxyType
from typing import Any, Final, Literal

from talaria.themes import ThemeSpec

SOURCES_SCHEMA_VERSION: Final[str] = "talaria-theme-sources-v1"
THEME_SOURCES_FILENAME: Final[str] = "theme-sources.json"

ImportSourceKind = Literal["file", "marketplace"]
_SOURCE_KINDS: Final[frozenset[str]] = frozenset({"file", "marketplace"})


@dataclass(frozen=True)
class ImportSource:
    """The recorded origin of one imported user theme."""

    slug: str
    kind: ImportSourceKind
    ref: str

    def __post_init__(self) -> None:
        try:
            ThemeSpec(slug=self.slug, name="source-record", dark=False, tokens={})
        except ValueError as exc:
            raise ValueError(f"invalid import-source slug: {self.slug!r}") from exc
        if self.kind not in _SOURCE_KINDS:
            raise ValueError(f"unknown import-source kind: {self.kind!r}")
        if not self.ref.strip():
            raise ValueError("import-source ref must not be empty")


def sources_path(config_dir: Path) -> Path:
    """Return the sidecar path for one configuration directory."""
    return config_dir / THEME_SOURCES_FILENAME


def _coerce_entry(slug: object, raw: object) -> ImportSource | None:
    if not isinstance(slug, str) or not isinstance(raw, dict):
        return None
    kind = raw.get("kind")
    ref = raw.get("ref")
    if kind not in _SOURCE_KINDS or not isinstance(ref, str):
        return None
    try:
        return ImportSource(slug=slug, kind=kind, ref=ref)
    except ValueError:
        return None


def load_import_sources(
    *, config_dir: Path
) -> tuple[Mapping[str, ImportSource], tuple[str, ...]]:
    """Load recorded sources, visibly skipping a corrupt sidecar document."""
    path = sources_path(config_dir)
    if not path.is_file():
        return MappingProxyType({}), ()
    try:
        payload: Any = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, ValueError) as exc:
        return MappingProxyType({}), (
            f"theme import sources were skipped: {path} is not valid JSON: {exc}",
        )
    if not isinstance(payload, dict):
        return MappingProxyType({}), (
            f"theme import sources were skipped: {path} root must be an object",
        )
    entries = payload.get("sources", {})
    if not isinstance(entries, dict):
        return MappingProxyType({}), (
            f"theme import sources were skipped: {path} sources must be an object",
        )
    sources = {
        entry.slug: entry
        for key, raw in entries.items()
        if (entry := _coerce_entry(key, raw)) is not None
    }
    if len(sources) != len(entries):
        return MappingProxyType(sources), (
            f"theme import sources were skipped: {path} has invalid entries",
        )
    return MappingProxyType(sources), ()


def record_import_source(
    *,
    config_dir: Path,
    slug: str,
    kind: ImportSourceKind,
    ref: str,
) -> Path:
    """Atomically remember one import source, keeping every other record."""
    entry = ImportSource(slug=slug, kind=kind, ref=ref)
    path = sources_path(config_dir)
    existing, _notices = load_import_sources(config_dir=config_dir)
    merged = dict(existing)
    merged[entry.slug] = entry
    document = {
        "schema_version": SOURCES_SCHEMA_VERSION,
        "sources": {
            key: {"kind": value.kind, "ref": value.ref}
            for key, value in sorted(merged.items())
        },
    }
    content = (json.dumps(document, indent=2, sort_keys=True) + "\n").encode("utf-8")
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(
        dir=str(path.parent), prefix=f".{path.name}."
    )
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(content)
        os.replace(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except OSError:
            pass
        raise
    return path


__all__ = [
    "SOURCES_SCHEMA_VERSION",
    "THEME_SOURCES_FILENAME",
    "ImportSource",
    "ImportSourceKind",
    "load_import_sources",
    "record_import_source",
    "sources_path",
]
