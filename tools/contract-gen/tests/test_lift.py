"""Tests 3 to 7. Schema lift, closed markers, rejected keywords, unions."""

from __future__ import annotations

import json
import unittest

import contract_gen

from . import support

# cargo-typify 0.8.0 accepts ProviderUsageWindow on its own. Together with the
# other inline schemas titled Scope, deleting scope's default clears the
# failure and deleting only resets_at's default does not. The title keyword
# stays. DisplayLease.viewer_id and MessageCompletePayload.text are the other
# default rows. No fourth row: the full lifted document generates after these.
REJECTED_ROWS = (
    ("DisplayLease", "viewer_id", "default"),
    ("MessageCompletePayload", "text", "default"),
    ("ProviderUsageWindow", "scope", "default"),
    ("CanonicalCorrectionAck", "status", "title"),
    ("PromptResponseResult", "status", "title"),
    ("ProviderLimit", "scope", "title"),
    ("ProviderUsageWindow", "scope", "title"),
    ("SessionAutomationParams", "kind", "title"),
    ("CanonicalPendingPrompt", "kind", "title"),
)

# The 13 const discriminator properties at the known union sites. preprocess_schemas
# rewrites each from {"const": v} to {"enum": [v]} and drops its title.
DISCRIMINATOR_CONSTS = (
    ("ConnectorPolicyAllowBody", "mode"),
    ("ConnectorPolicyDenyBody", "mode"),
    ("ConnectorPolicyDenyAllBody", "mode"),
    ("ConnectorPolicyUnrestrictedBody", "mode"),
    ("ConnectorPolicyEffectiveAllow", "mode"),
    ("ConnectorPolicyEffectiveDeny", "mode"),
    ("ConnectorPolicyEffectiveDenyAll", "mode"),
    ("ConnectorPolicyEffectiveUnrestricted", "mode"),
    ("AccountOwner", "type"),
    ("SessionOwner", "type"),
    ("ConnectorChange", "type"),
    ("ToolsChange", "type"),
    ("WaitBarrierUntil", "type"),
)


def _property(document: dict, schema: str, prop: str) -> dict:
    return document["$defs"][schema]["properties"][prop]


