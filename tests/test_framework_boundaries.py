"""Framework-import boundaries for Talaria's non-user-interface packages."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from types import ModuleType

import pytest

import talaria.status
import talaria.themes
import talaria.transport
from tests.domain.test_boundary import (
    _ALLOWED_INTERNAL_EXACT,
    _SWEEP,
    _package_module_names,
)

_FRAMEWORK_FREE_PACKAGES: tuple[tuple[ModuleType, tuple[str, ...]], ...] = (
    (talaria.themes, ("talaria.themes",)),
    (
        talaria.status,
        ("talaria.status", "talaria.domain", "talaria.recorder"),
    ),
    (
        talaria.transport,
        ("talaria.transport", "talaria.domain", "talaria.recorder"),
    ),
)


@pytest.mark.parametrize(
    ("package", "allowed_trees"),
    _FRAMEWORK_FREE_PACKAGES,
    ids=("themes", "status", "transport"),
)
def test_framework_free_package_imports_only_its_allowed_dependencies(
    package: ModuleType,
    allowed_trees: tuple[str, ...],
) -> None:
    package_file = package.__file__
    assert package_file is not None, f"{package.__name__} has no filesystem package root"
    module_names = _package_module_names(package.__name__, Path(package_file).parent)
    assert module_names, f"the import sweep found no modules under {package.__name__}"

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
        f"importing {package.__name__} failed outright:\n"
        f"{completed.stdout}\n{completed.stderr}"
    )

    observed = json.loads(completed.stdout)
    assert not observed["third_party"], (
        f"{package.__name__} pulled in a third-party framework package: "
        f"{observed['third_party']}"
    )
    assert not observed["internal"], (
        f"{package.__name__} pulled in a Talaria package outside "
        f"{list(allowed_trees)}: {observed['internal']}"
    )
