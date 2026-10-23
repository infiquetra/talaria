"""Tests 1 and 2. The pin file and the vendored bytes."""

from __future__ import annotations

import hashlib
import unittest

import contract_gen

from . import support


class PinTests(unittest.TestCase):
    def test_pinned_format_commit_and_minimum_version(self) -> None:
        text = support.PINNED_PATH.read_text(encoding="utf-8")
        lines = [line for line in text.splitlines() if line]
        self.assertEqual(len(lines), 2)
        self.assertTrue(lines[0].startswith("tag="))
        self.assertTrue(lines[1].startswith("commit="))
        pin = contract_gen.parse_pinned(text)
        self.assertEqual(pin.tag, lines[0].split("=", 1)[1])
        self.assertEqual(pin.commit, lines[1].split("=", 1)[1])
        self.assertEqual(len(pin.commit), 40)
        self.assertTrue(all(char in "0123456789abcdef" for char in pin.commit))
        # The annotated tag object is not the commit. v0.22.4's tag object starts
        # with 64dbae64747a; commit= must not be that object.
        self.assertFalse(pin.commit.startswith("64dbae64747a"))
        contract_gen.validate_pinned(pin)
        self.assertGreater(contract_gen.compare_versions("v0.22.10", "v0.22.4"), 0)
        self.assertLess(contract_gen.compare_versions("v0.22.4", "v0.22.10"), 0)
        too_old = contract_gen.Pin(tag="v0.22.2", commit=pin.commit)
        with self.assertRaises(contract_gen.PinError):
            contract_gen.validate_pinned(too_old)

    def test_vendored_contract_matches_recorded_sha256(self) -> None:
        contract = support.CONTRACT_PATH
        recorded = (support.VENDOR / (contract.name + ".sha256")).read_bytes()
        self.assertFalse(recorded.startswith(b"\xef\xbb\xbf"))
        self.assertNotIn(b"\r", recorded)
        text = recorded.decode("ascii")
        self.assertTrue(text.endswith("\n"))
        self.assertEqual(text.count("\n"), 1)
        digest, separator, name = text[:-1].partition("  ")
        self.assertEqual(separator, "  ")
        self.assertEqual(name, contract.name)
        self.assertEqual(len(digest), 64)
        self.assertTrue(all(char in "0123456789abcdef" for char in digest))
        body = contract.read_bytes()
        self.assertFalse(body.startswith(b"\xef\xbb\xbf"))
        self.assertNotIn(b"\r", body)
        self.assertEqual(hashlib.sha256(body).hexdigest(), digest)
        mutated = bytearray(body)
        mutated[-1] ^= 0x01
        self.assertNotEqual(hashlib.sha256(bytes(mutated)).hexdigest(), digest)
        licence = (support.VENDOR / "LICENSE.hermes").read_bytes()
        self.assertGreater(len(licence), 0)
        self.assertFalse(licence.startswith(b"\xef\xbb\xbf"))
        self.assertNotIn(b"\r", licence)


if __name__ == "__main__":
    unittest.main()
