"""The four release targets, as text and as one JSON line."""

from __future__ import annotations

import json
import unittest

import package


_TEXT = (
    "aarch64-apple-darwin macos-14 native\n"
    "x86_64-apple-darwin macos-14 cross\n"
    "x86_64-unknown-linux-musl ubuntu-24.04 native\n"
    "aarch64-unknown-linux-musl ubuntu-24.04-arm native\n"
)

_ROWS = (
    {"target": "aarch64-apple-darwin", "runner": "macos-14", "kind": "native"},
    {"target": "x86_64-apple-darwin", "runner": "macos-14", "kind": "cross"},
    {"target": "x86_64-unknown-linux-musl", "runner": "ubuntu-24.04", "kind": "native"},
    {
        "target": "aarch64-unknown-linux-musl",
        "runner": "ubuntu-24.04-arm",
        "kind": "native",
    },
)


class TargetTests(unittest.TestCase):
    def test_target_table_is_exactly_the_four_targets(self):
        text = package.targets_text()
        self.assertEqual(text, _TEXT, "package.py missing")
        raw = package.targets_json()
        self.assertEqual(raw.strip().count("\n"), 0, "package.py missing")
        rows = json.loads(raw)
        self.assertEqual(len(rows), 4, "package.py missing")
        for row, expected in zip(rows, _ROWS):
            self.assertEqual(list(row.keys()), ["target", "runner", "kind"], "package.py missing")
            self.assertEqual(row, expected, "package.py missing")
