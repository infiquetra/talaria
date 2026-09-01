"""Framework-import boundaries for Talaria's non-user-interface packages."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from types import ModuleType

import pytest

import talaria.config
import talaria.status
import talaria.themes
import talaria.transport
from tests.domain.test_boundary import (
    _ALLOWED_INTERNAL_EXACT,
    _SWEEP,
    _package_module_names,
)


def _package_modules(package: ModuleType) -> tuple[str, ...]:
    package_file = package.__file__
    assert package_file is not None, f"{package.__name__} has no filesystem package root"
    return tuple(_package_module_names(package.__name__, Path(package_file).parent))


_FRAMEWORK_FREE_IMPORTS: tuple[
    tuple[str, tuple[str, ...], tuple[str, ...]], ...
] = (
    ("text", ("talaria.text",), ("talaria.text",)),
    ("themes", _package_modules(talaria.themes), ("talaria.themes",)),
    (
        "status",
        _package_modules(talaria.status),
        ("talaria.status", "talaria.domain", "talaria.recorder", "talaria.text"),
    ),
    (
        "transport",
        _package_modules(talaria.transport),
        ("talaria.transport", "talaria.domain", "talaria.recorder"),
    ),
    (
        "config",
        ("talaria.config",),
        (
            "talaria.config",
            "talaria.status",
            "talaria.themes",
            "talaria.domain",
            "talaria.recorder",
            "talaria.text",
        ),
    ),
)


@pytest.mark.parametrize(
    ("scope", "module_names", "allowed_trees"),
    _FRAMEWORK_FREE_IMPORTS,
    ids=tuple(scope for scope, _, _ in _FRAMEWORK_FREE_IMPORTS),
)
def test_framework_free_modules_import_only_their_allowed_dependencies(
    scope: str,
    module_names: tuple[str, ...],
    allowed_trees: tuple[str, ...],
) -> None:
    assert module_names, f"the import sweep found no modules under {scope}"

    payload = json.dumps(
        [module_names, list(_ALLOWED_INTERNAL_EXACT), list(allowed_trees)]
    )
    completed = subprocess.run(
        [sys.executable, "-c", _SWEEP, payload],
        capture_output=True,
        text=True,
        check=False,
    )
    assert completed.returncode == 0, (
        f"importing {scope} failed outright:\n"
        f"{completed.stdout}\n{completed.stderr}"
    )

    observed = json.loads(completed.stdout)
    assert not observed["third_party"], (
        f"{scope} pulled in a third-party framework package: "
        f"{observed['third_party']}"
    )
    assert not observed["internal"], (
        f"{scope} pulled in a Talaria package outside "
        f"{list(allowed_trees)}: {observed['internal']}"
    )
