use super::{Change, Context, Proposal, SecretDetector};
use crate::settings::{FieldView, ValueKind};
use serde_json::Value;
use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq)]
pub enum RowStatus {
    Ready,
    NoChange,
    Invalid(String),
    Refused(String),
    HandOff,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub change: Change,
    pub status: RowStatus,
    pub destructive: bool,
    pub text: String,
    pub detail: Vec<String>,
    pub effect_label: Option<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ProposalError {
    WrongProfile { named: String, connected: String },
    TooMany(usize),
}
pub fn validate(
    _proposal: &Proposal,
    _ctx: &Context,
    _detect: SecretDetector,
) -> Result<Vec<Row>, ProposalError> {
    todo!("configure stub")
}
pub fn check_value(_kind: &ValueKind, _field: &FieldView, _value: &Value) -> Result<Value, String> {
    todo!("configure stub")
}
pub fn withhold<'a>(text: &'a str, _detect: SecretDetector) -> Cow<'a, str> {
    let _ = text;
    todo!("configure stub")
}
