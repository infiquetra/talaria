"""tests/test_packaging.py — the version has exactly one source of truth.

Talaria declared its version twice before v0.1.0 was cut: a literal in
``pyproject.toml`` and a separate ``__version__`` in ``talaria/__init__.py``,
with nothing reconciling them. Two literals that must agree are a drift waiting
to happen, and the drift is silent — a wheel says one number, the running
program says another, and a bug report cannot be tied to a build.

The fix is structural rather than assertive: ``pyproject.toml`` declares the
version ``dynamic`` and hatchling reads it from the package. These tests guard
the structure, because a future edit that re-adds a static literal would restore
the drift without breaking anything visible.

What these tests *cannot* see is the built distribution's metadata. That is
asserted in ``.github/workflows/validate.yml``'s install job, against a real
installed artifact.
"""

from __future__ import annotations

import tomllib
from pathlib import Path
from typing import Any

import pytest

import talaria.cli as cli_module
from talaria import __version__
from talaria.cli import main, parse_args

_REPO_ROOT = Path(__file__).resolve().parent.parent
_PYPROJECT = _REPO_ROOT / "pyproject.toml"


def _pyproject() -> dict[str, Any]:
    data: dict[str, Any] = tomllib.loads(_PYPROJECT.read_text(encoding="utf-8"))
    return data


def test_the_version_is_not_written_a_second_time_in_pyproject() -> None:
    project = _pyproject()["project"]
    assert "version" not in project, (
        "pyproject.toml declares a static `version`, which reintroduces the drift "
        "the dynamic declaration exists to prevent: this literal and "
        "talaria/__init__.py's `__version__` would then both be sources of truth "
        "and nothing would make them agree."
    )
    assert "version" in project.get("dynamic", []), (
        "pyproject.toml neither declares a static `version` nor lists `version` in "
        "`dynamic`, so the distribution has no version at all and the build fails."
    )


def test_the_declared_version_source_is_the_file_that_defines_the_literal() -> None:
    """hatchling's configured path must be the file ``__version__`` lives in.

    Moving the literal without moving the pointer breaks the build rather than
    the runtime, which is a slower and more confusing failure than this.
    """
    configured = _pyproject()["tool"]["hatch"]["version"]["path"]
    source = (_REPO_ROOT / configured).read_text(encoding="utf-8")
    assert f'__version__ = "{__version__}"' in source, (
        f"pyproject.toml points hatchling at {configured!r} for the version, but "
        f"that file does not declare the `__version__` the package imports "
        f"({__version__!r})."
    )


def test_the_source_distribution_names_what_it_ships() -> None:
    """The sdist must be an allow-list, because this repository is public.

    Hatchling's default is "everything version control does not ignore", which
    includes *untracked* files. Building v0.1.0 locally swept
    ``.claude/settings.local.json`` — never committed, never ignored — into the
    tarball. That instance was harmless; the shape is not, because it makes the
    contents of a release artifact depend on what was lying in the builder's
    working directory. A deny-list cannot exclude a file nobody anticipated.
    """
    sdist = _pyproject()["tool"]["hatch"]["build"]["targets"]["sdist"]
    assert sdist.get("include"), (
        "pyproject.toml does not give [tool.hatch.build.targets.sdist] an `include` "
        "allow-list, so the source distribution falls back to shipping every "
        "unignored file in the build directory — including untracked ones."
    )


def test_the_version_flag_prints_the_package_version_and_exits_clean(
    capsys: pytest.CaptureFixture[str],
) -> None:
    with pytest.raises(SystemExit) as exit_info:
        parse_args(["--version"])
    assert exit_info.value.code == 0
    assert capsys.readouterr().out.strip() == f"talaria {__version__}"


def test_the_version_flag_never_reaches_the_launcher(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """``--version`` answers without dialling, proved with a double.

    It is the first thing anyone runs against a fresh install and the thing a
    bug report asks for, so it has to answer on a machine where nothing else
    works — no gateway, no credential file, no configuration. ``run_live`` is
    replaced rather than called for the reason the neighbouring CLI tests give:
    calling it would dial whatever the default gateway URL resolves to, and on
    a development machine that is a real Hermes.
    """
    launched: list[object] = []

    def fake_run_live(args: object) -> int:
        launched.append(args)
        return 0

    monkeypatch.setattr(cli_module, "run_live", fake_run_live)

    with pytest.raises(SystemExit) as exit_info:
        main(["--version"])

    assert exit_info.value.code == 0
    assert launched == []
