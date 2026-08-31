"""A guard against the subclassing trap that cost this unit two debugging cycles.

Textual's ``App`` inherits a large private surface from ``MessagePump`` and
``DOMNode``. Two of this unit's own names collided with it, and neither failure
looked anything like its cause:

* ``_flush`` collided with ``App._flush`` (which flushes captured stdout).
  Overriding it with a render callback silently replaced framework behaviour.
* ``_closing`` collided with ``MessagePump._closing``. Setting it during our own
  teardown told the framework its shutdown was already underway, and the app
  then never finished closing — **every** Pilot test hung at the end of the
  ``run_test`` block, with a traceback that pointed only at the event loop and
  named nothing in this repository.

Neither is caught by mypy, ruff, or any behavioural test, because both are
perfectly legal Python: one is an override, the other an attribute assignment.
So the check is structural. Class attributes are compared directly; instance
attributes are read out of the source with :mod:`ast`, because ``_closing`` is
assigned in ``MessagePump.__init__`` and never declared at class level — which
is exactly why a class-dictionary comparison alone would have missed it.
"""

from __future__ import annotations

import ast
from pathlib import Path

from textual.app import App

import talaria.ui.app as app_module

#: Names this class overrides on purpose. Every entry is a documented Textual
#: extension point. Adding to this set is a deliberate decision, which is the
#: whole point: the alternative is stumbling into one.
DELIBERATE_OVERRIDES: frozenset[str] = frozenset(
    {
        "BINDINGS",
        "CSS",
        "TITLE",
        "compose",
        "on_mount",
        "on_unmount",
        "on_key",
        # Textual's documented Reactive theme selector. Issue #104 assigns it
        # deliberately after registering Talaria's four stable theme names.
        "theme",
        "__init__",
        "__module__",
        "__qualname__",
        "__doc__",
        "__annotations__",
        "__dict__",
        "__weakref__",
        "__parameters__",
        "__orig_bases__",
    }
)


def _textual_names() -> set[str]:
    names: set[str] = set()
    for klass in App.__mro__:
        names |= set(vars(klass))
    # Instance attributes the framework assigns in its own constructors.
    names |= set(vars(App()))
    return names


def _self_assignments() -> set[str]:
    """Every ``self.<name> = ...`` inside :class:`TalariaApp`, read from source."""
    tree = ast.parse(Path(app_module.__file__).read_text(encoding="utf-8"))
    assigned: set[str] = set()
    for node in ast.walk(tree):
        if not isinstance(node, ast.ClassDef) or node.name != "TalariaApp":
            continue
        for inner in ast.walk(node):
            targets: list[ast.expr] = []
            if isinstance(inner, ast.Assign):
                targets = list(inner.targets)
            elif isinstance(inner, ast.AnnAssign):
                targets = [inner.target]
            for target in targets:
                if (
                    isinstance(target, ast.Attribute)
                    and isinstance(target.value, ast.Name)
                    and target.value.id == "self"
                ):
                    assigned.add(target.attr)
    return assigned


def test_the_source_scan_finds_the_attributes_it_is_supposed_to_guard() -> None:
    """Guard the guard: a scan that silently found nothing would always pass."""
    assigned = _self_assignments()
    assert {"state", "render_ticks", "_teardown_started", "_coalesce_timer"} <= assigned
    body = _class_body_names()
    assert {"compose", "ingest", "drain", "measurements", "BINDINGS"} <= body


def _class_body_names() -> set[str]:
    """Names the class *body* defines, read from source.

    ``vars(TalariaApp)`` is the wrong instrument: Textual's
    ``DOMNode.__init_subclass__`` injects ``_reactives``, ``_computes``,
    ``_merged_bindings`` and friends into every subclass, so the class
    dictionary is full of names the author never wrote.
    """
    tree = ast.parse(Path(app_module.__file__).read_text(encoding="utf-8"))
    names: set[str] = set()
    for node in ast.walk(tree):
        if not isinstance(node, ast.ClassDef) or node.name != "TalariaApp":
            continue
        for statement in node.body:
            if isinstance(statement, ast.FunctionDef | ast.AsyncFunctionDef):
                names.add(statement.name)
            elif isinstance(statement, ast.AnnAssign) and isinstance(
                statement.target, ast.Name
            ):
                names.add(statement.target.id)
            elif isinstance(statement, ast.Assign):
                names.update(
                    target.id for target in statement.targets if isinstance(target, ast.Name)
                )
    return names


def test_no_class_attribute_shadows_textual() -> None:
    collisions = (_class_body_names() & _textual_names()) - DELIBERATE_OVERRIDES
    assert not collisions, (
        f"TalariaApp defines {sorted(collisions)}, which Textual already owns. "
        "Rename them, or add them to DELIBERATE_OVERRIDES with a reason."
    )


def test_no_assigned_attribute_shadows_textual() -> None:
    collisions = (_self_assignments() & _textual_names()) - DELIBERATE_OVERRIDES
    assert not collisions, (
        f"TalariaApp assigns {sorted(collisions)} on self, shadowing Textual's own "
        "state. Rename them — this is the failure mode that hangs every Pilot test."
    )
