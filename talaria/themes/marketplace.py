"""Bounded marketplace search/select/fetch for Talaria theme import.

Framework-independent: the standard library plus :mod:`talaria.themes`
only. Fetched bytes are parsed by the single strict entry point in
:mod:`talaria.ui.theme_import`; they are never executed, imported, or
dynamically loaded as code.

Per the settled decision D on the requirements ledger, a user-selected
source is accepted with no additional trust policy. "Unknown" below is a
scoping fact — the reference names nothing the search and lookup seams
returned — never a trust verdict.
"""

from __future__ import annotations

import json
import re
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from typing import Any, Final, Literal, Protocol

MarketplaceErrorKind = Literal[
    "unknown-source",
    "ambiguous-source",
    "network",
    "oversized",
]


class MarketplaceError(ValueError):
    """A marketplace reference cannot produce theme source bytes."""

    def __init__(self, message: str, *, kind: MarketplaceErrorKind) -> None:
        super().__init__(message)
        self.kind = kind


#: Refuse payloads larger than this before parsing; Visual Studio Code
#: color themes are small JSON documents, so anything past this bound is
#: either a mistake or an attack, never a theme.
MAX_MARKETPLACE_BYTES: Final[int] = 256 * 1024

#: One search page never returns more entries than this.
DEFAULT_SEARCH_LIMIT: Final[int] = 10
MAX_SEARCH_LIMIT: Final[int] = 25

#: The default registry spoken by :class:`UrllibMarketplaceTransport`.
#: Constructor-overridable; direct ``https://`` references bypass it.
DEFAULT_REGISTRY_BASE_URL: Final[str] = "https://open-vsx.org"

#: One registry round trip never waits longer than this.
FETCH_TIMEOUT_SECONDS: Final[float] = 10.0

_URL_SCHEME_RE: Final[re.Pattern[str]] = re.compile(r"^([A-Za-z][A-Za-z0-9+.-]*)://")
_EXTENSION_PART_RE: Final[re.Pattern[str]] = re.compile(r"^[A-Za-z0-9_.-]+$")
_SLUG_RUN_RE: Final[re.Pattern[str]] = re.compile(r"[^a-z0-9]+")

#: A GitHub file page names a theme file through a web page, not through
#: the raw file: ``github.com/<owner>/<repo>/blob/<rev>/<path>`` (and the
#: ``/raw/`` variant) converts to the ``raw.githubusercontent.com`` URL for
#: the same revision and path. The converted URL fetches under the same
#: size bound as any direct URL; the page itself is never fetched.
_GITHUB_PAGE_RE: Final[re.Pattern[str]] = re.compile(
    r"^https://github\.com/"
    r"(?P<owner>[A-Za-z0-9_.-]+)/(?P<repo>[A-Za-z0-9_.-]+)/"
    r"(?:blob|raw)/(?P<rev>[^/]+)/(?P<path>.+)$"
)

#: An Open VSX extension page names the same ``publisher/extension`` the
#: registry lookup takes: ``open-vsx.org/extension/<publisher>/<extension>``.
_OPEN_VSX_PAGE_RE: Final[re.Pattern[str]] = re.compile(
    r"^https://open-vsx\.org/extension/"
    r"(?P<publisher>[A-Za-z0-9_.-]+)/(?P<extension>[A-Za-z0-9_.-]+)/?$"
)

#: Gallery hosts whose web pages are never raw theme files. Their
#: ``/api/`` file paths still fetch directly; every other page on these
#: hosts is explained instead of downloaded, so a search page can never
#: come back as a size or parse failure.
_GALLERY_HOSTS: Final[frozenset[str]] = frozenset(
    {"open-vsx.org", "marketplace.visualstudio.com"}
)

#: The supported fetch inputs, repeated wherever the operator meets a
#: rejection so no failure sends them guessing. Gallery search pages and
#: other web pages are not convertible: search the registry first with
#: ``/theme search`` (or ``talaria theme search``) and fetch the reference
#: it lists.
_SUPPORTED_FORMS: Final[str] = (
    "supported sources are publisher/extension[/theme] registry references "
    "(the Open VSX registry), direct http(s) URLs to raw theme JSON files, "
    "GitHub file-page URLs (converted to the raw file automatically), and "
    "Open VSX extension pages"
)


