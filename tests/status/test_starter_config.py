"""C4 (#143): the shipped starter configuration parses and renders honestly.

The starter TOML lives in ``docs/configuration.md`` ("Starter status
configuration"); the starter script lives at
``docs/examples/status_bar_starter.py``. These tests pin the three
acceptance behaviors: the documented command parses to a direct-exec
argv, the shipped script renders real payload fields while labelling
the three real absences unavailable, and the documented field table
agrees with the frozen serializer.
"""

from __future__ import annotations

import asyncio
import re
import subprocess
import sys
import tomllib
from pathlib import Path

from talaria.domain.projection import StatusPayload
from talaria.status.contract import FROZEN_TOP_LEVEL_FIELDS, parse_command
from talaria.status.runner import StatusRunner

REPOSITORY = Path(__file__).resolve().parents[2]
GUIDE = REPOSITORY / "docs" / "configuration.md"
STARTER_SCRIPT = REPOSITORY / "docs" / "examples" / "status_bar_starter.py"
STARTER_HEADING = "## Starter status configuration"


def _starter_section() -> str:
    text = GUIDE.read_text(encoding="utf-8")
    start = text.index(STARTER_HEADING)
    following = text.find("\n## ", start + len(STARTER_HEADING))
    return text[start:] if following == -1 else text[start:following]


def _starter_toml() -> dict[str, object]:
    section = _starter_section()
    fences = re.findall(r"(?ms)^```toml\n(.*?)^```[ \t]*$", section)
    assert len(fences) == 1, "starter section must carry exactly its TOML"
    parsed: dict[str, object] = tomllib.loads(fences[0])
    return parsed


def test_starter_command_parses_to_a_direct_exec_argv() -> None:
    """The documented command is a python3 + absolute-path argv, no shell."""
    document = _starter_toml()
    assert set(document) == {"status"}
    status = document["status"]
    assert isinstance(status, dict)
    argv, notice = parse_command(status.get("command"))
    assert notice is None
    assert argv is not None
    assert argv[0] == "python3"
    assert argv[1].endswith("status_bar_starter.py")
    assert Path(argv[1]).is_absolute()


def test_shipped_starter_renders_payload_and_labels_absences() -> None:
    """The real script file renders through the runner: real fields shown,
    the three real gaps labelled unavailable, never fabricated."""
    assert STARTER_SCRIPT.is_file()

    async def scenario() -> None:
        observed = StatusPayload(
            version=1,
            mode="live",
            connection="connected",
            session_id="sess-9",
            session_title="starter check",
            turn="idle",
            pending_prompts=0,
            subagents_active=1,
            subagents_terminal=2,
            input_tokens=4321,
            output_tokens=876,
        )
        runner = StatusRunner(
            argv=(sys.executable, str(STARTER_SCRIPT)),
            launch_cwd=Path.cwd(),
        )
        result = await runner.tick(observed)
        await runner.aclose()
        assert result.outcome == "ok"
        rows = result.rows
        text = "\n".join(rows)
        assert "sess-9" in text
        assert "4321" in text and "876" in text
        for absent in ("context window", "rate limits", "spending"):
            assert absent in text
        assert text.count("unavailable") == 3

    asyncio.run(scenario())


def test_shipped_starter_labels_unobserved_usage_without_numbers() -> None:
    """Null usage renders as not-observed, not as zero."""

    async def scenario() -> None:
        unobserved = StatusPayload(
            version=1,
            mode="replay",
            connection="connected",
            session_id="sess-1",
            session_title=None,
            turn="idle",
            pending_prompts=0,
            subagents_active=0,
            subagents_terminal=0,
            input_tokens=None,
            output_tokens=None,
        )
        runner = StatusRunner(
            argv=(sys.executable, str(STARTER_SCRIPT)),
            launch_cwd=Path.cwd(),
        )
        result = await runner.tick(unobserved)
        await runner.aclose()
        assert result.outcome == "ok"
        text = "\n".join(result.rows)
        assert "not observed" in text
        assert "untitled" in text

    asyncio.run(scenario())


def test_starter_survives_empty_stdin_with_exit_zero() -> None:
    """A starter must never break the bar it demonstrates, even unreadable."""
    completed = subprocess.run(
        [sys.executable, str(STARTER_SCRIPT)],
        input=b"",
        capture_output=True,
        timeout=30,
    )
    assert completed.returncode == 0
    assert len(completed.stdout.splitlines()) == 6


def test_documented_field_table_matches_the_frozen_serializer() -> None:
    """Every frozen top-level field is named in the input-fields section,
    and each real absence is labelled unavailable there."""
    text = GUIDE.read_text(encoding="utf-8")
    start = text.index("## Status script input fields")
    following = text.find("\n## ", start + 1)
    section = text[start:] if following == -1 else text[start:following]
    for field in sorted(FROZEN_TOP_LEVEL_FIELDS):
        assert f"`{field}`" in section, field
    for gap in ("context window", "rate limits", "spending"):
        assert gap in section
    assert section.count("unavailable") >= 3


def test_apply_restart_rule_is_stated_for_both_cases() -> None:
    """Item 6 (status part): script edits live next tick, config needs a
    restart, and the section promises nothing beyond status behavior."""
    text = GUIDE.read_text(encoding="utf-8")
    start = text.index("## When status changes apply")
    following = text.find("\n## ", start + 1)
    section = text[start:] if following == -1 else text[start:following]
    assert "takes effect on the next tick" in section
    assert "needs a restart" in section
    assert "promises nothing beyond status behavior" in section
