#![allow(dead_code, unused_variables)]
pub fn key_name_is_safe(name: &str) -> bool {
    todo!("configuration surface probe stub")
}
pub fn is_credential_key(name: &str) -> bool {
    todo!("configuration surface probe stub")
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretClass {
    Empty,
    EnvReference,
    Masked,
    Plain,
}
impl SecretClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::EnvReference => "env-reference",
            Self::Masked => "masked",
            Self::Plain => "plain",
        }
    }
}
pub fn classify(value: Option<&str>) -> SecretClass {
    todo!("configuration surface probe stub")
}
pub struct HttpReply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
pub fn parse_response(raw: &[u8]) -> Result<HttpReply, String> {
    todo!("configuration surface probe stub")
}
pub fn write_probe_allowed(profile: &str, writes_env: Option<&str>) -> Result<(), String> {
    todo!("configuration surface probe stub")
}
pub fn config_set_allowed(key: &str, write_allowed: bool) -> bool {
    todo!("configuration surface probe stub")
}
