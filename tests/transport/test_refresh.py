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
    write_profile_token,
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


# ── v0.4 KTD5: pairing a named profile ───────────────────────────────────

#: A second distinctive value, so a test can prove one entry was written
#: without disturbing the other.
SECOND_CANARY = "canary-Pw41ka-do-not-leak"


def test_pairing_a_profile_creates_the_table_it_needs(tmp_path: Path) -> None:
    path = tmp_path / "credentials"

    created, tightened, preserved = write_profile_token(path, "alpha-fixture", CANARY)

    assert created is True
    assert tightened is False
    assert preserved == ()
    assert _mode(path) == 0o600
    assert path.read_text(encoding="utf-8") == (
        f'[profiles.alpha-fixture]\ntoken = "{CANARY}"\n'
    )


def test_pairing_a_profile_leaves_the_default_entry_and_every_sibling_alone(
    tmp_path: Path,
) -> None:
    """The property that makes a fleet credential file safe to keep editing."""
    path = tmp_path / "credentials"
    path.write_text(
        "# the machine's own gateway\n"
        'url = "ws://127.0.0.1:9119/api/ws"\n'
        'token = "the-default-profiles-token"\n'
        "\n"
        "[profiles.beta-fixture]\n"
        "# paired on a Tuesday\n"
        f'token = "{SECOND_CANARY}"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)

    _, _, preserved = write_profile_token(path, "alpha-fixture", CANARY)

    text = path.read_text(encoding="utf-8")
    assert 'token = "the-default-profiles-token"' in text
    assert f'token = "{SECOND_CANARY}"' in text
    assert f'token = "{CANARY}"' in text
    assert "# paired on a Tuesday" in text
    assert "# the machine's own gateway" in text
    assert set(preserved) == {"url", "token", "profiles.beta-fixture"}


def test_repairing_a_profile_replaces_only_its_own_token_line(tmp_path: Path) -> None:
    """A rewrite must stop at the next table header, or it edits a neighbour."""
    path = tmp_path / "credentials"
    path.write_text(
        "[profiles.alpha-fixture]\n"
        'token = "alphas-dead-token"\n'
        "\n"
        "[profiles.beta-fixture]\n"
        f'token = "{SECOND_CANARY}"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)

    write_profile_token(path, "alpha-fixture", CANARY)

    text = path.read_text(encoding="utf-8")
    assert "alphas-dead-token" not in text
    assert f'token = "{SECOND_CANARY}"' in text
    assert text.count("[profiles.alpha-fixture]") == 1


def test_a_profile_table_without_a_token_gains_one_inside_itself(tmp_path: Path) -> None:
    path = tmp_path / "credentials"
    path.write_text(
        "[profiles.alpha-fixture]\n# nothing paired yet\n\n[profiles.beta-fixture]\n"
        f'token = "{SECOND_CANARY}"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)

    write_profile_token(path, "alpha-fixture", CANARY)

    lines = path.read_text(encoding="utf-8").splitlines()
    alpha = lines.index("[profiles.alpha-fixture]")
    beta = lines.index("[profiles.beta-fixture]")
    assert any(f'token = "{CANARY}"' == line for line in lines[alpha:beta])
    assert f'token = "{SECOND_CANARY}"' in lines[beta:]


def test_refreshing_the_default_profile_never_edits_a_profile_table(
    tmp_path: Path,
) -> None:
    """A ``token`` under a table header belongs to that profile, not to the file."""
    path = tmp_path / "credentials"
    path.write_text(
        "[profiles.alpha-fixture]\n" f'token = "{SECOND_CANARY}"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)

    write_token(path, CANARY)

    text = path.read_text(encoding="utf-8")
    assert f'token = "{SECOND_CANARY}"' in text
    assert text.startswith(f'token = "{CANARY}"\n')


def test_a_profile_name_that_could_rewrite_the_document_is_refused(
    tmp_path: Path,
) -> None:
    """The name reaches a TOML table header, so it is validated before it does."""
    path = tmp_path / "credentials"
    path.write_text('token = "keep-me"\n', encoding="utf-8")
    path.chmod(0o600)

    for name in ('bad"]\n[profiles.other', "dotted.name", "", "-leading"):
        with pytest.raises(RefreshError):
            write_profile_token(path, name, CANARY)

    assert path.read_text(encoding="utf-8") == 'token = "keep-me"\n'


def test_a_profile_refresh_reports_which_entry_it_wrote(tmp_path: Path) -> None:
    path = tmp_path / "credentials"

    with dashboard_serving(TOKEN_PAGE) as origin:
        report = refresh_credential(origin, path, timeout=10, profile="alpha-fixture")

    assert report.profile == "alpha-fixture"
    assert _mode(path) == 0o600
    assert CANARY not in repr(report)
    assert CANARY not in str(report)
    assert "[profiles.alpha-fixture]" in path.read_text(encoding="utf-8")


def test_the_written_profile_entry_is_the_one_the_provider_reads(
    tmp_path: Path,
) -> None:
    """The write and the read must agree, and they are two different modules."""
    import asyncio

    from talaria.transport.credentials import LoopbackTokenProvider

    path = tmp_path / "credentials"
    write_profile_token(path, "alpha-fixture", CANARY)

    credential = asyncio.run(
        LoopbackTokenProvider(
            credentials_path=path, allow_prompt=False, profile="alpha-fixture"
        ).acquire()
    )
    assert credential.value == CANARY
    assert credential.source == "profile-file"


# ── table-header boundaries: comments and array-of-tables (CR2 finding 1) ──
#
# A header the boundary regex cannot see is not a header that gets skipped; it
# is a boundary that is not there, so a rewrite runs straight past it into the
# next profile's table. Both directions below were reproduced against the first
# draft, which anchored the header line to end at ``]`` and therefore missed
# ``[profiles.alpha]  # staging`` — legal TOML an operator would plausibly write.


def test_a_commented_table_header_still_bounds_the_default_rewrite(
    tmp_path: Path,
) -> None:
    """The default's token must not land inside a profile's table (CR2 finding 1a)."""
    path = tmp_path / "credentials"
    path.write_text(
        "[profiles.alpha-fixture]  # staging gateway\n" f'token = "{SECOND_CANARY}"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)

    write_token(path, CANARY)

    text = path.read_text(encoding="utf-8")
    # The profile keeps its own credential, and the default's lands above the
    # header rather than replacing the first ``token`` line it can find.
    assert f'token = "{SECOND_CANARY}"' in text
    assert text.startswith(f'token = "{CANARY}"\n')


def test_a_commented_next_header_bounds_a_profile_insert(tmp_path: Path) -> None:
    """Inserting alpha's token must not reach into beta's table (CR2 finding 1b)."""
    path = tmp_path / "credentials"
    path.write_text(
        "[profiles.alpha-fixture]  # no token yet\n"
        "[profiles.beta-fixture]  # already paired\n"
        f'token = "{SECOND_CANARY}"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)

    write_profile_token(path, "alpha-fixture", CANARY)

    text = path.read_text(encoding="utf-8")
    alpha = text.index("[profiles.alpha-fixture]")
    beta = text.index("[profiles.beta-fixture]")
    # The new token sits between the two headers, i.e. inside alpha, and beta's
    # own credential is untouched.
    assert alpha < text.index(f'token = "{CANARY}"') < beta
    assert f'token = "{SECOND_CANARY}"' in text
    assert text.count(f'token = "{CANARY}"') == 1


def test_an_array_of_tables_header_bounds_a_rewrite(tmp_path: Path) -> None:
    """``[[…]]`` is a boundary even though it is never a profile entry."""
    path = tmp_path / "credentials"
    path.write_text(
        "[[history]]\n" f'token = "{SECOND_CANARY}"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)

    write_token(path, CANARY)

    text = path.read_text(encoding="utf-8")
    assert f'token = "{SECOND_CANARY}"' in text
    assert text.startswith(f'token = "{CANARY}"\n')


def test_a_profile_insert_stops_at_a_table_it_cannot_edit(tmp_path: Path) -> None:
    """Any header ends a span, including ones this writer refuses to edit.

    CR2 round 2. The first remediation taught the *regex* about
    ``[[…]]`` but left ``_rewrite_profile_token``'s boundary loop skipping every
    line ``_table_path`` would not name — which is exactly those headers. So an
    insert into a table with no token line ran past ``[[servers]]`` and replaced
    a foreign table's value: the pairing reported success, the live credential
    landed under a table other readers own, and the profile just "paired" still
    had no token.
    """
    path = tmp_path / "credentials"
    path.write_text(
        "[profiles.alpha-fixture]\n" "[[servers]]\n" f'token = "{SECOND_CANARY}"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)

    write_profile_token(path, "alpha-fixture", CANARY)

    text = path.read_text(encoding="utf-8")
    # The foreign table keeps its own value, alpha gained one, and alpha's sits
    # above the boundary rather than inside the table beyond it.
    assert f'token = "{SECOND_CANARY}"' in text
    assert text.index(f'token = "{CANARY}"') < text.index("[[servers]]")


def test_a_profile_insert_stops_at_a_quoted_key_header(tmp_path: Path) -> None:
    """A quoted-key header is unnameable but still a boundary (CR2 round 2)."""
    path = tmp_path / "credentials"
    path.write_text(
        "[profiles.alpha-fixture]\n" '["ops notes"]\n' f'token = "{SECOND_CANARY}"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)

    write_profile_token(path, "alpha-fixture", CANARY)

    text = path.read_text(encoding="utf-8")
    assert f'token = "{SECOND_CANARY}"' in text
    assert text.index(f'token = "{CANARY}"') < text.index('["ops notes"]')


def test_a_quoted_key_carrying_a_bracket_is_still_a_boundary(tmp_path: Path) -> None:
    """Bounding and recognising are different jobs (CR2 round 3).

    ``["we[ird"]`` is legal TOML whose key the header pattern's charclass cannot
    match — so it was an invisible boundary, and a profile insert ran past it and
    replaced a foreign table's token. Span-ending now asks the loosest question
    that is true of every header: the line opens with ``[``.
    """
    path = tmp_path / "credentials"
    path.write_text(
        "[profiles.alpha-fixture]\n" '["we[ird"]\n' f'token = "{SECOND_CANARY}"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)

    write_profile_token(path, "alpha-fixture", CANARY)

    text = path.read_text(encoding="utf-8")
    assert f'token = "{SECOND_CANARY}"' in text
    assert text.index(f'token = "{CANARY}"') < text.index('["we[ird"]')


def test_a_top_level_write_stops_at_a_bracket_bearing_quoted_key(tmp_path: Path) -> None:
    """The same boundary, on the default profile's path (CR2 round 3)."""
    path = tmp_path / "credentials"
    path.write_text(
        '["we[ird"]\n' f'token = "{SECOND_CANARY}"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)

    write_token(path, CANARY)

    text = path.read_text(encoding="utf-8")
    assert f'token = "{SECOND_CANARY}"' in text
    assert text.startswith(f'token = "{CANARY}"\n')


# ── the verification gate (CR2 round 4) ───────────────────────────────────
#
# Four review rounds found four ways a line-wise scanner misreads TOML
# structure, each silent and each destroying or stranding a credential. The
# scanner is no longer the safety mechanism: the document is parsed before and
# after, and a rewrite that changed anything but the target key is refused
# before it reaches the disk. These pin the refusal for the shapes that were
# demonstrated, and — more importantly — that the file on disk is untouched.


def test_a_header_inside_a_multiline_string_refuses_rather_than_clobbering(
    tmp_path: Path,
) -> None:
    """A false span START: the scanner has no string context (CR2 round 4)."""
    path = tmp_path / "credentials"
    path.write_text(
        "[profiles.alpha-fixture]\n"
        'note = """\n'
        "[profiles.beta-fixture]\n"
        '"""\n'
        f'token = "{SECOND_CANARY}"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)
    before = path.read_text(encoding="utf-8")

    with pytest.raises(RefreshError) as caught:
        write_profile_token(path, "beta-fixture", CANARY)

    # Nothing was written, so alpha keeps the credential its gateway minted.
    assert path.read_text(encoding="utf-8") == before
    assert SECOND_CANARY in path.read_text(encoding="utf-8")
    assert CANARY not in str(caught.value)


def test_a_boundary_inside_a_multiline_string_refuses_the_top_level_write(
    tmp_path: Path,
) -> None:
    """A false span END, and the worst direction: a silently stale token."""
    path = tmp_path / "credentials"
    path.write_text(
        'note = """\n' "[whatever]\n" '"""\n' f'token = "{SECOND_CANARY}"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)
    before = path.read_text(encoding="utf-8")

    with pytest.raises(RefreshError):
        write_token(path, CANARY)

    assert path.read_text(encoding="utf-8") == before


def test_an_insert_that_would_break_the_file_refuses(tmp_path: Path) -> None:
    """A nested inline array: the edit would leave the document unparseable."""
    path = tmp_path / "credentials"
    path.write_text(
        "extras = [\n" "  [1, 2],\n" "]\n" f'token = "{SECOND_CANARY}"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)
    before = path.read_text(encoding="utf-8")

    with pytest.raises(RefreshError) as caught:
        write_token(path, CANARY)

    assert path.read_text(encoding="utf-8") == before
    assert "unparseable" in str(caught.value)


def test_an_ordinary_multi_profile_file_still_writes(tmp_path: Path) -> None:
    """The gate must refuse misreads, not ordinary work."""
    path = tmp_path / "credentials"
    path.write_text(
        f'token = "{SECOND_CANARY}"\n'
        'url = "ws://127.0.0.1:9119/api/ws"\n'
        "\n"
        "[profiles.alpha-fixture]  # staging\n"
        'token = "alpha-value"\n',
        encoding="utf-8",
    )
    path.chmod(0o600)

    write_profile_token(path, "beta-fixture", CANARY)

    import tomllib

    document = tomllib.loads(path.read_text(encoding="utf-8"))
    assert document["token"] == SECOND_CANARY
    assert document["url"] == "ws://127.0.0.1:9119/api/ws"
    assert document["profiles"]["alpha-fixture"]["token"] == "alpha-value"
    assert document["profiles"]["beta-fixture"]["token"] == CANARY
