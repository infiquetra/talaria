from __future__ import annotations

import sys
from collections.abc import Sequence

import pytest

from talaria.domain.projection import StatusPayload


def python_argv(code: str, *extra_args: str) -> tuple[str, ...]:
    """Build a no-shell argv that runs ``code`` under this interpreter.

    Every failure-matrix scenario needs a small script; going through
    ``sys.executable -c`` keeps every scenario portable (no ``/bin/sh``
    dependency) and exercises the same "exec argv directly, no shell" path
    the runner promises (R18).
    """
    return (sys.executable, "-c", code, *extra_args)


@pytest.fixture
def sample_payload() -> StatusPayload:
    return StatusPayload(
        version=1,
        mode="replay",
        connection="connected",
        session_id="sess-1",
        session_title="sample session",
        turn="idle",
        pending_prompts=0,
        subagents_active=0,
        subagents_terminal=0,
        input_tokens=None,
        output_tokens=None,
    )


def assert_argv_type(argv: Sequence[str]) -> None:
    assert isinstance(argv, tuple)
