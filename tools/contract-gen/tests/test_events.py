"""Tests 12 and 13. Notification variants and a declared hand-written event."""

from __future__ import annotations

import json
import unittest

import contract_gen

from . import support


class EventTests(unittest.TestCase):
    def test_every_notification_and_server_request_has_a_variant(self) -> None:
        contract = support.load_contract()
        model = contract_gen.notification_model(contract)
        declared = {entry["name"] for entry in contract["x-notifications"]}
        got = {entry["name"] for entry in model["notifications"]}
        self.assertEqual(got, declared | set(contract_gen.HAND_WRITTEN_EVENTS))
        variants = {entry["variant"] for entry in model["notifications"]}
        self.assertIn(model["unknown_variant"], {"Unknown"})
        self.assertEqual(model["unknown_variant"], "Unknown")
        voice = [
            entry
            for entry in model["notifications"]
            if entry["name"] == "voice.interrupted"
        ]
        self.assertEqual(len(voice), 1)
        self.assertTrue(voice[0]["dataless"])
        self.assertEqual(voice[0]["variant"], "VoiceInterrupted")
        for entry in model["notifications"]:
            self.assertEqual(entry["variant"], contract_gen.rust_variant_name(entry["name"]))
            self.assertIn(entry["variant"], variants)
        requests = {entry["name"] for entry in contract["x-server-requests"]}
        got_requests = {entry["name"] for entry in model["server_requests"]}
        self.assertEqual(got_requests, requests)
        for entry in model["server_requests"]:
            self.assertEqual(entry["module"], entry["name"].replace(".", "_"))
            self.assertEqual(entry["variant"], contract_gen.rust_variant_name(entry["name"]))

    def test_declared_hand_written_event_is_fatal(self) -> None:
        contract = support.load_contract()
        contract["x-notifications"].append(
            {
                "name": "approval.request",
                "params": [
                    {
                        "name": "params",
                        "schema": {"$ref": "#/components/schemas/ApprovalRequest"},
                    }
                ],
            }
        )
        with support.temporary_directory() as temp_name:
            root = support.Path(temp_name)
            script = support.copy_generator(root)
            contract_path = root / "contract.json"
            contract_path.write_text(json.dumps(contract), encoding="utf-8")
            out = root / "out"
            out.mkdir()
            code, stdout, stderr = support.run_script(
                script,
                ["generate", "--contract", str(contract_path), "--out", str(out)],
            )
            combined = stdout + stderr
            self.assertNotIn("Traceback", combined)
            self.assertEqual(code, 1)
            self.assertIn(
                "the contract now declares approval.request; "
                "remove it from the hand-written list",
                combined,
            )
            self.assertEqual(list(out.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
