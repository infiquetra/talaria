"""Test 18. Catalogue diff and the temporary workspace copy."""

from __future__ import annotations

import unittest
from contextlib import ExitStack, redirect_stdout
from io import StringIO
import json
from pathlib import Path
import subprocess
from unittest.mock import patch

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


class DriftDiagnosticsTests(unittest.TestCase):
    def setUp(self) -> None:
        self.stack = ExitStack()
        self.addCleanup(self.stack.close)
        self.root = Path(self.stack.enter_context(support.temporary_directory()))
        self.contract = {
            "methods": [],
            "x-canonical-methods": [],
            "x-notifications": [],
            "x-server-requests": [],
            "components": {"schemas": {}},
        }
        self.raw = json.dumps(self.contract).encode("utf-8")
        contract_path = self.root / "contract.json"
        contract_path.write_bytes(self.raw)
        owner_path = self.root / "owners.txt"
        owner_path.write_text("session.list\n", encoding="utf-8")
        self.stack.enter_context(patch.object(contract_gen, "CONTRACT", contract_path))
        self.stack.enter_context(patch.object(contract_gen, "OWNER_FILE", owner_path))
        self.resolve = self.stack.enter_context(
            patch.object(contract_gen, "resolve_tag", return_value=("v0.22.4", "a" * 40))
        )
        self.stack.enter_context(patch.object(contract_gen, "gh_raw", return_value=self.raw))
        self.stack.enter_context(
            patch.object(contract_gen, "extract_owner_methods", return_value=["session.list"])
        )
        self.stack.enter_context(
            patch.object(contract_gen, "copy_workspace", side_effect=self.prepare_workspace)
        )
        self.generate = self.stack.enter_context(patch.object(contract_gen, "generate"))
        self.results = [(0, b"test result: ok\n")] * 4
        self.commands = []
        self.run = self.stack.enter_context(
            patch.object(contract_gen.subprocess, "run", side_effect=self.run_phase)
        )
        self.stdout = StringIO()
        self.stack.enter_context(redirect_stdout(self.stdout))

    def prepare_workspace(self, destination: Path) -> None:
        crate = destination / "crates" / "hermes-contract"
        (crate / "vendor").mkdir(parents=True)
        tests = crate / "tests"
        tests.mkdir()
        for name in ("hygiene", "canonical_events", "frames", "responses"):
            (tests / (name + ".rs")).write_text("", encoding="utf-8")

    def run_phase(self, args, *, cwd, stdout, stderr):
        self.assertTrue((Path(cwd) / "crates/hermes-contract/vendor/gateway-contract.openrpc.json").is_file())
        self.assertEqual(stdout, subprocess.PIPE)
        self.assertEqual(stderr, subprocess.STDOUT)
        code, output = self.results[len(self.commands)]
        self.commands.append(list(args))
        return subprocess.CompletedProcess(args, code, output)

    def report(self) -> str:
        output = self.stdout.getvalue()
        self.assertIn("# Hermes contract drift\n", output)
        return output[output.index("# Hermes contract drift\n"):]

    def assert_phase(self, label: str, code: int, command: list) -> None:
        lines = self.stdout.getvalue().splitlines()
        self.assertTrue(
            any(label in line and json.dumps(command) in line
                and "returncode={}".format(code) in line for line in lines),
            self.stdout.getvalue(),
        )

    def test_compiler_diagnostic_and_context_survive_build_failure(self) -> None:
        diagnostic = (
            "error[E0308]: mismatched types\n"
            " --> src/generated/schemas.rs:12:5\n"
            "12 | value\n   | ^^^^^ expected String\n"
        )
        self.results[0] = (17, diagnostic.encode())
        self.assertEqual(contract_gen.drift("latest"), 1)
        self.assertIn(diagnostic, self.stdout.getvalue())
        self.assert_phase("build", 17, ["cargo", "build", "-p", "hermes-contract"])
        self.assertIn("build (cargo build -p hermes-contract): failed", self.report())
        self.assertEqual(len(self.commands), 4)

    def test_binding_assertion_details_survive_failure(self) -> None:
        diagnostic = "thread 'native' panicked:\nassertion left == right\n left: 1\nright: 2\n"
        self.results[1] = (101, diagnostic.encode())
        self.assertEqual(contract_gen.drift("latest"), 1)
        self.assertIn(diagnostic, self.stdout.getvalue())
        self.assert_phase("binding tests", 101, self.commands[1])
        self.assertIn("binding tests (", self.report())

    def test_doc_only_failure_is_a_gate_and_keeps_diagnostics(self) -> None:
        diagnostic = "error[E0425]: cannot find value in this scope\n --> example.rs:4:1\n"
        self.results[2] = (101, diagnostic.encode())
        self.assertEqual(contract_gen.drift("latest"), 1)
        self.assertIn(diagnostic, self.stdout.getvalue())
        self.assert_phase("doc tests", 101, ["cargo", "test", "-p", "hermes-contract", "--doc"])
        self.assertEqual(len(self.commands), 4)

    def test_success_without_summary_prints_release_diff_and_every_phase(self) -> None:
        self.assertEqual(contract_gen.drift("latest"), 0)
        report = self.report()
        self.assertIn("Release: v0.22.4 ({})".format("a" * 40), report)
        diff = json.loads(report.split("```json\n", 1)[1].split("\n```", 1)[0])
        self.assertEqual(diff["legacy"], {"added": [], "changed": [], "removed": []})
        expected = [
            ["cargo", "build", "-p", "hermes-contract"],
            ["cargo", "test", "-p", "hermes-contract", "--lib", "--test", "canonical_events", "--test", "hygiene"],
            ["cargo", "test", "-p", "hermes-contract", "--doc"],
            ["cargo", "test", "-p", "hermes-contract", "--test", "frames", "--test", "responses"],
        ]
        self.assertEqual(self.commands, expected)
        for label, command in zip(("build", "binding tests", "doc tests", "recording tests"), expected):
            self.assert_phase(label, 0, command)
            self.assertIn("{} ({}): passed".format(label, " ".join(command)), report)

    def test_actual_summary_bytes_equal_the_report_printed_to_stdout(self) -> None:
        summary = self.root / "summary.md"
        self.assertEqual(contract_gen.drift("latest", summary, self.root / "hermes"), 0)
        self.assertEqual(summary.read_bytes(), self.report().encode("utf-8"))
        self.assertIn("Owner methods added: []; removed: []", self.report())

    def test_recording_only_failure_is_informational_and_visible(self) -> None:
        diagnostic = "recorded response differs\n expected: session.list\n observed: session.get"
        self.results[3] = (23, diagnostic.encode())
        self.assertEqual(contract_gen.drift("latest"), 0)
        self.assertIn(diagnostic, self.stdout.getvalue())
        self.assert_phase("recording tests", 23, self.commands[3])
        self.assertIn("recording tests (", self.report())
        self.assertIn(": failed", self.report())

    def test_generation_exception_keeps_failure_report_and_return_code(self) -> None:
        self.generate.side_effect = contract_gen.ContractError("generation failed\nschema detail")
        self.assertEqual(contract_gen.drift("latest"), 1)
        self.assertIn("failed: generation failed schema detail", self.report())
        self.run.assert_not_called()

    def test_summary_write_error_propagates_after_report_is_printed(self) -> None:
        with self.assertRaises(IsADirectoryError):
            contract_gen.drift("latest", self.root)
        self.assertIn("Release: v0.22.4", self.report())
        self.assertIn("recording tests (", self.report())

    def test_cli_summary_write_error_retains_failure_and_visible_report(self) -> None:
        code = contract_gen.main(["drift", "--tag", "latest", "--summary", str(self.root)])
        self.assertEqual(code, 1)
        output = self.stdout.getvalue()
        self.assertIn("# Hermes contract drift\n", output)
        self.assertIn("recording tests (", output)
        self.assertIn("Is a directory", output)
        self.assertNotIn("Traceback", output)


if __name__ == "__main__":
    unittest.main()
