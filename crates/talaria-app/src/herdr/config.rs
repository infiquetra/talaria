//! Terminal agent discovery configuration.

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HerdrConfig {
    pub enabled: bool,
    pub binary: PathBuf,
    pub poll_interval_ms: u64,
    pub workspace_filter: Option<String>,
}
