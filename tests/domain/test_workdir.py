"""C13 window one: the status matrix, normalisation, and the filesystem ban.

Every row of the status table in issue #157 appears below as a named case, so
a reader can hold the table and the test side by side. The two readings the
table forces but does not spell out — a first report that differs stays
``not-adopted`` even if a later report lands on the requested directory, and a
move away and back reads ``adopted`` — are pinned with the table wording that
decides them, because a pure function must do *something* there and silent
choice is how divergence returns.
"""

from __future__ import annotations

import os
import pathlib

import pytest

from talaria.domain.workdir import DirectoryStatus, directory_status, normalize_cwd


@pytest.mark.parametrize(
    ("requested", "first", "latest", "expected"),
    [
        # No reply or event has named a directory yet.
        pytest.param("/talaria", None, None, "unreported", id="nothing-reported-yet"),
        pytest.param(None, None, None, "unreported", id="resume-names-nothing-yet"),
        # A directory was reported and none was requested (a resumed session).
        pytest.param(
            None, "/sessions/alpha", "/sessions/alpha", "reported", id="resumed-shows-own"
        ),
        pytest.param(
            None,
            "/sessions/alpha",
            "/sessions/beta",
            "reported",
            id="resume-never-adopts-later-reports",
        ),
        # The reported directory equals the requested one after normalisation.
        pytest.param(
            "/talaria", "/talaria", "/talaria", "adopted", id="exact-match-adopts"
        ),
        pytest.param(
            "/talaria",
            "/talaria/",
            "/talaria/./sub/..",
            "adopted",
            id="spelling-does-not-defeat-adoption",
        ),
        # A directory was requested and the first report differs.
        pytest.param(
            "/talaria",
            "/var/gateway",
            "/var/gateway",
            "not-adopted",
            id="gateway-default-instead-of-request",
        ),
        pytest.param(
            "/talaria",
            "/var/gateway",
            "/talaria",
            "not-adopted",
            id="first-report-rules-despite-later-coincidence",
        ),
        # Adopted, then a later report differs: the agent changed directory.
        pytest.param(
            "/talaria", "/talaria", "/var/gateway", "moved", id="agent-moved-away"
        ),
        # One report in hand serves as both first and latest.
        pytest.param("/talaria", None, "/talaria", "adopted", id="single-report-adopts"),
        pytest.param(
            "/talaria", None, "/var/gateway", "not-adopted", id="single-report-differs"
        ),
    ],
)
def test_directory_status_matrix(
    requested: str | None,
    first: str | None,
    latest: str | None,
    expected: DirectoryStatus,
) -> None:
    assert directory_status(requested, first, latest) == expected


@pytest.mark.parametrize(
    ("raw", "normal"),
    [
        ("/talaria/", "/talaria"),
        ("/talaria/./live", "/talaria/live"),
        ("/talaria/work/../live", "/talaria/live"),
        ("/talaria//live", "/talaria/live"),
        ("/", "/"),
        ("relative/dir/", "relative/dir"),
    ],
)
def test_normalize_cwd_is_textual(raw: str, normal: str) -> None:
    assert normalize_cwd(raw) == normal


def test_normalize_cwd_is_case_sensitive() -> None:
    assert normalize_cwd("/Talaria") != normalize_cwd("/talaria")


def test_normalize_cwd_agrees_with_itself_on_either_spelling() -> None:
    assert directory_status("/talaria/", "/talaria", "/talaria/.") == "adopted"


def test_directory_status_never_touches_the_filesystem(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The domain performs no filesystem access (ADR-0002).

    Every resolution-shaped call raises, so a future helper that reaches for
    the disk fails here instead of passing differently on each machine.
    """

    def _forbidden(*args: object, **kwargs: object) -> object:
        raise AssertionError("filesystem access in pure domain code")

    monkeypatch.setattr(os, "getcwd", _forbidden)
    monkeypatch.setattr(os.path, "abspath", _forbidden)
    monkeypatch.setattr(os.path, "realpath", _forbidden)
    monkeypatch.setattr(os.path, "expanduser", _forbidden)
    monkeypatch.setattr(pathlib.Path, "resolve", _forbidden)
    monkeypatch.setattr(pathlib.Path, "exists", _forbidden)
    assert directory_status("/talaria", "/talaria/", "/talaria/./sub/..") == "adopted"
    assert normalize_cwd("/talaria//live/") == "/talaria/live"