@dataclass(frozen=True)
class MarketplaceEntry:
    """One selectable theme file behind a marketplace reference."""

    source_id: str
    publisher: str
    extension: str
    theme_label: str
    description: str
    download_url: str

    def __post_init__(self) -> None:
        if not self.source_id.strip():
            raise ValueError("marketplace source_id must not be empty")
        if not self.download_url.strip():
            raise ValueError("marketplace download_url must not be empty")


class MarketplaceTransport(Protocol):
    """The two registry seams the client needs; fakes replace both in tests."""

    def search(self, query: str, *, limit: int) -> tuple[MarketplaceEntry, ...]:
        """Return at most ``limit`` entries matching free text."""
        ...

    def lookup(
        self, publisher: str, extension: str
    ) -> tuple[MarketplaceEntry, ...]:
        """Return every theme entry one extension contributes, or ``()``."""
        ...

    def fetch_bytes(self, entry: MarketplaceEntry) -> bytes:
        """Return the raw theme file for one entry selected earlier."""
        ...


def normalize_query(query: str) -> str:
    """Strip one search query, rejecting the empty string."""
    cleaned = query.strip()
    if not cleaned:
        raise ValueError("marketplace search needs a non-empty query")
    return cleaned


def clamp_limit(limit: int) -> int:
    """Clamp one requested page size to the bounded search scope."""
    return min(max(int(limit), 1), MAX_SEARCH_LIMIT)


def slugify_theme_label(label: str) -> str:
    """Derive the storage-stem fallback for one marketplace theme label."""
    return _SLUG_RUN_RE.sub("-", label.strip().lower()).strip("-")


def _wrap_network(action: str, exc: Exception) -> MarketplaceError:
    return MarketplaceError(
        f"marketplace {action} failed: {exc}", kind="network"
    )


def search_marketplace(
    query: str,
    *,
    transport: MarketplaceTransport,
    limit: int = DEFAULT_SEARCH_LIMIT,
) -> tuple[MarketplaceEntry, ...]:
    """Search through one transport, bounding the page and naming outages."""
    cleaned = normalize_query(query)
    bounded = clamp_limit(limit)
    try:
        entries = transport.search(cleaned, limit=bounded)
    except MarketplaceError:
        raise
    except (OSError, TimeoutError, ValueError) as exc:
        raise _wrap_network("search", exc) from exc
    return tuple(entries[:bounded])


def convert_page_url(ref: str) -> str | None:
    """Convert one web-page URL to the supported reference behind it.

    GitHub file pages convert to the raw file URL for the same revision
    and path; Open VSX extension pages convert to the
    ``publisher/extension`` reference the registry lookup takes. Anything
    else — gallery search pages, other hosts, other paths — returns
    ``None``: the page is explained, never guessed at. Conversion is a
    string rewrite only; every bound still applies to what is fetched.
    """
    page = _GITHUB_PAGE_RE.match(ref.strip())
    if page is not None:
        return (
            "https://raw.githubusercontent.com/"
            f"{page.group('owner')}/{page.group('repo')}/"
            f"{page.group('rev')}/{page.group('path')}"
        )
    extension_page = _OPEN_VSX_PAGE_RE.match(ref.strip())
    if extension_page is not None:
        return (
            f"{extension_page.group('publisher')}"
            f"/{extension_page.group('extension')}"
        )
    return None


def _direct_entry(ref: str) -> MarketplaceEntry:
    match = _URL_SCHEME_RE.match(ref)
    if match is None:
        raise AssertionError("unreachable: non-URL ref in _direct_entry")
    if match.group(1).lower() not in {"http", "https"}:
        raise MarketplaceError(
            f"marketplace source {ref!r} is unknown: {_SUPPORTED_FORMS}",
            kind="unknown-source",
        )
    return MarketplaceEntry(
        source_id=ref,
        publisher="",
        extension="",
        theme_label=ref,
        description="",
        download_url=ref,
    )


