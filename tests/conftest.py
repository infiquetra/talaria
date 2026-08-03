"""Repository-wide test fixtures.

The isolation below lives here, not in one test file, because it must hold for
every test in the suite. ``talaria.config`` reads the operator's real
``~/.talaria/config.toml`` and — since the repo-local level resolves against
``Path.cwd()`` — the repository's own git-ignored ``./.talaria/``. Any test
that reaches :func:`talaria.config.load_config` without this fixture would pass
or fail on machine-local state, silently. Later units (U3's startup-precedence
tests, U6's status runner) call into config without knowing this fixture
exists, which is exactly why it is autouse and repository-wide.

**Writing a test that reads a fixture file? Read this first.** Because the
fixture below calls ``monkeypatch.chdir(tmp_path)``, every test in this suite
runs from a temporary directory. A repo-relative path like
``Path("tests/recorder/fixtures/frame.jsonl")`` therefore raises
``FileNotFoundError``, and the error says nothing about the working directory.
Anchor fixture paths to the test module instead — use the :func:`fixtures_dir`
fixture below, or ``Path(__file__).parent / "fixtures"``.

**Do not "fix" that FileNotFoundError by deleting the chdir.** It is load
bearing: without it, ``load_config()`` called with no ``cwd`` resolves the
repo-local level against the real repository and reads its git-ignored
``.talaria/config.toml``. KTD15 designs that file for per-project status
commands and KTD5 makes ``status.command`` executable, so removing the chdir
reopens a real hole and the suite starts passing or failing on machine state.
``test_repo_local_level_is_isolated_to_tmp_path`` pins it.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from talaria import config as config_module


@pytest.fixture(autouse=True)
def isolated_global_config_dir(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    """Isolate every level of KTD15's chain that reads from outside the test.

    Three things, and all three are load-bearing:

    * ``TALARIA_CONFIG_DIR`` redirects the *global* level into ``tmp_path``.
    * ``monkeypatch.chdir`` moves the process into ``tmp_path``, which isolates
      the *repo-local* level. Redirecting only the global level is not enough:
      ``load_config()`` resolves ``./.talaria/config.toml`` against
      ``Path.cwd()``, so a test calling it without an explicit ``cwd`` would
      read the repository's own git-ignored ``.talaria/`` — and KTD15 designs
      that file for per-project status commands, so an operator having a real
      one is the expected state, not an exotic one.
    * Every ``TALARIA_*`` variable is cleared so the environment level cannot
      leak in from the operator's shell.

    Autouse and repository-wide because later units (U3's startup-precedence
    tests, U6's status runner) call into config without knowing this exists.
    """
    global_dir = tmp_path / "global-talaria"
    global_dir.mkdir()
    monkeypatch.setenv("TALARIA_CONFIG_DIR", str(global_dir))
    monkeypatch.chdir(tmp_path)
    for env_name in config_module._ENV_KEY_MAP:
        monkeypatch.delenv(env_name, raising=False)
    return global_dir


@pytest.fixture
def fixtures_dir(request: pytest.FixtureRequest) -> Path:
    """The ``fixtures/`` directory beside the requesting test module.

    Anchored to the test file rather than the working directory, so it keeps
    working under the autouse ``chdir`` above. Prefer this over a repo-relative
    path when a test needs a checked-in corpus or frame log.
    """
    return Path(request.path).parent / "fixtures"
