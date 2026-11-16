"""Test 19. CLI exit codes, with fake gh and cargo kept off the network."""

from __future__ import annotations

import hashlib
import json
import subprocess
import tomllib
import unittest
from pathlib import Path

from . import support

GENERATED_PLACEHOLDER = "committed-placeholder\n"
OLD_PIN_TAG = "v0.22.4"
OLD_PIN_COMMIT = "4d04303818b61bf3aa09278af48356315515965b"
NEW_GH_COMMIT = "0123456789abcdef0123456789abcdef01234567"
PINNED_RS_BODY = (
    "pub const TAG: &str = crate::compat::CURRENT.pinned_tag;\n"
    "pub const COMMIT: &str = crate::compat::CURRENT.pinned_commit;\n"
)
README_BODY = (
    "<!-- hermes-compat:start -->\n"
    "Hermes compatibility\n"
    "<!-- hermes-compat:end -->\n\n"
    "## Install\n"
)
CASE_PHRASES = (
    "a title collision in the generator",
    "a removed event still named in crates/hermes-contract/src/events.rs",
    "a recorded frame or a mock that rejects a new field",
    "a new field that a hand-written struct literal must name",
)


def _pin_commit() -> str:
    text = support.PINNED_PATH.read_text(encoding="utf-8")
    for line in text.splitlines():
        if line.startswith("commit="):
            return line.split("=", 1)[1]
    raise AssertionError("PINNED has no commit")


def _prepare_workspace(root: Path, generated_text: str = "") -> Path:
    script = support.copy_generator(root)
    vendor = root / "crates" / "hermes-contract" / "vendor"
    vendor.mkdir(parents=True)
    for name in (
        "gateway-contract.openrpc.json",
        "gateway-contract.openrpc.json.sha256",
        "LICENSE.hermes",
    ):
        source = support.VENDOR / name
        (vendor / name).write_bytes(source.read_bytes())
    pinned = root / "crates" / "hermes-contract" / "PINNED"
    pinned.write_text(support.PINNED_PATH.read_text(encoding="utf-8"), encoding="utf-8")
    if generated_text:
        generated = root / "crates" / "hermes-contract" / "src" / "generated"
        generated.mkdir(parents=True)
        for name in support.GENERATED_FILES:
            (generated / name).write_text(generated_text, encoding="utf-8")
    owner = root / "tools" / "contract-gen" / "native-owner-methods.txt"
    owner.write_text("pet.info\n", encoding="utf-8")
    (root / "Cargo.toml").write_text(
        '[workspace]\nmembers = ["crates/hermes-contract"]\n',
        encoding="utf-8",
    )
    (root / "Cargo.lock").write_text("# lock\n", encoding="utf-8")
    (root / "rust-toolchain.toml").write_text(
        '[toolchain]\nchannel = "1.98.1"\n',
        encoding="utf-8",
    )
    return script


def _gh_env(bin_dir: Path, log: Path, contract: Path, licence: Path) -> dict:
    return {
        "PATH": support.prepend_path(bin_dir),
        "GH_LOG": str(log),
        "GH_CONTRACT": str(contract),
        "GH_LICENSE": str(licence),
        "GH_COMMIT": _pin_commit(),
        "GH_MISSING_TAG": "v0.0.0-missing",
    }


def _prepare_sync_workspace(root: Path, newest_tested: str) -> Path:
    script = _prepare_workspace(root)
    crate = root / "crates" / "hermes-contract"
    (crate / "PINNED").write_text(
        f"tag={OLD_PIN_TAG}\ncommit={OLD_PIN_COMMIT}\n",
        encoding="utf-8",
    )
    (crate / "src").mkdir(parents=True, exist_ok=True)
    (crate / "src" / "pinned.rs").write_text(PINNED_RS_BODY, encoding="utf-8")
    (crate / "src" / "generated").mkdir(parents=True, exist_ok=True)
    (crate / "compat.toml").write_text(
        "\n".join(
            [
                "[[release]]",
                'talaria = "0.8.0"',
                'floor = "v0.23.0"',
                f'pinned_tag = "{OLD_PIN_TAG}"',
                f'pinned_commit = "{OLD_PIN_COMMIT}"',
                f'newest_tested = "{newest_tested}"',
                "",
            ]
        ),
        encoding="utf-8",
    )
    (root / "README.md").write_text(README_BODY, encoding="utf-8")
    (root / "docs").mkdir(parents=True, exist_ok=True)
    return script