def _lookup_entries(
    publisher: str, extension: str, *, transport: MarketplaceTransport
) -> tuple[MarketplaceEntry, ...]:
    try:
        return tuple(transport.lookup(publisher, extension))
    except MarketplaceError:
        raise
    except (OSError, TimeoutError, ValueError) as exc:
        raise _wrap_network("lookup", exc) from exc


def _select_entry(
    ref: str,
    publisher: str,
    extension: str,
    selector: str | None,
    entries: tuple[MarketplaceEntry, ...],
) -> MarketplaceEntry:
    if not entries:
        raise MarketplaceError(
            f"marketplace source {ref!r} is unknown: "
            f"no themes found for {publisher}/{extension}",
            kind="unknown-source",
        )
    if selector is None:
        if len(entries) == 1:
            return entries[0]
        labels = ", ".join(
            f"{index + 1}. {entry.theme_label}"
            for index, entry in enumerate(entries)
        )
        raise MarketplaceError(
            f"marketplace source {ref!r} contributes {len(entries)} themes; "
            f"select one of: {labels}",
            kind="ambiguous-source",
        )
    if selector.isdigit():
        index = int(selector) - 1
        if 0 <= index < len(entries):
            return entries[index]
    lowered = selector.lower()
    for entry in entries:
        if entry.theme_label.lower() == lowered:
            return entry
    labels = ", ".join(entry.theme_label for entry in entries)
    raise MarketplaceError(
        f"marketplace source {ref!r} is unknown: "
        f"{selector!r} matches none of: {labels}",
        kind="unknown-source",
    )


def resolve_marketplace_source(
    ref: str, *, transport: MarketplaceTransport
) -> MarketplaceEntry:
    """Turn one user-selected reference into the entry to fetch.

    Accepted forms are a direct ``http(s)`` URL to a raw theme JSON file
    or ``publisher/extension`` with an optional ``/theme`` selector (a
    1-based index or the exact theme label). GitHub file pages convert to
    the raw file URL and Open VSX extension pages convert to the
    ``publisher/extension`` reference before anything is fetched. Anything
    else is an unknown source, never a trust decision.
    """
    cleaned = ref.strip()
    if not cleaned:
        raise ValueError("marketplace source must not be empty")
    converted = convert_page_url(cleaned)
    if converted is not None:
        cleaned = converted
    if _URL_SCHEME_RE.match(cleaned) is not None:
        host = urllib.parse.urlsplit(cleaned).hostname or ""
        path = urllib.parse.urlsplit(cleaned).path
        if host.lower() in _GALLERY_HOSTS and not path.startswith("/api/"):
            raise MarketplaceError(
                f"marketplace source {ref!r} is a gallery web page, not a "
                f"theme file: {_SUPPORTED_FORMS}",
                kind="unknown-source",
            )
        return _direct_entry(cleaned)
    parts = cleaned.split("/")
    if len(parts) not in (2, 3) or not all(parts[:2]):
        raise MarketplaceError(
            f"marketplace source {ref!r} is unknown: {_SUPPORTED_FORMS}",
            kind="unknown-source",
        )
    publisher, extension = parts[0], parts[1]
    if (
        _EXTENSION_PART_RE.fullmatch(publisher) is None
        or _EXTENSION_PART_RE.fullmatch(extension) is None
    ):
        raise MarketplaceError(
            f"marketplace source {ref!r} is unknown: "
            "publisher and extension use letters, digits, dot, dash, underscore",
            kind="unknown-source",
        )
    selector = parts[2].strip() or None if len(parts) == 3 else None
    entries = _lookup_entries(publisher, extension, transport=transport)
    return _select_entry(cleaned, publisher, extension, selector, entries)


def _require_http_url(url: str) -> str:
    match = _URL_SCHEME_RE.match(url.strip())
    if match is None or match.group(1).lower() not in {"http", "https"}:
        raise MarketplaceError(
            f"marketplace source {url!r} is unknown: "
            "only http(s) download URLs are supported",
            kind="unknown-source",
        )
    return url.strip()


