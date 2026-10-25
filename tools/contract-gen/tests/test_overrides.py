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


if __name__ == "__main__":
    unittest.main()
