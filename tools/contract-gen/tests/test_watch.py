"""Tests for the scheduled Hermes contract drift report."""

from __future__ import annotations

import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]


class ScheduledDriftReportTest(unittest.TestCase):
    def test_a_newer_hermes_release_never_turns_the_report_red(self) -> None:
        text = (REPO / ".github" / "workflows" / "contract.yml").read_text(encoding="utf-8")
        start = text.index("      - name: Report drift")
        step = text[start:]
        self.assertIn("continue-on-error: true", step)
        self.assertIn("contract_gen.py drift --tag latest", step)


if __name__ == "__main__":
    unittest.main()