def fetch_marketplace_bytes(
    entry: MarketplaceEntry,
    *,
    transport: MarketplaceTransport,
    max_bytes: int = MAX_MARKETPLACE_BYTES,
) -> bytes:
    """Fetch one entry's raw bytes, enforcing the size bound before parsing."""
    _require_http_url(entry.download_url)
    try:
        data = transport.fetch_bytes(entry)
    except MarketplaceError:
        raise
    except (OSError, TimeoutError, ValueError) as exc:
        raise _wrap_network("fetch", exc) from exc
    if len(data) > max_bytes:
        raise MarketplaceError(
            f"marketplace source {entry.source_id!r} is {len(data)} bytes, "
            f"past the {max_bytes}-byte bound; a web page URL fetches the "
            "page, not the file, so use the raw theme file URL instead "
            "(GitHub file pages convert automatically, other pages do not)",
            kind="oversized",
        )
    return bytes(data)


def _read_capped(response: Any, *, max_bytes: int) -> bytes:
    """Read one HTTP body, refusing to buffer past the bound."""
    chunks: list[bytes] = []
    total = 0
    while True:
        chunk = response.read(65536)
        if not chunk:
            break
        total += len(chunk)
        if total > max_bytes + 1:
            raise MarketplaceError(
                f"marketplace download is past the {max_bytes}-byte bound",
                kind="oversized",
            )
        chunks.append(chunk)
    return b"".join(chunks)


