//! JSON-lines adapter for exercising a script against the mock backend.

use std::{
    collections::HashSet,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::Serialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::{
    engine::{self, EngineConfig, ResolveError, RunOutcome, WorkflowRef, WorkflowResolver},
    mock::MockBackend,
};

#[derive(Clone, Debug, PartialEq)]
pub struct DryRunOptions {
    pub script: PathBuf,
    pub mock: PathBuf,
    pub args: Option<Value>,
    pub max_sync_ms: Option<u64>,
    pub seed: u64,
}
pub fn parse_args(argv: &[String]) -> Result<DryRunOptions, String> {
    let (mut script, mut mock, mut args, mut max_sync_ms, mut seed) = (None, None, None, None, 0);
    let mut seen = HashSet::new();
    let mut iter = argv.iter();
    while let Some(arg) = iter.next() {
        if arg.starts_with('-') {
            if !["--mock", "--args", "--max-sync-ms", "--seed"].contains(&arg.as_str()) {
                return Err(format!("unknown flag {arg}"));
            }
            if !seen.insert(arg) {
                return Err(format!("repeated flag {arg}"));
            }
            let value = iter
                .next()
                .filter(|v| !v.starts_with("--"))
                .ok_or_else(|| format!("{arg} needs a value"))?;
            match arg.as_str() {
                "--mock" => mock = Some(PathBuf::from(value)),
                "--args" => {
                    args = Some(serde_json::from_str(value).map_err(|e| format!("--args: {e}"))?)
                }
                "--max-sync-ms" => {
                    let n = value
                        .parse::<u64>()
                        .ok()
                        .filter(|n| *n > 0)
                        .ok_or_else(|| "--max-sync-ms needs a positive integer".to_owned())?;
                    max_sync_ms = Some(n);
                }
                "--seed" => seed = value.parse().map_err(|_| "--seed needs a u64".to_owned())?,
                _ => unreachable!(),
            }
        } else if script.replace(PathBuf::from(arg)).is_some() {
            return Err("exactly one script path is required".into());
        }
    }
    Ok(DryRunOptions {
        script: script.ok_or("a script path is required")?,
        mock: mock.ok_or("--mock is required")?,
        args,
        max_sync_ms,
        seed,
    })
}

struct Files(PathBuf);
impl WorkflowResolver for Files {
    fn resolve(&self, reference: &WorkflowRef) -> Result<String, ResolveError> {
        let path = match reference {
            WorkflowRef::Name(name) => self.0.join(format!("{name}.js")),
            WorkflowRef::ScriptPath(path) => self.0.join(path),
        };
        std::fs::read_to_string(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ResolveError::NotFound
            } else {
                ResolveError::Unreadable(e.to_string())
            }
        })
    }
}
fn line(out: &mut dyn Write, value: &impl serde::Serialize) -> std::io::Result<()> {
    serde_json::to_writer(&mut *out, value)?;
    out.write_all(b"\n")?;
    out.flush()
}
pub async fn run(opts: &DryRunOptions, out: &mut dyn Write, cancel: CancellationToken) -> i32 {
    let inputs = (|| -> Result<_, String> {
        let source = std::fs::read_to_string(&opts.script).map_err(|e| format!("script: {e}"))?;
        let rules = std::fs::read_to_string(&opts.mock).map_err(|e| format!("mock: {e}"))?;
        let backend = MockBackend::from_json(&rules).map_err(|e| e.to_string())?;
        Ok((source, backend))
    })();
    let (source, backend) = match inputs {
        Ok(inputs) => inputs,
        Err(e) => {
            eprintln!("{e}");
            return 64;
        }
    };
    let parsed = match engine::parse_script(&source) {
        Ok(parsed) => parsed,
        Err(e) => {
            return if line(out, &json!({"refused": e.to_string()})).is_ok() {
                2
            } else {
                64
            };
        }
    };
    let mut config = EngineConfig::new("dry-run", 0, opts.seed);
    config.args = opts.args.clone();
    if let Some(ms) = opts.max_sync_ms {
        config.max_sync_slice = std::time::Duration::from_millis(ms);
    }
    let resolver = Arc::new(Files(
        opts.script
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .into(),
    ));
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let future = engine::run(parsed, config, Arc::new(backend), resolver, sender, cancel);
    tokio::pin!(future);
    let mut write_failed = false;
    let outcome = loop {
        tokio::select! {
            biased;
            event = receiver.recv() => {
                if let Some(event) = event { write_failed |= line(out, &event).is_err(); }
                else { break future.await; }
            }
            outcome = &mut future => break outcome,
        }
    };
    while let Ok(event) = receiver.try_recv() {
        write_failed |= line(out, &event).is_err();
    }
    #[derive(Serialize)]
    struct FailedLine<'a> {
        failed: &'a engine::ScriptFailure,
    }
    let (code, written) = match outcome {
        RunOutcome::Completed { result } => (0, line(out, &json!({"result": result}))),
        RunOutcome::Failed { error } => (1, line(out, &FailedLine { failed: &error })),
        RunOutcome::Aborted { reason } => (3, line(out, &json!({"aborted": reason}))),
    };
    if write_failed || written.is_err() {
        64
    } else {
        code
    }
}
