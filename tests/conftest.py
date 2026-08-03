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
    """Redirect the global config dir into ``tmp_path`` and clear TALARIA_* vars."""
    global_dir = tmp_path / "global-talaria"
    global_dir.mkdir()
    monkeypatch.setenv("TALARIA_CONFIG_DIR", str(global_dir))
    for env_name in config_module._ENV_KEY_MAP:
        monkeypatch.delenv(env_name, raising=False)
    return global_dir
