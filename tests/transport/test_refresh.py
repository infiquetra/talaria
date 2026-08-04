"""``talaria refresh-credential``: fetch the dashboard's token, write it at 0600.

The assertions that matter most here are refusals. This module fetches a
credential over the network and writes it to disk, so the interesting questions
are which addresses it declines to fetch from, which mode the file ends up with,
and whether the value can escape into a message, a report, or a preserved line.
"""

from __future__ import annotations

import contextlib
import stat
import threading
from collections.abc import Iterator
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from typing import Any

import pytest

from talaria.transport.refresh import (
    RefreshError,
    dashboard_origin_for,
    extract_session_token,
    fetch_dashboard_index,
    refresh_credential,
    write_token,
)

#: One distinctive value, searched for wherever it must not appear.
CANARY = "canary-Kf82ns-do-not-leak"

TOKEN_PAGE = (
    "<!doctype html><html><head>"
    f'<script>window.__HERMES_SESSION_TOKEN__="{CANARY}";'
    "window.__HERMES_DASHBOARD_EMBEDDED_CHAT__=true;"
    'window.__HERMES_BASE_PATH__="";'
    "window.__HERMES_AUTH_REQUIRED__=false;</script>"
    "</head><body></body></html>"
)

GATED_PAGE = (
    "<!doctype html><html><head>"
    "<script>window.__HERMES_DASHBOARD_EMBEDDED_CHAT__=true;"
    'window.__HERMES_BASE_PATH__="";'
    "window.__HERMES_AUTH_REQUIRED__=true;</script>"
    "</head><body></body></html>"
)


@contextlib.contextmanager
def dashboard_serving(body: str) -> Iterator[str]:
    """A loopback HTTP server answering every GET with ``body``."""

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            payload = body.encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, format: str, *args: Any) -> None:
            return

    server = HTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}/"
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


# ── deriving the dashboard address ───────────────────────────────────────


@pytest.mark.parametrize(
    ("endpoint", "expected"),
    [
        ("ws://127.0.0.1:9119/api/ws", "http://127.0.0.1:9119/"),
        ("wss://gateway.example:443/api/ws", "https://gateway.example:443/"),
        ("http://127.0.0.1:9119/", "http://127.0.0.1:9119/"),
        ("ws://localhost:8765/api/ws", "http://localhost:8765/"),
    ],
)
def test_the_dashboard_address_comes_from_the_gateway_endpoint(
    endpoint: str, expected: str
) -> None:
    """One configured endpoint, so the refresh cannot target a different box."""
    assert dashboard_origin_for(endpoint) == expected


def test_deriving_the_address_drops_any_credential_on_the_endpoint() -> None:
    """Belt and braces: callers pass a stripped URL, and this strips again."""
    origin = dashboard_origin_for(f"ws://127.0.0.1:9119/api/ws?token={CANARY}")

    assert origin == "http://127.0.0.1:9119/"
    assert CANARY not in origin


def test_an_endpoint_with_no_host_is_refused() -> None:
    with pytest.raises(RefreshError, match="names no host"):
        dashboard_origin_for("ws:///api/ws")


# ── what it refuses to fetch from ────────────────────────────────────────


@pytest.mark.parametrize("origin", ["file:///etc/passwd", "ftp://127.0.0.1/x", "gopher://x/"])
def test_only_http_and_https_are_fetched(origin: str) -> None:
    """B310's actual concern: ``urlopen`` will read ``file:`` if it is handed one.

    A wrong ``--from`` should be a refusal, not an arbitrary-file read whose
    contents get scanned for something that looks like a token.
    """
    with pytest.raises(RefreshError, match="must be http or https"):
        fetch_dashboard_index(origin)


def test_a_cleartext_fetch_from_another_machine_is_refused() -> None:
    """A token read over plain HTTP off-box is a token given to the network."""
    with pytest.raises(RefreshError, match="plain HTTP"):
        fetch_dashboard_index("http://dashboard.example:9119/")


def test_an_unreachable_dashboard_is_a_named_failure() -> None:
    # Port 1 on loopback: reserved, and nothing listens there.
    with pytest.raises(RefreshError, match="no dashboard answered"):
        fetch_dashboard_index("http://127.0.0.1:1/", timeout=5)


# ── reading the token out of the page ────────────────────────────────────


def test_the_token_is_read_from_the_bootstrap_script() -> None:
    assert extract_session_token(TOKEN_PAGE) == CANARY


def test_a_gated_dashboard_reports_the_unbuilt_provider_not_a_missing_token() -> None:
    """The OAuth gate does not inject a token at all; it issues per-dial tickets.

    Reporting that as "no token found" would send an operator looking for a
    broken dashboard, when what they have is a dashboard v0.1 cannot dial.
    """
    with pytest.raises(RefreshError, match="OAuth gate"):
        extract_session_token(GATED_PAGE)


