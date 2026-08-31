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

**What a green run here does and does not prove.** It proves the domain core
imports no third-party package and no sibling ``talaria`` package. It does
**not** prove I/O discipline: the whole standard library is allowed, so a
domain module may ``import socket``, ``subprocess`` or ``asyncio`` and stay
green. That is the correct scope for this check — banning ``pathlib`` would be
absurd — but ADR-0002's rationale is partly about keeping I/O out of the domain
(KTD3 puts the ``FrameSource`` seam in ``talaria/transport/`` because a live
implementation owns a socket). Since this is the only mechanical ADR-0002
signal in the repository, it is easy to read a green run as the whole boundary.
I/O discipline remains a design-review property, not something this test
enforces. It is also import-time only: a lazy ``import`` inside a function body
evades it.

The "demonstrably red on a deliberate violation" scenario required by U1's
test-scenario list is verified by adding a scratch domain module that imports
a third-party package, running this test once, and discarding it — a committed
red-by-construction fixture would permanently fail this test.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from importlib.machinery import all_suffixes
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

    Two details here are load bearing, and both were found by attacking an
    earlier version that used ``_DOMAIN_ROOT.rglob("*.py")``:

    *Follow symlinks.* ``Path.rglob`` does not descend into a symlinked
    directory, but the import system does. A symlinked subpackage under
    ``talaria/domain`` was therefore fully importable and completely invisible
    to the sweep — and worse, the companion ``__init__.py`` test *approved* it,
    because the directory it points at does contain one. Git stores symlinks,
    so that can land in a repository and survive review. ``os.walk`` with
    ``followlinks=True`` is used rather than ``Path.rglob(recurse_symlinks=...)``
    because that parameter does not exist before Python 3.13 and this project
    supports 3.12.

    *Match every import suffix, not ``.py``.* A sourceless ``.pyc`` or a
    compiled ``.so`` dropped into the package imports perfectly well and has no
    ``.py`` to find. ``importlib.machinery.all_suffixes()`` is the interpreter's
    own answer to "what can be imported", so it stays correct if that set
    changes.
    """
    names: list[str] = []
    suffixes = tuple(all_suffixes())
    for dirpath, dirnames, filenames in os.walk(_DOMAIN_ROOT, followlinks=True):
        dirnames[:] = [name for name in dirnames if name != "__pycache__"]
        for filename in filenames:
            if not filename.endswith(suffixes):
                continue
            path = Path(dirpath) / filename
            # Strip only the matched import suffix. Path.with_suffix() would
            # mangle a name like ``fswatcher.cpython-312-darwin.so``.
            matched = next(s for s in suffixes if filename.endswith(s))
            stem = filename[: -len(matched)]
            parts = list(path.parent.relative_to(_DOMAIN_ROOT).parts)
            if stem != "__init__":
                parts.append(stem)
            names.append(".".join([talaria.domain.__name__, *parts]))
    return sorted(set(names))


def test_every_domain_directory_is_an_importable_package() -> None:
    """A directory without ``__init__.py`` is a hole in the sweep below."""
    missing = []
    # followlinks, for the same reason _domain_module_names walks that way: a
    # symlinked subpackage is importable, so it is in scope for this rule too.
    for dirpath, dirnames, _ in os.walk(_DOMAIN_ROOT, followlinks=True):
        dirnames[:] = [name for name in dirnames if name != "__pycache__"]
        for name in dirnames:
            directory = Path(dirpath) / name
            if not (directory / "__init__.py").is_file():
                missing.append(str(directory.relative_to(_DOMAIN_ROOT.parent)))
    missing.sort()
    assert not missing, (
        "every directory under talaria/domain must contain __init__.py so the "
        f"ADR-0002 sweep can see its modules; missing in: {missing}"
    )


def test_domain_package_imports_only_stdlib_and_its_own_package() -> None:
    module_names = _domain_module_names()
    assert module_names, "the ADR-0002 sweep found no domain modules to import"
    assert "talaria.domain.changes" in module_names, (
        "the issue #107 inspector domain module fell outside the ADR-0002 import sweep"
    )

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
