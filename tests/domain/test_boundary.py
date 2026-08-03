"""ADR-0002 boundary check.

Imports every module under ``talaria.domain`` and fails the domain test run
if the presentation framework (``textual``) or ``talaria.ui`` has landed in
``sys.modules`` afterward. This lives in the domain package's own test run,
not a separate lint step, so it fails the same way a domain unit test would.

The "demonstrably red on a deliberate violation" scenario required by U1's
test-scenario list is verified manually (a scratch domain module that
imports textual, run once and discarded) rather than committed — a
red-by-construction fixture would either need to be excluded from this same
sweep (defeating the point) or would permanently fail this test.
"""

from __future__ import annotations

import importlib
import pkgutil
import sys

import talaria.domain

_FORBIDDEN_PREFIXES = ("textual", "talaria.ui")


def _domain_module_names() -> list[str]:
    names = [talaria.domain.__name__]
    for module_info in pkgutil.walk_packages(
        talaria.domain.__path__, prefix=f"{talaria.domain.__name__}."
    ):
        names.append(module_info.name)
    return names


def test_domain_package_does_not_import_presentation_framework() -> None:
    for module_name in _domain_module_names():
        importlib.import_module(module_name)

    def _matches(loaded: str, prefix: str) -> bool:
        return loaded == prefix or loaded.startswith(f"{prefix}.")

    violations = [
        loaded
        for loaded in sys.modules
        if any(_matches(loaded, prefix) for prefix in _FORBIDDEN_PREFIXES)
    ]
    assert not violations, (
        "talaria.domain (or a module it imported) pulled in a presentation "
        f"framework module, violating ADR-0002: {sorted(violations)}"
    )
