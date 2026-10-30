//! Agent launch configuration.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcpAgentEntry {
    pub id: String,
    pub display_name: String,
    pub command: String,
    pub args: Vec<String>,
    pub auth_method: Option<String>,
    pub allow_fs: bool,
    pub allow_terminal: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AcpConfig {
    pub agents: Vec<AcpAgentEntry>,
}

impl AcpConfig {
    pub fn parse(_text: &str) -> Result<Self, String> {
        todo!("add agent configuration")
    }
}