def test_a_page_with_no_token_and_no_gate_says_so() -> None:
    with pytest.raises(RefreshError, match="carried no session token"):
        extract_session_token("<!doctype html><html></html>")


# ── writing the credential file ──────────────────────────────────────────


def _mode(path: Path) -> int:
    return stat.S_IMODE(path.stat().st_mode)


def test_a_new_credential_file_is_created_at_0600(tmp_path: Path) -> None:
    path = tmp_path / "credentials"

    created, tightened, preserved = write_token(path, CANARY)

    assert created is True
    assert tightened is False
    assert preserved == ()
    assert _mode(path) == 0o600
    assert path.read_text(encoding="utf-8") == f'token = "{CANARY}"\n'


def test_refreshing_preserves_the_endpoint_and_the_operators_comments(
    tmp_path: Path,
) -> None:
    """A refresh must not quietly discard the ``url`` key or reformat the file."""
    path = tmp_path / "credentials"
    path.write_text(
        '# the staging gateway, do not point this at prod\nurl = "ws://127.0.0.1:9119/api/ws"\n'
        'token = "the-token-that-died-with-the-last-dashboard"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)

    created, tightened, preserved = write_token(path, CANARY)

    text = path.read_text(encoding="utf-8")
    assert created is False
    assert preserved == ("url",)
    assert "# the staging gateway, do not point this at prod" in text
    assert 'url = "ws://127.0.0.1:9119/api/ws"' in text
    assert f'token = "{CANARY}"' in text
    assert "died-with-the-last-dashboard" not in text, "the stale token survived the refresh"


def test_a_file_without_a_token_key_gains_one(tmp_path: Path) -> None:
    path = tmp_path / "credentials"
    path.write_text('url = "ws://127.0.0.1:9119/api/ws"\n', encoding="utf-8")
    path.chmod(0o600)

    write_token(path, CANARY)

    assert 'url = "ws://127.0.0.1:9119/api/ws"' in path.read_text(encoding="utf-8")
    assert f'token = "{CANARY}"' in path.read_text(encoding="utf-8")


def test_a_loose_credential_file_is_tightened_and_the_tightening_is_reported(
    tmp_path: Path,
) -> None:
    """The provider refuses to read a loose file, so leaving it loose is a dead end."""
    path = tmp_path / "credentials"
    path.write_text('token = "stale"\n', encoding="utf-8")
    path.chmod(0o644)

    _, tightened, _ = write_token(path, CANARY)

    assert tightened is True
    assert _mode(path) == 0o600


def test_a_malformed_file_is_preserved_without_claiming_which_keys_survived(
    tmp_path: Path,
) -> None:
    """Reporting a key that was never parsed would be a guess dressed as a fact."""
    path = tmp_path / "credentials"
    path.write_text("this = is = not = toml\n", encoding="utf-8")
    path.chmod(0o600)

    _, _, preserved = write_token(path, CANARY)

    assert preserved == ()
    assert "this = is = not = toml" in path.read_text(encoding="utf-8")


def test_no_temporary_file_is_left_beside_the_credential(tmp_path: Path) -> None:
    """A leftover temp file would hold a live credential under a stale name.

    Written into its own directory, which also exercises the ``mkdir`` for a
    config directory that does not exist yet.
    """
    path = tmp_path / "fresh-config-dir" / "credentials"
    write_token(path, CANARY)

    assert sorted(p.name for p in path.parent.iterdir()) == ["credentials"]


# ── end to end, against a real socket ────────────────────────────────────


def test_a_refresh_fetches_writes_and_never_reports_the_token(tmp_path: Path) -> None:
    """The whole command path over a real HTTP server on loopback."""
    path = tmp_path / "credentials"

    with dashboard_serving(TOKEN_PAGE) as origin:
        report = refresh_credential(origin, path, timeout=10)

    assert report.created is True
    assert report.origin == origin
    assert _mode(path) == 0o600
    assert f'token = "{CANARY}"' in path.read_text(encoding="utf-8")

    # The report is the object a caller prints. It must not carry the value.
    assert CANARY not in repr(report)
    assert CANARY not in str(report)


def test_a_gated_dashboard_writes_nothing(tmp_path: Path) -> None:
    """A refusal must not truncate the credential the operator already had."""
    path = tmp_path / "credentials"
    path.write_text('token = "the-existing-one"\n', encoding="utf-8")
    path.chmod(0o600)

    with dashboard_serving(GATED_PAGE) as origin:
        with pytest.raises(RefreshError, match="OAuth gate"):
            refresh_credential(origin, path, timeout=10)

    assert path.read_text(encoding="utf-8") == 'token = "the-existing-one"\n'
