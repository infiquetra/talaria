"""The /config choice proof file: values read from a Hermes checkout."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

import contract_gen

DEFAULTS = '''
DEFAULT_CONFIG = {
    "display": {
        # Pick one: plain | fancy
        "style": "plain",
        "flag": None,  # None = auto; True = always; False = never
    },
}
'''


class ConfigChoicesTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="config-choices-")
        self.root = Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()

    def write(self, path, text):
        file = self.root / path
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(text)

    def entry(self, recipe):
        return contract_gen.config_choice_entry(self.root, recipe)

    def test_constant_extraction_reads_literal(self):
        self.write("m.py", 'X = {"b", "a"}\nY = ("one", "two")\nZ = frozenset({"q"})\n')
        self.assertEqual(self.entry({"key": "k", "kind": "constant", "file": "m.py", "name": "X"})["values"], ["a", "b"])
        self.assertEqual(self.entry({"key": "k", "kind": "constant", "file": "m.py", "name": "Y"})["values"], ["one", "two"])
        self.assertEqual(self.entry({"key": "k", "kind": "constant", "file": "m.py", "name": "Z"})["values"], ["q"])

    def test_constant_selects_keys_items_and_names(self):
        self.write("m.py", 'D: dict = {"a": {"x": f()}, "b": {}}\nC = {"g": ["one", "two"]}\n'
                   'L = [{"name": "n1", "label": "L"}, {"name": "n2"}]\n')
        self.assertEqual(self.entry({"key": "k", "kind": "constant", "file": "m.py", "name": "D", "select": "keys"})["values"], ["a", "b"])
        self.assertEqual(self.entry({"key": "k", "kind": "constant", "file": "m.py", "name": "C", "select": "g"})["values"], ["one", "two"])
        self.assertEqual(self.entry({"key": "k", "kind": "constant", "file": "m.py", "name": "L", "select": "names"})["values"], ["n1", "n2"])

    def test_missing_constant_fails(self):
        self.write("m.py", "X = 1\n")
        with self.assertRaises(contract_gen.ContractError):
            self.entry({"key": "k", "kind": "constant", "file": "m.py", "name": "Missing"})

    def test_comment_block_must_name_every_value(self):
        self.write("hermes_cli/config_defaults.py", DEFAULTS)
        entry = self.entry({"key": "display.style", "kind": "comment", "values": ["plain", "fancy"]})
        self.assertEqual(entry["values"], ["plain", "fancy"])
        self.assertEqual(entry["file"], "hermes_cli/config_defaults.py")
        flag = self.entry({"key": "display.flag", "kind": "comment", "values": ["true", "false"]})
        self.assertEqual(flag["values"], ["true", "false"])
        with self.assertRaises(contract_gen.ContractError):
            self.entry({"key": "display.style", "kind": "comment", "values": ["plain", "loud"]})

    def test_missing_anchor_fails(self):
        self.write("hermes_cli/config_defaults.py", DEFAULTS)
        with self.assertRaises(contract_gen.ContractError):
            self.entry({"key": "display.style", "kind": "comment", "anchor": "no such text", "values": ["plain"]})
        anchored = self.entry({"key": "display.style", "kind": "comment", "anchor": "Pick one", "values": ["fancy"]})
        self.assertEqual(anchored["values"], ["fancy"])

    def test_function_literals(self):
        self.write("g.py", 'def mode(raw):\n    if raw not in ("off", "natural"):\n        return "natural"\n    return raw\n')
        entry = self.entry({"key": "k", "kind": "function", "file": "g.py", "function": "mode", "values": ["off", "natural"]})
        self.assertEqual(entry["values"], ["off", "natural"])
        with self.assertRaises(contract_gen.ContractError):
            self.entry({"key": "k", "kind": "function", "file": "g.py", "function": "mode", "values": ["custom"]})

    def test_desktop_list(self):
        self.write(contract_gen.DESKTOP_CHOICES, "export const X = {\n  'a.b': ['one', 'two'],\n  'a.c': [\n    'x'\n  ]\n}\n")
        self.assertEqual(self.entry({"key": "a.b", "kind": "desktop"})["values"], ["one", "two"])
        self.assertEqual(self.entry({"key": "a.c", "kind": "desktop"})["values"], ["x"])

    def test_schema_select_extraction(self):
        self.write(contract_gen.SCHEMA_SOURCE, '_SCHEMA_OVERRIDES = {\n'
                   '    "d": _select("desc", "x", "y"),\n'
                   '    "e": _select("desc", "", *OTHER, clearable=True),\n'
                   '    "f": {"type": "string"},\n}\n')
        self.assertEqual(contract_gen.config_schema_selects(self.root), [
            {"key": "d", "options": ["x", "y"], "dynamic": False},
            {"key": "e", "options": [""], "dynamic": True},
        ])

    def test_recipes_are_one_per_key(self):
        keys = [recipe["key"] for recipe in contract_gen.CONFIG_CHOICE_SOURCES]
        self.assertEqual(len(keys), len(set(keys)))

    def test_committed_file_names_the_pin(self):
        data = json.loads(contract_gen.CONFIG_CHOICES_FILE.read_text())
        self.assertEqual(data["hermes"]["commit"], contract_gen.read_pin()[1])
        self.assertEqual([e["key"] for e in data["choices"]], [r["key"] for r in contract_gen.CONFIG_CHOICE_SOURCES])


if __name__ == "__main__":
    unittest.main()
