use serde_json::Value;
use std::{io::Write, path::PathBuf};
use tokio_util::sync::CancellationToken;

pub struct DryRunOptions {
    pub script: PathBuf,
    pub mock: PathBuf,
    pub args: Option<Value>,
    pub max_sync_ms: Option<u64>,
    pub seed: u64,
}
pub fn parse_args(_argv: &[String]) -> Result<DryRunOptions, String> {
    todo!()
}
pub async fn run(_opts: &DryRunOptions, _out: &mut dyn Write, _cancel: CancellationToken) -> i32 {
    todo!()
}
