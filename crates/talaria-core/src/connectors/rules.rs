//! Policy layers. Only the member layer is written.

use std::collections::BTreeMap;

use serde::Deserialize;

/// `connectors.policy.get` and the body of a set reply.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Policy {
    #[serde(default)]
    pub layers: Vec<PolicyLayer>,
    #[serde(default)]
    pub effective: Option<EffectivePolicy>,
}

/// One policy layer.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PolicyLayer {
    pub kind: String,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub body: PolicyBody,
}

/// The layer body the switch and the tool list read.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct PolicyBody {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub connectors: Vec<String>,
    #[serde(default)]
    pub disabled_connectors: Vec<String>,
    #[serde(default)]
    pub tools: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub tags: Option<PolicyTags>,
}

/// Tag locks on a layer.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct PolicyTags {
    #[serde(default)]
    pub disable: Vec<String>,
    #[serde(default)]
    pub enable: Vec<String>,
}

/// The effective policy. Talaria displays it and does not write it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct EffectivePolicy {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub disabled_connectors: Vec<String>,
    #[serde(default)]
    pub tools: BTreeMap<String, Vec<String>>,
}

/// Answer to a policy write. The following reload is separate.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct PolicySetReply {
    #[serde(default)]
    pub revision: Option<String>,
}

/// `connectors.tools`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ToolsReply {
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub tools: Vec<ToolSpec>,
}

/// One tool the card can draft.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ToolSpec {
    pub slug: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub facet: String,
    #[serde(default)]
    pub hints: Vec<String>,
    #[serde(default)]
    pub deprecated: bool,
}
