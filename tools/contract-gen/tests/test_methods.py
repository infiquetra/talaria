"""Tests 10 and 11. One module per method, per transport."""

from __future__ import annotations

import unittest

import contract_gen

from . import support


class MethodTests(unittest.TestCase):
    def test_one_module_per_method_per_transport(self) -> None:
        contract = support.load_contract()
        catalogue = contract_gen.method_catalogue(contract)
        legacy_names = [method["name"] for method in contract["methods"]]
        canonical_names = [method["name"] for method in contract["x-canonical-methods"]]
        self.assertEqual(len(legacy_names), len(set(legacy_names)))
        self.assertEqual(
            [row["name"] for row in catalogue["legacy"]],
            legacy_names,
        )
        self.assertEqual(
            [row["name"] for row in catalogue["canonical"]],
            canonical_names,
        )
        for row in catalogue["legacy"] + catalogue["canonical"]:
            self.assertEqual(row["module"], contract_gen.rust_module_name(row["name"]))
            self.assertTrue(row["params"])
            self.assertTrue(row["result"])

    def test_shared_names_point_at_their_own_schemas(self) -> None:
        contract = support.load_contract()
        catalogue = contract_gen.method_catalogue(contract)
        by_transport = {
            "legacy": {row["name"]: row for row in catalogue["legacy"]},
            "canonical": {row["name"]: row for row in catalogue["canonical"]},
        }
        legacy = {method["name"]: method for method in contract["methods"]}
        canonical = {method["name"]: method for method in contract["x-canonical-methods"]}
        shared = sorted(set(legacy) & set(canonical))
        self.assertGreater(len(shared), 0)
        identical = []
        for name in shared:
            left = support.method_ref_pair(legacy[name])
            right = support.method_ref_pair(canonical[name])
            self.assertEqual(by_transport["legacy"][name]["params"], _schema(left[0]))
            self.assertEqual(by_transport["legacy"][name]["result"], _schema(left[1]))
            self.assertEqual(by_transport["canonical"][name]["params"], _schema(right[0]))
            self.assertEqual(by_transport["canonical"][name]["result"], _schema(right[1]))
            if left == right:
                identical.append(name)
        self.assertEqual(identical, ["clarify.lock"])


def _schema(ref: str) -> str:
    prefix = "#/components/schemas/"
    if ref.startswith(prefix):
        return ref[len(prefix) :]
    return ref


if __name__ == "__main__":
    unittest.main()
