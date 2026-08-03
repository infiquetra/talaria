"""ADR-0002 boundary check.

Imports every module under ``talaria.domain`` **in a fresh subprocess** and
fails the domain test run if anything outside the domain's allowed import set
landed in that subprocess's ``sys.modules`` afterward. This lives in the domain
package's own test run, not a separate lint step, so it fails the same way a
domain unit test would.

Three properties this check has to hold, each of which an earlier version got
wrong:

*Complete.* The module list comes from a filesystem walk, not
``pkgutil.walk_packages``. ``walk_packages`` cannot see into a directory that
has no ``__init__.py``, so a domain module could be fully importable at
runtime and completely invisible to the sweep. A companion test asserts every
directory under ``talaria/domain`` carries an ``__init__.py``, which keeps the
package a package rather than relying on namespace-package behavior.

*Attributable.* The sweep runs in a subprocess, so what it observes is caused
by ``talaria.domain`` alone. Reading the pytest process's own ``sys.modules``
would fail whenever any other test imported a framework first — which becomes
routine once ``talaria/ui`` lands, since that package is the one place a
presentation framework is allowed.

*Durable.* The rule is an **allow-list**, not a list of banned frameworks. An
earlier version named ``textual`` literally, which made it a deny-list of one:
a domain module importing ``prompt_toolkit`` — the fallback U4 assessed and
recommended for exactly the case where Textual fails its gate — passed
cleanly. A deny-list goes stale the moment the project's framework choice
moves, and it goes stale silently, which is the worst way for a guard to fail.
So: the domain may import the standard library and its own package, and
nothing else. Widening either allowance below is a deliberate ADR-0002
decision, not a routine edit.

The "demonstrably red on a deliberate violation" scenario required by U1's
test-scenario list is verified by adding a scratch domain module that imports
a third-party package, running this test once, and discarding it — a committed
red-by-construction fixture would permanently fail this test.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import talaria.domain

#: The root package, allowed as an *exact* match only. Importing any
#: ``talaria.domain`` module necessarily executes ``talaria/__init__.py``, so
#: the root has to be permitted — but permitting it as a prefix would allow
#: every subpackage in the project and disarm the check below entirely.
_ALLOWED_INTERNAL_EXACT = ("talaria",)

#: Subtrees the domain core may import, prefix-matched. Everything else in the
#: project — ``talaria.ui``, a future ``talaria.tui``, ``talaria.transport`` —
#: is out of bounds, so renaming the presentation package cannot quietly
#: disarm this check. Widening this tuple is an ADR-0002 decision.
_ALLOWED_INTERNAL_TREES = ("talaria.domain",)

_DOMAIN_ROOT = Path(talaria.domain.__file__).parent

_SWEEP = """
import importlib, json, sys

module_names, allowed_exact, allowed_trees = json.loads(sys.argv[1])

# Snapshot first: whatever the interpreter loaded to bootstrap itself is not
# the domain package's doing, and site-packages .pth hooks vary by machine.
before = {name.split(".")[0] for name in sys.modules}

for name in module_names:
    importlib.import_module(name)

after = {name.split(".")[0] for name in sys.modules}
third_party = sorted(
    top
    for top in after - before
    if top not in sys.stdlib_module_names and top != "talaria"
)

internal = sorted(
    name
    for name in sys.modules
    if name.split(".")[0] == "talaria"
    and name not in allowed_exact
    and not any(name == t or name.startswith(t + ".") for t in allowed_trees)
)

print(json.dumps({"third_party": third_party, "internal": internal}))
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
    """A directory without ``__init__.py`` is a hole in the sweep below."""
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


def test_domain_package_imports_only_stdlib_and_its_own_package() -> None:
    module_names = _domain_module_names()
    assert module_names, "the ADR-0002 sweep found no domain modules to import"

    payload = json.dumps(
        [module_names, list(_ALLOWED_INTERNAL_EXACT), list(_ALLOWED_INTERNAL_TREES)]
    )
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

    observed = json.loads(completed.stdout)

    assert not observed["third_party"], (
        "talaria.domain (or a module it imported) pulled in a third-party "
        "package, violating ADR-0002. The domain core may import the standard "
        f"library and talaria only; found: {observed['third_party']}"
    )
    assert not observed["internal"], (
        "talaria.domain (or a module it imported) pulled in a talaria package "
        f"outside {list(_ALLOWED_INTERNAL_TREES)}, violating ADR-0002: "
        f"{observed['internal']}"
    )
