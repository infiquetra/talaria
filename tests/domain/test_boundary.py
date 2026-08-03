"""ADR-0002 boundary check.

Imports every module under ``talaria.domain`` **in a fresh subprocess** and
fails the domain test run if the presentation framework (``textual``) or
``talaria.ui`` landed in that subprocess's ``sys.modules`` afterward. This
lives in the domain package's own test run, not a separate lint step, so it
fails the same way a domain unit test would.

Two properties this check has to hold, both of which an earlier version got
wrong:

*Complete.* The module list comes from a filesystem walk, not
``pkgutil.walk_packages``. ``walk_packages`` cannot see into a directory that
has no ``__init__.py``, so a domain module could be fully importable at
runtime and completely invisible to the sweep. A companion test asserts every
directory under ``talaria/domain`` carries an ``__init__.py``, which keeps the
package a package rather than relying on namespace-package behavior.

*Attributable.* The sweep runs in a subprocess, so what it observes is caused
by ``talaria.domain`` alone. Reading the pytest process's own ``sys.modules``
would fail whenever any other test imported ``textual`` first — which becomes
routine once ``talaria/ui`` lands, since that package is the one place the
framework is allowed.

The "demonstrably red on a deliberate violation" scenario required by U1's
test-scenario list is verified by adding a scratch domain module that imports
textual, running this test once, and discarding it — a committed
red-by-construction fixture would permanently fail this test.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import talaria.domain

_FORBIDDEN_PREFIXES = ("textual", "talaria.ui")
_DOMAIN_ROOT = Path(talaria.domain.__file__).parent

_SWEEP = """
import importlib, json, sys

module_names, forbidden = json.loads(sys.argv[1])
for name in module_names:
    importlib.import_module(name)

violations = sorted(
    loaded
    for loaded in sys.modules
    if any(loaded == p or loaded.startswith(p + ".") for p in forbidden)
)
print(json.dumps(violations))
"""


def _domain_module_names() -> list[str]:
    """Every importable module under ``talaria/domain``, found on disk.

    Derived from the filesystem rather than ``pkgutil`` so that a module in a
    directory lacking ``__init__.py`` cannot hide from the sweep.
    """
    names: list[str] = []
    for path in sorted(_DOMAIN_ROOT.rglob("*.py")):
        parts = list(path.relative_to(_DOMAIN_ROOT).with_suffix("").parts)
        if parts and parts[-1] == "__init__":
            parts.pop()
        names.append(".".join([talaria.domain.__name__, *parts]))
    return names


def test_every_domain_directory_is_an_importable_package() -> None:
    """A directory without ``__init__.py`` is a hole in the sweep above."""
    missing = [
        str(directory.relative_to(_DOMAIN_ROOT.parent))
        for directory in sorted(_DOMAIN_ROOT.rglob("*"))
        if directory.is_dir()
        and directory.name != "__pycache__"
        and not (directory / "__init__.py").is_file()
    ]
    assert not missing, (
        "every directory under talaria/domain must contain __init__.py so the "
        f"ADR-0002 sweep can see its modules; missing in: {missing}"
    )


def test_domain_package_does_not_import_presentation_framework() -> None:
    module_names = _domain_module_names()
    assert module_names, "the ADR-0002 sweep found no domain modules to import"

    payload = json.dumps([module_names, list(_FORBIDDEN_PREFIXES)])
    # Fixed argv, no shell, test-only: the payload is this module's own data.
    completed = subprocess.run(
        [sys.executable, "-c", _SWEEP, payload],
        capture_output=True,
        text=True,
        check=False,
    )
    assert completed.returncode == 0, (
        "importing the domain modules failed outright:\n"
        f"{completed.stdout}\n{completed.stderr}"
    )

    violations = json.loads(completed.stdout)
    assert not violations, (
        "talaria.domain (or a module it imported) pulled in a presentation "
        f"framework module, violating ADR-0002: {violations}"
    )
