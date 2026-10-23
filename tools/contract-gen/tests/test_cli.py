"""Test 19. CLI exit codes, with fake gh and cargo kept off the network."""

from __future__ import annotations

import hashlib
import unittest
from pathlib import Path

from . import support

GENERATED_PLACEHOLDER = "committed-placeholder\n"


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


if __name__ == "__main__":
    unittest.main()
