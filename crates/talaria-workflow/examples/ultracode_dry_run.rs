//! Exercise the complete built-in workflow and its review child offline.
use std::{path::Path, process::ExitCode};
use talaria_workflow::dry_run::{DryRunOptions, run};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<ExitCode, serde_json::Error> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let args = serde_json::from_str(include_str!(
        "../tests/data/builtins/ultracode/happy.args.json"
    ))?;
    let options = DryRunOptions {
        script: root.join("workflows/ultracode.js"),
        mock: root.join("tests/data/builtins/ultracode/happy.mock.json"),
        args: Some(args),
        max_sync_ms: None,
        seed: 0,
    };
    let code = run(&options, &mut std::io::stdout(), CancellationToken::new()).await;
    Ok(ExitCode::from(u8::try_from(code).unwrap_or(1)))
}
