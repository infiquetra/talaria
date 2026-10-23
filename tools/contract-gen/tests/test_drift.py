"""Test 18. Catalogue diff and the temporary workspace copy."""

from __future__ import annotations

import unittest
from pathlib import Path

import contract_gen

from . import support


def _method(name: str, params: str, result: str) -> dict:
    return {
        "name": name,
        "params": [
            {"name": "params", "schema": {"$ref": "#/components/schemas/" + params}}
        ],
        "result": {"schema": {"$ref": "#/components/schemas/" + result}},
    }


def _event(name: str, schema: str) -> dict:
    return {
        "name": name,
        "params": [
            {"name": "params", "schema": {"$ref": "#/components/schemas/" + schema}}
        ],
    }


def _request(name: str, params: str, result: str) -> dict:
    return {
        "name": name,
        "params": [
            {"name": "params", "schema": {"$ref": "#/components/schemas/" + params}}
        ],
        "result": {"schema": {"$ref": "#/components/schemas/" + result}},
    }


def _object(properties: dict) -> dict:
    return {"type": "object", "properties": properties}


class DriftTests(unittest.TestCase):
    def test_diff_lists_added_removed_and_changed_names(self) -> None:
        shared = {
            "Keep": _object({"n": {"type": "string"}}),
            "Note": _object({"n": {"type": "string"}}),
            "Request": _object({"n": {"type": "string"}}),
            "Extra": _object({"n": {"type": "string"}}),
        }
        before = {
            "methods": [
                _method("a.keep", "Keep", "Keep"),
                _method("a.gone", "Keep", "Keep"),
                _method("a.changed", "Changed", "Changed"),
            ],
            "x-canonical-methods": [_method("c.keep", "Keep", "Keep")],
            "x-notifications": [_event("e.keep", "Note"), _event("e.gone", "Note")],
            "x-server-requests": [_request("s.keep", "Request", "Request")],
            "components": {
                "schemas": dict(
                    shared,
                    Changed=_object({"n": {"type": "string"}}),
                    Removed=_object({"n": {"type": "string"}}),
                )
            },
        }
        after = {
            "methods": [
                _method("a.keep", "Keep", "Keep"),
                _method("a.new", "Keep", "Keep"),
                _method("a.changed", "Changed", "Changed"),
            ],
            "x-canonical-methods": [_method("c.keep", "Keep", "Extra")],
            "x-notifications": [_event("e.keep", "Note"), _event("e.new", "Note")],
            "x-server-requests": [_request("s.keep", "Request", "Keep")],
            "components": {
                "schemas": dict(
                    shared,
                    Changed=_object(
                        {"n": {"type": "string"}, "extra": {"type": "string"}}
                    ),
                    Added=_object({"n": {"type": "string"}}),
                )
            },
        }
        diff = contract_gen.catalogue_diff(
            before,
            after,
            ["own.a", "own.keep"],
            ["own.b", "own.keep"],
        )
        self.assertEqual(diff["legacy"]["added"], ["a.new"])
        self.assertEqual(diff["legacy"]["removed"], ["a.gone"])
        self.assertEqual(diff["legacy"]["changed"], ["a.changed"])
        self.assertEqual(diff["canonical"]["added"], [])
        self.assertEqual(diff["canonical"]["removed"], [])
        self.assertEqual(diff["canonical"]["changed"], ["c.keep"])
        self.assertEqual(diff["notifications"]["added"], ["e.new"])
        self.assertEqual(diff["notifications"]["removed"], ["e.gone"])
        self.assertEqual(diff["notifications"]["changed"], [])
        self.assertEqual(diff["server_requests"]["added"], [])
        self.assertEqual(diff["server_requests"]["removed"], [])
        self.assertEqual(diff["server_requests"]["changed"], ["s.keep"])
        self.assertEqual(diff["schemas"]["added"], ["Added"])
        self.assertEqual(diff["schemas"]["removed"], ["Removed"])
        self.assertEqual(diff["owner_methods"]["added"], ["own.b"])
        self.assertEqual(diff["owner_methods"]["removed"], ["own.a"])

    def test_workspace_copy_has_root_manifest_lock_and_toolchain(self) -> None:
        # The real checkout has no toolchain file yet, so the source tree is explicit.
        with support.temporary_directory() as temp_name:
            source = Path(temp_name) / "source"
            crate = source / "crates" / "hermes-contract"
            crate.mkdir(parents=True)
            (crate / "Cargo.toml").write_text(
                '[package]\nname = "hermes-contract"\n',
                encoding="utf-8",
            )
            (source / "crates" / "other").mkdir()
            (source / "Cargo.toml").write_text(
                "\n".join(
                    [
                        "[workspace]",
                        'members = ["crates/hermes-contract", "crates/other"]',
                        "",
                    ]
                ),
                encoding="utf-8",
            )
            (source / "Cargo.lock").write_text("# lock\n", encoding="utf-8")
            (source / "rust-toolchain.toml").write_text(
                '[toolchain]\nchannel = "1.98.1"\n',
                encoding="utf-8",
            )
            dest = Path(temp_name) / "dest"
            contract_gen.copy_workspace_for_drift(source, dest)
            manifest = (dest / "Cargo.toml").read_text(encoding="utf-8")
            self.assertIn("crates/hermes-contract", manifest)
            self.assertNotIn("crates/other", manifest)
            self.assertEqual((dest / "Cargo.lock").read_text(encoding="utf-8"), "# lock\n")
            self.assertTrue((dest / "rust-toolchain.toml").is_file())
            self.assertTrue((dest / "crates" / "hermes-contract" / "Cargo.toml").is_file())
            self.assertFalse((dest / "crates" / "other").exists())


if __name__ == "__main__":
    unittest.main()
