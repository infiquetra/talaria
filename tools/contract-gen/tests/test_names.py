"""Tests 8 and 9. Wire names become Rust identifiers."""

from __future__ import annotations

import unittest

import contract_gen


class NameTests(unittest.TestCase):
    def test_names_to_rust_identifiers(self) -> None:
        self.assertEqual(
            contract_gen.rust_module_name("display.switchSandboxImage"),
            "display_switch_sandbox_image",
        )
        self.assertEqual(contract_gen.rust_variant_name("gateway.ready"), "GatewayReady")
        self.assertEqual(
            contract_gen.rust_variant_name("vault.save_login"),
            "VaultSaveLogin",
        )
        self.assertEqual(contract_gen.rust_variant_name("tool.output_risk"), "ToolOutputRisk")
        self.assertEqual(contract_gen.rust_module_name("type"), "type_")
        self.assertEqual(contract_gen.rust_module_name("1"), "1_")
        self.assertEqual(
            contract_gen.rust_module_name("session.workspace.move"),
            "session_workspace_move",
        )

    def test_no_collisions_and_a_collision_is_fatal(self) -> None:
        with self.assertRaises(contract_gen.ContractError) as caught:
            contract_gen.refuse_identifier_collisions(["a.b_c", "a.bC"])
        message = str(caught.exception)
        self.assertIn("a.b_c", message)
        self.assertIn("a.bC", message)
        self.assertNotIn("Traceback", message)


if __name__ == "__main__":
    unittest.main()
