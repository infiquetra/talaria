use super::{Context, Need, SecretDetector};

#[derive(Clone, Debug, PartialEq)]
pub struct Brief {
    pub text: String,
    pub omitted_keys: usize,
    pub shortened_lists: Vec<&'static str>,
    pub withheld: usize,
}
pub fn build_brief(_ctx: &Context, _request: &str, _detect: SecretDetector) -> Brief {
    todo!("configure stub")
}
pub fn build_context_reply(_ctx: &Context, _need: &Need, _detect: SecretDetector) -> String {
    todo!("configure stub")
}
pub fn build_refine_text(operator_text: &str) -> String {
    let _ = operator_text;
    todo!("configure stub")
}