class UrllibMarketplaceTransport:
    """The default registry transport over the standard library.

    Search and lookup speak the Open VSX registry API; theme files are
    plain ``package.json`` ``contributes.themes`` paths fetched as raw
    bytes. Everything read is JSON-decoded as data and size-capped.
    """

    def __init__(
        self,
        base_url: str = DEFAULT_REGISTRY_BASE_URL,
        *,
        timeout: float = FETCH_TIMEOUT_SECONDS,
        max_bytes: int = MAX_MARKETPLACE_BYTES,
        urlopen: Any = None,
    ) -> None:
        self._base_url = base_url.rstrip("/")
        self._timeout = timeout
        self._max_bytes = max_bytes
        self._urlopen = urlopen if urlopen is not None else urllib.request.urlopen

    def _get_json(self, url: str) -> Any:
        request = urllib.request.Request(
            url, headers={"User-Agent": "talaria-theme-import"}
        )
        try:
            with self._urlopen(request, timeout=self._timeout) as response:
                status = getattr(response, "status", 200)
                if status == 404:
                    raise MarketplaceError(
                        f"marketplace registry has no document at {url}",
                        kind="unknown-source",
                    )
                if status != 200:
                    raise MarketplaceError(
                        f"marketplace registry answered HTTP {status} for {url}",
                        kind="network",
                    )
                body = _read_capped(response, max_bytes=self._max_bytes)
        except MarketplaceError:
            raise
        except urllib.error.HTTPError as exc:
            if exc.code == 404:
                raise MarketplaceError(
                    f"marketplace registry has no document at {url}",
                    kind="unknown-source",
                ) from exc
            raise _wrap_network(f"GET {url}", exc) from exc
        except (OSError, TimeoutError, ValueError) as exc:
            raise _wrap_network(f"GET {url}", exc) from exc
        try:
            return json.loads(body)
        except (json.JSONDecodeError, ValueError, UnicodeDecodeError) as exc:
            raise MarketplaceError(
                f"marketplace registry answered invalid JSON for {url}: {exc}",
                kind="network",
            ) from exc

    def _file_url(
        self, publisher: str, extension: str, version: str, path: str
    ) -> str:
        quoted = "/".join(urllib.parse.quote(part) for part in path.split("/"))
        return (
            f"{self._base_url}/api/{publisher}/{extension}/{version}"
            f"/file/{quoted}"
        )

    def _themes_from_package(
        self,
        publisher: str,
        extension: str,
        version: str,
        package: Any,
        *,
        description: str,
    ) -> tuple[MarketplaceEntry, ...]:
        if not isinstance(package, dict):
            return ()
        contributes = package.get("contributes")
        themes = contributes.get("themes") if isinstance(contributes, dict) else None
        if not isinstance(themes, list):
            return ()
        entries: list[MarketplaceEntry] = []
        for index, raw in enumerate(themes):
            if not isinstance(raw, dict):
                continue
            label = raw.get("label")
            path = raw.get("path")
            if not isinstance(label, str) or not label.strip():
                continue
            if not isinstance(path, str) or not path.strip():
                continue
            entries.append(
                MarketplaceEntry(
                    source_id=f"{publisher}/{extension}/{index + 1}",
                    publisher=publisher,
                    extension=extension,
                    theme_label=label.strip(),
                    description=description,
                    download_url=self._file_url(
                        publisher, extension, version, path.strip().lstrip("./")
                    ),
                )
            )
        return tuple(entries)

    def lookup(
        self, publisher: str, extension: str
    ) -> tuple[MarketplaceEntry, ...]:
        metadata = self._get_json(f"{self._base_url}/api/{publisher}/{extension}")
        if not isinstance(metadata, dict):
            return ()
        version = metadata.get("version")
        if not isinstance(version, str) or not version:
            versions = metadata.get("allVersions")
            if isinstance(versions, dict) and versions:
                version = next(iter(versions))
        if not isinstance(version, str) or not version:
            return ()
        description = metadata.get("description")
        package = self._get_json(
            self._file_url(publisher, extension, version, "package.json")
        )
        return self._themes_from_package(
            publisher,
            extension,
            version,
            package,
            description=description if isinstance(description, str) else "",
        )

    def search(self, query: str, *, limit: int) -> tuple[MarketplaceEntry, ...]:
        cleaned = normalize_query(query)
        bounded = clamp_limit(limit)
        params = urllib.parse.urlencode(
            {
                "query": cleaned,
                "size": str(bounded),
                "includeAllVersions": "false",
            }
        )
        payload = self._get_json(f"{self._base_url}/api/-/search?{params}")
        extensions = payload.get("extensions") if isinstance(payload, dict) else None
        if not isinstance(extensions, list):
            raise MarketplaceError(
                "marketplace registry search answered without an extensions list",
                kind="network",
            )
        found: list[MarketplaceEntry] = []
        for raw in extensions:
            if len(found) >= bounded or not isinstance(raw, dict):
                continue
            namespace = raw.get("namespace")
            name = raw.get("name")
            if not isinstance(namespace, str) or not isinstance(name, str):
                continue
            try:
                found.extend(self.lookup(namespace, name))
            except MarketplaceError as exc:
                if exc.kind == "unknown-source":
                    continue
                raise
            if len(found) >= bounded:
                break
        return tuple(found[:bounded])

    def fetch_bytes(self, entry: MarketplaceEntry) -> bytes:
        url = _require_http_url(entry.download_url)
        request = urllib.request.Request(
            url, headers={"User-Agent": "talaria-theme-import"}
        )
        try:
            with self._urlopen(request, timeout=self._timeout) as response:
                status = getattr(response, "status", 200)
                if status == 404:
                    raise MarketplaceError(
                        f"marketplace source {entry.source_id!r} is unknown: "
                        "the registry no longer serves its file",
                        kind="unknown-source",
                    )
                if status != 200:
                    raise MarketplaceError(
                        f"marketplace fetch answered HTTP {status} "
                        f"for {entry.source_id!r}",
                        kind="network",
                    )
                return _read_capped(response, max_bytes=self._max_bytes)
        except MarketplaceError:
            raise
        except urllib.error.HTTPError as exc:
            if exc.code == 404:
                raise MarketplaceError(
                    f"marketplace source {entry.source_id!r} is unknown: "
                    "the registry no longer serves its file",
                    kind="unknown-source",
                ) from exc
            raise _wrap_network("fetch", exc) from exc
        except (OSError, TimeoutError, ValueError) as exc:
            raise _wrap_network("fetch", exc) from exc


__all__ = [
    "DEFAULT_REGISTRY_BASE_URL",
    "DEFAULT_SEARCH_LIMIT",
    "FETCH_TIMEOUT_SECONDS",
    "MAX_MARKETPLACE_BYTES",
    "MAX_SEARCH_LIMIT",
    "MarketplaceEntry",
    "MarketplaceError",
    "MarketplaceErrorKind",
    "MarketplaceTransport",
    "UrllibMarketplaceTransport",
    "clamp_limit",
    "convert_page_url",
    "fetch_marketplace_bytes",
    "normalize_query",
    "resolve_marketplace_source",
    "search_marketplace",
    "slugify_theme_label",
]