def _sync_env(root: Path, bin_dir: Path, contract: Path | None = None) -> dict:
    vendor = root / "crates" / "hermes-contract" / "vendor"
    env = _gh_env(
        bin_dir,
        root / "gh.log",
        contract if contract is not None else vendor / "gateway-contract.openrpc.json",
        vendor / "LICENSE.hermes",
    )
    env["GH_COMMIT"] = NEW_GH_COMMIT
    env["CARGO_LOG"] = str(root / "cargo.log")
    return env


def _git_add_fixtures(root: Path, *paths: str) -> None:
    subprocess.run(["git", "init"], cwd=root, check=True, capture_output=True)
    subprocess.run(["git", "add", *paths], cwd=root, check=True, capture_output=True)


class CliTests(unittest.TestCase):
    def test_unknown_tag_prints_one_plain_line_and_exits_1(self) -> None:
        with support.temporary_directory() as temp_name:
            root = Path(temp_name)
            script = _prepare_workspace(root)
            bin_dir = root / "bin"
            support.install_fakes(bin_dir, include_cargo=True)
            code, stdout, stderr = support.run_script(
                script,
                ["drift", "--tag", "v0.0.0-missing"],
                _gh_env(
                    bin_dir,
                    root / "gh.log",
                    support.CONTRACT_PATH,
                    support.VENDOR / "LICENSE.hermes",
                ),
            )
            combined = stdout + stderr
            self.assertNotIn("Traceback", combined)
            self.assertEqual(code, 1)
            self.assertIn(
                "tag v0.0.0-missing was not found in NousResearch/hermes-agent",
                combined,
            )

    def test_check_lists_each_differing_file_and_exits_1(self) -> None:
        with support.temporary_directory() as temp_name:
            root = Path(temp_name)
            script = _prepare_workspace(root, GENERATED_PLACEHOLDER)
            bin_dir = root / "bin"
            support.install_fakes(bin_dir, include_cargo=True)
            env = _gh_env(
                bin_dir,
                root / "gh.log",
                support.CONTRACT_PATH,
                support.VENDOR / "LICENSE.hermes",
            )
            env["CARGO_LOG"] = str(root / "cargo.log")
            code, stdout, stderr = support.run_script(
                script,
                ["check", "--require-typify"],
                env,
            )
            combined = stdout + stderr
            self.assertNotIn("Traceback", combined)
            self.assertEqual(code, 1, combined)
            for name in support.GENERATED_FILES:
                self.assertIn(name, combined)

    def test_verify_vendor_checks_the_contract_and_the_licence(self) -> None:
        with support.temporary_directory() as temp_name:
            root = Path(temp_name)
            script = _prepare_workspace(root)
            bin_dir = root / "bin"
            support.install_fakes(bin_dir, include_cargo=False)
            contract = support.CONTRACT_PATH.read_bytes()
            licence = (support.VENDOR / "LICENSE.hermes").read_bytes()
            mutated_contract = bytearray(contract)
            mutated_contract[-1] ^= 0x01
            mutated_licence = bytearray(licence)
            mutated_licence[-1] ^= 0x01
            contract_copy = root / "served-contract.json"
            licence_copy = root / "served-licence"
            bad_contract = root / "bad-contract.json"
            bad_licence = root / "bad-licence"
            contract_copy.write_bytes(contract)
            licence_copy.write_bytes(licence)
            bad_contract.write_bytes(bytes(mutated_contract))
            bad_licence.write_bytes(bytes(mutated_licence))

            match_log = root / "match.log"
            code, stdout, stderr = support.run_script(
                script,
                ["verify-vendor"],
                _gh_env(bin_dir, match_log, contract_copy, licence_copy),
            )
            combined = stdout + stderr
            self.assertNotIn("Traceback", combined)
            self.assertEqual(code, 0, combined)
            log = match_log.read_text(encoding="utf-8")
            self.assertIn("application/vnd.github.raw+json", log)
            self.assertIn("repos/NousResearch/hermes-agent/contents/", log)
            self.assertIn(_pin_commit(), log)

            code, stdout, stderr = support.run_script(
                script,
                ["verify-vendor"],
                _gh_env(bin_dir, root / "bad-contract.log", bad_contract, licence_copy),
            )
            combined = stdout + stderr
            self.assertNotIn("Traceback", combined)
            self.assertEqual(code, 1, combined)
            self.assertIn(hashlib.sha256(contract).hexdigest(), combined)
            self.assertIn(hashlib.sha256(bytes(mutated_contract)).hexdigest(), combined)

            code, stdout, stderr = support.run_script(
                script,
                ["verify-vendor"],
                _gh_env(bin_dir, root / "bad-licence.log", contract_copy, bad_licence),
            )
            combined = stdout + stderr
            self.assertNotIn("Traceback", combined)
            self.assertEqual(code, 1, combined)
            self.assertIn(hashlib.sha256(licence).hexdigest(), combined)
            self.assertIn(hashlib.sha256(bytes(mutated_licence)).hexdigest(), combined)

    def test_drift_exit_status_follows_binding_tests_only(self) -> None:
        self._drift_case("frames", 0)
        self._drift_case("unions", 1)

    def _drift_case(self, failing_test: str, expected: int) -> None:
        with support.temporary_directory() as temp_name:
            root = Path(temp_name)
            script = _prepare_workspace(root, GENERATED_PLACEHOLDER)
            bin_dir = root / "bin"
            support.install_fakes(bin_dir, include_cargo=True)
            summary = root / "summary.md"
            env = _gh_env(
                bin_dir,
                root / "gh.log",
                support.CONTRACT_PATH,
                support.VENDOR / "LICENSE.hermes",
            )
            env["CARGO_LOG"] = str(root / "cargo.log")
            env["FAKE_CARGO_FAIL"] = failing_test
            code, stdout, stderr = support.run_script(
                script,
                ["drift", "--tag", "v0.22.4", "--summary", str(summary)],
                env,
            )
            combined = stdout + stderr
            self.assertNotIn("Traceback", combined)
            self.assertEqual(code, expected, combined)
            text = summary.read_text(encoding="utf-8")
            self.assertIn(failing_test, text)

    def test_bump_writes_the_pin_and_raises_newest_tested(self) -> None:
        with support.temporary_directory() as temp_name:
            root = Path(temp_name)
            script = _prepare_sync_workspace(root, "v0.22.4")
            bin_dir = root / "bin"
            support.install_fakes(bin_dir, include_cargo=True)

            matrix = root / "fixtures" / "configuration-surface" / "matrix.json"
            results = root / "fixtures" / "gateway-concurrency" / "results.json"
            matrix.parent.mkdir(parents=True, exist_ok=True)
            results.parent.mkdir(parents=True, exist_ok=True)
            matrix.write_text('{"recorded_against":"v0.22.4"}\n', encoding="utf-8")
            results.write_text('{"recorded_against":"v0.23.0"}\n', encoding="utf-8")
            _git_add_fixtures(
                root,
                "fixtures/configuration-surface/matrix.json",
                "fixtures/gateway-concurrency/results.json",
            )

            pinned_rs = root / "crates" / "hermes-contract" / "src" / "pinned.rs"
            pinned_rs_before = pinned_rs.read_bytes()

            code, stdout, stderr = support.run_script(
                script,
                ["bump", "--tag", "v0.23.0"],
                _sync_env(root, bin_dir),
            )
            combined = stdout + stderr
            self.assertNotIn("Traceback", combined)
            self.assertEqual(code, 0, combined)

            crate = root / "crates" / "hermes-contract"
            self.assertEqual(
                (crate / "PINNED").read_text(encoding="utf-8"),
                f"tag=v0.23.0\ncommit={NEW_GH_COMMIT}\n",
            )
            self.assertEqual(pinned_rs.read_bytes(), pinned_rs_before)

            vendor = crate / "vendor"
            contract_bytes = (vendor / "gateway-contract.openrpc.json").read_bytes()
            expected_sha = (
                hashlib.sha256(contract_bytes).hexdigest()
                + "  gateway-contract.openrpc.json\n"
            )
            self.assertEqual(
                (vendor / "gateway-contract.openrpc.json.sha256").read_text(
                    encoding="utf-8"
                ),
                expected_sha,
            )

            row = tomllib.loads(
                (crate / "compat.toml").read_text(encoding="utf-8")
            )["release"][-1]
            self.assertEqual(row["pinned_tag"], "v0.23.0")
            self.assertEqual(row["newest_tested"], "v0.23.0")
            self.assertEqual(row["floor"], "v0.23.0")

            compat_doc = root / "docs" / "compatibility.md"
            compat_rs = crate / "src" / "generated" / "compat.rs"
            self.assertIn("v0.23.0", compat_doc.read_text(encoding="utf-8"))
            compat_rs_text = compat_rs.read_text(encoding="utf-8")
            self.assertIn("v0.23.0", compat_rs_text)
            self.assertIn("pub const CURRENT", compat_rs_text)

            self.assertIn("fixtures/configuration-surface/matrix.json", stdout)
            self.assertNotIn("fixtures/gateway-concurrency/results.json", stdout)

            gh_lines = [
                line
                for line in (root / "gh.log").read_text(encoding="utf-8").splitlines()
                if "contents/" in line
            ]
            self.assertEqual(len(gh_lines), 2, gh_lines)
            self.assertTrue(
                any("apps/shared/src/gateway-contract.openrpc.json" in line for line in gh_lines)
            )
            self.assertTrue(any("LICENSE" in line for line in gh_lines))

            cargo_lines = (root / "cargo.log").read_text(encoding="utf-8").splitlines()
            check_lines = [line for line in cargo_lines if line.startswith("check")]
            self.assertEqual(check_lines, ["check --workspace --all-targets"])
            self.assertFalse(any("--hermes-checkout" in line for line in cargo_lines))

    def test_bump_does_not_lower_newest_tested(self) -> None:
        with support.temporary_directory() as temp_name:
            root = Path(temp_name)
            script = _prepare_sync_workspace(root, "v0.23.1")
            bin_dir = root / "bin"
            support.install_fakes(bin_dir, include_cargo=True)

            code, stdout, stderr = support.run_script(
                script,
                ["bump", "--tag", "v0.23.0"],
                _sync_env(root, bin_dir),
            )
            combined = stdout + stderr
            self.assertNotIn("Traceback", combined)
            self.assertEqual(code, 0, combined)

            row = tomllib.loads(
                (root / "crates" / "hermes-contract" / "compat.toml").read_text(
                    encoding="utf-8"
                )
            )["release"][-1]
            self.assertEqual(row["pinned_tag"], "v0.23.0")
            self.assertEqual(row["newest_tested"], "v0.23.1")

    def test_bump_title_collision_leaves_the_tree_and_names_the_runbook(self) -> None:
        with support.temporary_directory() as temp_name:
            root = Path(temp_name)
            script = _prepare_sync_workspace(root, "v0.22.4")
            bin_dir = root / "bin"
            support.install_fakes(bin_dir, include_cargo=True)

            crate = root / "crates" / "hermes-contract"
            vendor = crate / "vendor"
            contract = json.loads(
                (vendor / "gateway-contract.openrpc.json").read_text(encoding="utf-8")
            )
            contract["components"]["schemas"]["CollisionProbe"] = {
                "type": "object",
                "properties": {
                    "first": {
                        "type": "string",
                        "title": "CollisionProbeTitle",
                        "enum": ["one"],
                    },
                    "second": {
                        "type": "string",
                        "title": "CollisionProbeTitle",
                        "enum": ["two"],
                    },
                },
            }
            served_contract = root / "collision-contract.json"
            served_contract.write_text(json.dumps(contract), encoding="utf-8")

            recorded_files = [
                crate / "PINNED",
                crate / "src" / "pinned.rs",
                crate / "compat.toml",
                root / "README.md",
                script,
                *sorted(path for path in vendor.rglob("*") if path.is_file()),
            ]
            recorded_bytes = {path: path.read_bytes() for path in recorded_files}

            code, stdout, stderr = support.run_script(
                script,
                ["bump", "--tag", "v0.23.0"],
                _sync_env(root, bin_dir, contract=served_contract),
            )
            combined = stdout + stderr
            self.assertNotIn("Traceback", combined)
            self.assertEqual(code, 1, combined)
            self.assertIn("a title collision in the generator", combined)
            self.assertIn(
                "inline string enum title collision: CollisionProbeTitle",
                combined,
            )

            for path, expected in recorded_bytes.items():
                self.assertEqual(path.read_bytes(), expected, f"{path} changed")
            self.assertFalse((root / "docs" / "compatibility.md").exists())
            self.assertFalse(
                (crate / "src" / "generated" / "compat.rs").exists()
            )

    def test_bump_without_gh_leaves_the_pin_unchanged(self) -> None:
        with support.temporary_directory() as temp_name:
            root = Path(temp_name)
            script = _prepare_sync_workspace(root, "v0.22.4")
            bin_dir = root / "bin"
            cargo_script = support.fake_cargo_script()
            support.write_executable(bin_dir / "cargo", cargo_script)
            support.write_executable(bin_dir / "cargo-typify", cargo_script)

            pinned = root / "crates" / "hermes-contract" / "PINNED"
            pinned_before = pinned.read_bytes()

            code, stdout, stderr = support.run_script(
                script,
                ["bump", "--tag", "v0.23.0"],
                {"PATH": str(bin_dir)},
            )
            combined = stdout + stderr
            self.assertNotIn("Traceback", combined)
            self.assertEqual(code, 1, combined)
            self.assertIn("gh is not installed", combined)
            for phrase in CASE_PHRASES:
                self.assertNotIn(phrase, combined)
            self.assertEqual(pinned.read_bytes(), pinned_before)

    def test_tested_records_a_newer_release_without_moving_the_pin(self) -> None:
        with support.temporary_directory() as temp_name:
            root = Path(temp_name)
            script = _prepare_sync_workspace(root, "v0.22.4")
            bin_dir = root / "bin"
            support.install_fakes(bin_dir, include_cargo=True)

            crate = root / "crates" / "hermes-contract"
            pinned_before = (crate / "PINNED").read_bytes()
            gh_log = root / "gh.log"

            code, stdout, stderr = support.run_script(
                script,
                ["tested", "--tag", "v0.23.0"],
                _sync_env(root, bin_dir),
            )
            combined = stdout + stderr
            self.assertNotIn("Traceback", combined)
            self.assertEqual(code, 0, combined)

            row = tomllib.loads(
                (crate / "compat.toml").read_text(encoding="utf-8")
            )["release"][-1]
            self.assertEqual(row["newest_tested"], "v0.23.0")
            self.assertEqual(row["pinned_tag"], OLD_PIN_TAG)
            self.assertEqual((crate / "PINNED").read_bytes(), pinned_before)
            self.assertEqual(
                gh_log.read_text(encoding="utf-8") if gh_log.exists() else "",
                "",
            )
            self.assertIn(
                "v0.23.0",
                (root / "docs" / "compatibility.md").read_text(encoding="utf-8"),
            )
            self.assertIn(
                "v0.23.0",
                (root / "README.md").read_text(encoding="utf-8"),
            )
            self.assertNotIn("recorded_against", stdout)

    def test_tested_refuses_a_tag_that_is_not_newer(self) -> None:
        with support.temporary_directory() as temp_name:
            root = Path(temp_name)
            script = _prepare_sync_workspace(root, "v0.23.0")
            bin_dir = root / "bin"
            support.install_fakes(bin_dir, include_cargo=True)

            compat_toml = root / "crates" / "hermes-contract" / "compat.toml"
            compat_before = compat_toml.read_bytes()

            for tag in ("v0.22.4", "v0.23.0"):
                code, stdout, stderr = support.run_script(
                    script,
                    ["tested", "--tag", tag],
                    _sync_env(root, bin_dir),
                )
                combined = stdout + stderr
                self.assertNotIn("Traceback", combined)
                self.assertEqual(code, 1, combined)
                self.assertIn("newest tested is already v0.23.0", combined)
                self.assertEqual(compat_toml.read_bytes(), compat_before)

    def test_pin_constants_read_the_generated_table(self) -> None:
        repo_pinned_rs = (
            support.REPO / "crates" / "hermes-contract" / "src" / "pinned.rs"
        ).read_text(encoding="utf-8")
        self.assertIn(
            "pub const TAG: &str = crate::compat::CURRENT.pinned_tag;",
            repo_pinned_rs,
        )
        self.assertIn(
            "pub const COMMIT: &str = crate::compat::CURRENT.pinned_commit;",
            repo_pinned_rs,
        )
        self.assertNotIn('pub const TAG: &str = "', repo_pinned_rs)

        with support.temporary_directory() as temp_name:
            root = Path(temp_name)
            script = _prepare_sync_workspace(root, "v0.22.4")
            bin_dir = root / "bin"
            support.install_fakes(bin_dir, include_cargo=True)

            crate = root / "crates" / "hermes-contract"
            pinned_rs = crate / "src" / "pinned.rs"
            pinned_rs_before = pinned_rs.read_bytes()

            code, stdout, stderr = support.run_script(
                script,
                ["bump", "--tag", "v0.23.0"],
                _sync_env(root, bin_dir),
            )
            combined = stdout + stderr
            self.assertNotIn("Traceback", combined)
            self.assertEqual(code, 0, combined)
            self.assertEqual(pinned_rs.read_bytes(), pinned_rs_before)
            self.assertEqual(
                (crate / "PINNED").read_text(encoding="utf-8"),
                f"tag=v0.23.0\ncommit={NEW_GH_COMMIT}\n",
            )

    def test_tested_restores_copies_when_write_fails(self) -> None:
        with support.temporary_directory() as temp_name:
            root = Path(temp_name)
            script = _prepare_sync_workspace(root, "v0.22.4")
            bin_dir = root / "bin"
            support.install_fakes(bin_dir, include_cargo=True)

            compat_toml = root / "crates" / "hermes-contract" / "compat.toml"
            readme = root / "README.md"
            compat_before = compat_toml.read_bytes()
            readme_before = readme.read_bytes()

            compat_doc_dir = root / "docs" / "compatibility.md"
            compat_doc_dir.mkdir(parents=True, exist_ok=True)

            code, stdout, stderr = support.run_script(
                script,
                ["tested", "--tag", "v0.23.0"],
                _sync_env(root, bin_dir),
            )
            combined = stdout + stderr
            self.assertNotIn("Traceback", combined)
            self.assertEqual(code, 1, combined)
            self.assertEqual(compat_toml.read_bytes(), compat_before)
            self.assertEqual(readme.read_bytes(), readme_before)
            self.assertTrue(compat_doc_dir.is_dir())
            self.assertIn("docs/compatibility.md", combined)

    def test_bump_scans_only_top_level_json(self) -> None:
        with support.temporary_directory() as temp_name:
            root = Path(temp_name)
            script = _prepare_sync_workspace(root, "v0.22.4")
            bin_dir = root / "bin"
            support.install_fakes(bin_dir, include_cargo=True)

            matrix = root / "fixtures" / "configuration-surface" / "matrix.json"
            static_rows = root / "fixtures" / "configuration-surface" / "static-rows.json"
            note = root / "crates" / "note.txt"
            matrix.parent.mkdir(parents=True, exist_ok=True)
            note.parent.mkdir(parents=True, exist_ok=True)
            matrix.write_text('{"recorded_against":"v0.22.4"}\n', encoding="utf-8")
            static_rows.write_text(
                '{"meta":{"recorded_against":"v0.22.4"}}\n',
                encoding="utf-8",
            )
            note.write_text('{"recorded_against":"v0.22.4"}\n', encoding="utf-8")
            _git_add_fixtures(
                root,
                "fixtures/configuration-surface/matrix.json",
                "fixtures/configuration-surface/static-rows.json",
            )

            code, stdout, stderr = support.run_script(
                script,
                ["bump", "--tag", "v0.23.0"],
                _sync_env(root, bin_dir),
            )
            combined = stdout + stderr
            self.assertNotIn("Traceback", combined)
            self.assertEqual(code, 0, combined)
            self.assertIn("matrix.json", stdout)
            self.assertNotIn("static-rows.json", stdout)
            self.assertNotIn("note.txt", stdout)


if __name__ == "__main__":
    unittest.main()
