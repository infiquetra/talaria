"""Test 20. Committed generated files match a fresh generate."""

from __future__ import annotations

import os
import shutil
import unittest

import contract_gen

from . import support

SKIP_MESSAGE = (
    "cargo-typify " + contract_gen.TYPIFY_VERSION + " is not installed; "
    "install it with the command in tools/contract-gen/README.md"
)


class GeneratedCurrentTests(unittest.TestCase):
    def test_committed_generated_files_are_current(self) -> None:
        if shutil.which("cargo-typify") is None:
            if os.environ.get("CONTRACT_GEN_REQUIRE_TYPIFY") == "1":
                self.fail(SKIP_MESSAGE)
            self.skipTest(SKIP_MESSAGE)
        code, stdout, stderr = support.run_script(
            support.SCRIPT_PATH,
            ["check", "--require-typify"],
        )
        combined = stdout + stderr
        self.assertNotIn("Traceback", combined)
        self.assertEqual(code, 0, combined)


if __name__ == "__main__":
    unittest.main()
