use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use serde_json::Value;
use std::fmt;

#[derive(Clone, PartialEq)]
pub struct ConfigSchemaView {
    pub fields: Vec<FieldView>,
    pub category_order: Vec<String>,
}
#[derive(Clone, PartialEq)]
pub struct FieldView {
    pub key: String,
    pub kind: FieldKind,
    pub description: String,
    pub category: String,
    pub options: Option<Vec<Value>>,
    pub searchable: bool,
    pub clearable: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Boolean,
    Number,
    String,
    Text,
    List,
    Select,
    Object,
    Other(String),
}
impl FieldKind {
    pub fn name(&self) -> &str {
        match self {
            Self::Boolean => "boolean",
            Self::Number => "number",
            Self::String => "string",
            Self::Text => "text",
            Self::List => "list",
            Self::Select => "select",
            Self::Object => "object",
            Self::Other(s) => s,
        }
    }
}
impl ConfigSchemaView {
    pub fn from_json_str(text: &str) -> Result<Self, serde_json::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(deserialize_with = "ordered_fields")]
            fields: Vec<FieldView>,
            #[serde(default)]
            category_order: Vec<String>,
        }
        let wire: Wire = serde_json::from_str(text)?;
        Ok(Self {
            fields: wire.fields,
            category_order: wire.category_order,
        })
    }
    pub fn categories(&self) -> Vec<String> {
        let mut remaining: std::collections::BTreeSet<_> =
            self.fields.iter().map(|f| f.category.clone()).collect();
        let mut result = Vec::new();
        for category in &self.category_order {
            if remaining.remove(category) {
                result.push(category.clone());
            }
        }
        result.extend(remaining);
        result
    }
    pub(crate) fn ordered_fields(&self) -> Vec<&FieldView> {
        self.categories()
            .iter()
            .flat_map(|category| self.fields.iter().filter(move |f| f.category == *category))
            .collect()
    }
}
fn ordered_fields<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<FieldView>, D::Error> {
    struct Fields;
    impl<'de> Visitor<'de> for Fields {
        type Value = Vec<FieldView>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an ordered field object")
        }
        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut fields = Vec::new();
            while let Some((key, value)) = map.next_entry::<String, Value>()? {
                let kind = match value
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("string")
                {
                    "boolean" => FieldKind::Boolean,
                    "number" => FieldKind::Number,
                    "string" => FieldKind::String,
                    "text" => FieldKind::Text,
                    "list" => FieldKind::List,
                    "select" => FieldKind::Select,
                    "object" => FieldKind::Object,
                    other => FieldKind::Other(other.into()),
                };
                fields.push(FieldView {
                    description: value
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or(&key)
                        .into(),
                    category: value
                        .get("category")
                        .and_then(Value::as_str)
                        .unwrap_or("general")
                        .into(),
                    options: value.get("options").and_then(Value::as_array).cloned(),
                    searchable: value
                        .get("searchable")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    clearable: value
                        .get("clearable")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    key,
                    kind,
                });
            }
            Ok(fields)
        }
    }
    deserializer.deserialize_map(Fields)
}
impl fmt::Debug for ConfigSchemaView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfigSchemaView")
            .field("fields", &self.fields.len())
            .field("categories", &self.category_order.len())
            .finish()
    }
}
impl fmt::Debug for FieldView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FieldView")
            .field("kind", &self.kind)
            .field("options", &self.options.as_ref().map(Vec::len))
            .finish_non_exhaustive()
    }
}