class LiftTests(unittest.TestCase):
    def test_refs_point_into_defs(self) -> None:
        contract = support.load_contract()
        lifted = contract_gen.lift_schemas(contract)
        self.assertNotIn("components", lifted)
        self.assertIn("$defs", lifted)
        self.assertEqual(set(lifted["$defs"]), set(contract["components"]["schemas"]))

        def walk(node: object) -> None:
            if isinstance(node, dict):
                ref = node.get("$ref")
                if isinstance(ref, str):
                    self.assertTrue(
                        ref.startswith("#/$defs/"),
                        ref,
                    )
                    self.assertNotIn("/components/schemas/", ref)
                for child in node.values():
                    walk(child)
            elif isinstance(node, list):
                for child in node:
                    walk(child)

        walk(lifted)

    def test_closed_object_markers_removed_and_only_those(self) -> None:
        contract = support.load_contract()
        before = support.count_additional(contract["components"]["schemas"])
        lifted = contract_gen.preprocess_schemas(contract)
        after = support.count_additional(lifted)
        self.assertGreater(before["false"], 0)
        self.assertEqual(after["false"], 0)
        self.assertEqual(before["false"] - after["false"], before["false"])
        self.assertEqual(after["true"], before["true"])
        self.assertEqual(after["schema"], before["schema"])
        # The inline voice.interrupted marker lives outside components.schemas
        # and must still be present on the untouched contract.
        voice = [
            entry
            for entry in contract["x-notifications"]
            if entry["name"] == "voice.interrupted"
        ]
        self.assertEqual(len(voice), 1)
        self.assertIs(
            voice[0]["params"][0]["schema"].get("additionalProperties"),
            False,
        )

    def test_rejected_defaults_dropped_only_at_listed_sites(self) -> None:
        contract = support.load_contract()
        rows = [tuple(row) for row in contract_gen.rejected_keyword_sites()]
        self.assertEqual(sorted(rows), sorted(REJECTED_ROWS))
        lifted = contract_gen.preprocess_schemas(contract)
        for schema, prop, keyword in REJECTED_ROWS:
            self.assertNotIn(keyword, _property(lifted, schema, prop))
        scope = _property(lifted, "ProviderUsageWindow", "scope")
        self.assertIn("enum", scope)
        self.assertIn("default", _property(lifted, "DisplayLease", "reason"))
        self.assertEqual(_property(lifted, "DisplayLease", "reason").get("default"), "")
        self.assertIn("default", _property(lifted, "ProviderUsageWindow", "resets_at"))
        kept = _property(lifted, "CanonicalListParams", "scope")
        self.assertEqual(kept.get("default"), "live")
        self.assertEqual(kept.get("title"), "Scope")

    def test_root_variant_references_every_definition(self) -> None:
        contract = support.load_contract()
        lifted = contract_gen.preprocess_schemas(contract)
        rooted = contract_gen.root_anyof_document(lifted)
        names = set(rooted["$defs"])
        refs = [entry["$ref"] for entry in rooted["anyOf"]]
        self.assertEqual(sorted(refs), sorted("#/$defs/" + name for name in names))
        self.assertEqual(len(refs), len(names))

    def test_discriminator_sites_and_constrained_strings_match_the_known_tables(
        self,
    ) -> None:
        contract = support.load_contract()
        self.assertEqual(
            list(contract_gen.known_union_sites()),
            support.discriminator_sites(contract),
        )
        self.assertEqual(
            set(contract_gen.known_received_constrained_types()),
            support.received_constrained_types(contract),
        )
        self.assertEqual(support.received_constrained_types(contract), {"SessionOwner"})

        constrained = json.loads(json.dumps(contract))
        replay = constrained["components"]["schemas"]["GatewayReadyPayload"][
            "properties"
        ]["replay_epoch"]
        replay["minLength"] = 1
        with self.assertRaises(contract_gen.ContractError) as caught:
            contract_gen.validate_contract(constrained)
        self.assertIn("GatewayReadyPayload", str(caught.exception))

        extra_union = json.loads(json.dumps(contract))
        extra_union["components"]["schemas"]["GatewayReadyPayload"]["properties"][
            "brand_new_union"
        ] = {
            "type": "object",
            "discriminator": {"propertyName": "kind", "mapping": {}},
        }
        with self.assertRaises(contract_gen.ContractError) as caught_union:
            contract_gen.validate_contract(extra_union)
        self.assertIn("brand_new_union", str(caught_union.exception))

        # Every const discriminator property becomes a one value enum without a title.
        lifted = contract_gen.preprocess_schemas(contract)
        schemas = contract["components"]["schemas"]
        for schema, prop in DISCRIMINATOR_CONSTS:
            original = schemas[schema]["properties"][prop]
            self.assertIn("const", original)
            rewritten = _property(lifted, schema, prop)
            self.assertEqual(rewritten.get("enum"), [original["const"]], schema)
            self.assertNotIn("const", rewritten, schema)
            self.assertNotIn("title", rewritten, schema)
            self.assertEqual(rewritten.get("type"), original.get("type"), schema)

        # Everything else keeps its const, including WaitBarrierTarget.type (an enum
        # already) and the 15 non discriminator const properties.
        rewritten_sites = set(DISCRIMINATOR_CONSTS)
        untouched = 0
        for schema, definition in schemas.items():
            for prop, node in definition.get("properties", {}).items():
                if (schema, prop) in rewritten_sites:
                    continue
                if "const" in node or any(
                    "const" in branch
                    for branch in node.get("anyOf", [])
                    if isinstance(branch, dict)
                ):
                    untouched += 1
                    self.assertEqual(_property(lifted, schema, prop), node, schema + "." + prop)
        self.assertEqual(untouched, 15)
        self.assertEqual(
            _property(lifted, "WaitBarrierTarget", "type"),
            schemas["WaitBarrierTarget"]["properties"]["type"],
        )

    def test_inline_string_enums_sharing_a_title_must_agree(self) -> None:
        contract = support.load_contract()
        contract_gen.validate_contract(contract)

        def with_enums(first: list, second: list) -> dict:
            mutated = json.loads(json.dumps(contract))
            properties = mutated["components"]["schemas"]["GatewayReadyPayload"][
                "properties"
            ]
            properties["colliding_a"] = {
                "type": "string",
                "enum": first,
                "title": "Collision Probe",
            }
            properties["colliding_b"] = {
                "type": "string",
                "enum": second,
                "title": "Collision Probe",
            }
            return mutated

        contract_gen.validate_contract(with_enums(["x", "y"], ["y", "x"]))
        with self.assertRaises(contract_gen.ContractError) as caught:
            contract_gen.validate_contract(with_enums(["x", "y"], ["x", "z"]))
        self.assertIn("Collision Probe", str(caught.exception))


if __name__ == "__main__":
    unittest.main()
