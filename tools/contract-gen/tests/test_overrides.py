"""The archived_truncated field is added to TranscriptMessage and its builder."""

from __future__ import annotations

import unittest

import contract_gen

TYPIFY_SHAPE = """pub struct TranscriptMessage {
    pub role: ::std::string::String,
}
impl TranscriptMessage {
    pub fn builder() -> builder::TranscriptMessage {
        Default::default()
    }
}
pub mod builder {
    pub struct TranscriptMessage {
        role: ::std::result::Result<::std::string::String, ::std::string::String>,
    }
    impl ::std::default::Default for TranscriptMessage {
        fn default() -> Self {
            Self {
                role: Err("no value supplied for role".to_string()),
            }
        }
    }
    impl TranscriptMessage {
        pub fn role<T>(mut self, value: T) -> Self {
            self
        }
    }
    impl ::std::convert::TryFrom<TranscriptMessage> for super::TranscriptMessage {
        type Error = super::error::ConversionError;
        fn try_from(value: TranscriptMessage) -> ::std::result::Result<Self, super::error::ConversionError> {
            Ok(Self {
                role: value.role?,
            })
        }
    }
    impl ::std::convert::From<super::TranscriptMessage> for TranscriptMessage {
        fn from(value: super::TranscriptMessage) -> Self {
            Self {
                role: Ok(value.role),
            }
        }
    }
}
"""


class ArchivedTruncatedTests(unittest.TestCase):
    def test_every_site_gains_the_field_once(self) -> None:
        text = contract_gen.postprocess_schemas_rs(TYPIFY_SHAPE)
        self.assertEqual(text.count("pub archived_truncated: ::std::option::Option<bool>,"), 1)
        self.assertEqual(text.count("crate::overrides::deserialize_archived_truncated"), 1)
        self.assertEqual(text.count("archived_truncated: ::std::result::Result<"), 1)
        self.assertEqual(text.count("archived_truncated: Ok(Default::default()),"), 1)
        self.assertEqual(text.count("pub fn archived_truncated<T>"), 1)
        self.assertEqual(text.count("archived_truncated: value.archived_truncated?,"), 1)
        self.assertEqual(text.count("archived_truncated: Ok(value.archived_truncated),"), 1)
        self.assertLess(text.index("pub archived_truncated"), text.index("pub role"))

    def test_output_without_the_struct_is_unchanged(self) -> None:
        self.assertEqual(contract_gen.postprocess_schemas_rs("// typify\n"), "// typify\n")

    def test_a_missing_builder_site_is_fatal(self) -> None:
        drifted = TYPIFY_SHAPE.replace("Ok(Self {", "Ok(Self::new(")
        with self.assertRaises(contract_gen.ContractError):
            contract_gen.postprocess_schemas_rs(drifted)

    def test_a_schema_that_declares_the_field_is_fatal(self) -> None:
        declared = TYPIFY_SHAPE.replace(
            "    pub role:", "    pub archived_truncated: ::std::option::Option<bool>,\n    pub role:", 1
        )
        with self.assertRaises(contract_gen.ContractError) as caught:
            contract_gen.postprocess_schemas_rs(declared)
        self.assertIn("remove the override", str(caught.exception))


class ModelContractTests(unittest.TestCase):
    def test_mutation_result_keeps_unlisted_receipt_fields(self) -> None:
        contract = {"components": {"schemas": {
            "SessionMutateResult": {"type": "object", "properties": {
                "session_id": {"type": "string"},
            }},
            "OtherResult": {"type": "object"},
        }}}
        projected = contract_gen.preprocess_schemas(contract)["$defs"]
        self.assertEqual(projected["SessionMutateResult"]["additionalProperties"], {})
        self.assertNotIn("additionalProperties", projected["OtherResult"])
        self.assertNotIn("additionalProperties", contract["components"]["schemas"]["SessionMutateResult"])

    def test_normal_catalogue_omits_refresh_false_only_at_its_site(self) -> None:
        text = """pub struct ModelOptionsParams {
    #[serde(default)]
    pub include_unconfigured: bool,
    #[serde(default)]
    pub refresh: bool,
}
pub struct OtherParams {
    #[serde(default)]
    pub refresh: bool,
}
"""
        result = contract_gen.postprocess_schemas_rs(text)
        expected = text.replace(
            "#[serde(default)]", '#[serde(default, skip_serializing_if = "::std::ops::Not::not")]', 2
        )
        self.assertEqual(result, expected)

    def test_changed_refresh_shape_requires_a_deliberate_override_update(self) -> None:
        with self.assertRaises(contract_gen.ContractError):
            contract_gen.postprocess_schemas_rs("pub struct ModelOptionsParams { pub refresh: Option<bool>, }")


if __name__ == "__main__":
    unittest.main()
