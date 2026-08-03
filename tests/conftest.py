"""Repository-wide test fixtures.

The isolation below lives here, not in one test file, because it must hold for
every test in the suite. ``talaria.config`` reads the operator's real
``~/.talaria/config.toml`` and — since the repo-local level resolves against
``Path.cwd()`` — the repository's own git-ignored ``./.talaria/``. Any test
that reaches :func:`talaria.config.load_config` without this fixture would pass
or fail on machine-local state, silently. Later units (U3's startup-precedence
tests, U6's status runner) call into config without knowing this fixture
exists, which is exactly why it is autouse and repository-wide.
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
