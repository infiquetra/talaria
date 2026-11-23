//! `talaria workflow check`: judge a script before a run, without the gateway or any agent.
use std::{fs, path::PathBuf};

use clap::Args;
use serde_json::Value;
use talaria_workflow::{
    builtins::{ScriptArg, get, resolve_script_arg},
    runner,
    script_check::{self, CheckOptions, CheckReport, Finding},
    staffing::{self, StaffingTable},
};

use crate::{args::required_path, workflow::safe};

#[derive(Clone, Debug, Args)]
pub struct CheckArgs {
    /// The script to check: a file path, or the name of an installed or built-in workflow.
    #[arg(value_name = "SCRIPT", value_parser = required_path)]
    pub script: PathBuf,
    /// Print one JSON object (`version`, `ok`, `errors`, `warnings`, `probe`) instead of text.
    #[arg(long)]
    pub json: bool,
    /// The JSON the script would receive as `args`, as `run --args` takes it.
    #[arg(long, conflicts_with = "args_file")]
    pub args: Option<String>,
    /// A file holding the JSON the script would receive as `args`.
    #[arg(long, value_parser = required_path)]
    pub args_file: Option<PathBuf>,
    /// The staffing table to check tier names against; defaults to the one `run` reads.
    #[arg(long, value_parser = required_path)]
    pub staffing: Option<PathBuf>,
    /// The run's working folder: `workflow({scriptPath})` falls back to it, as it does for `run
    /// --cwd`. Defaults to the current folder.
    #[arg(long, value_parser = required_path)]
    pub cwd: Option<PathBuf>,
    /// Where named workflows are found; defaults to the one `run` reads.
    #[arg(long, value_parser = required_path)]
    pub workflows_dir: Option<PathBuf>,
}

fn location(finding: &Finding) -> String {
    match (finding.line, finding.column) {
        (Some(line), Some(column)) => format!(" at line {line}, column {column}"),
        (Some(line), None) => format!(" at line {line}"),
        _ => String::new(),
    }
}

fn plural(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

pub fn render_text(script: &str, report: &CheckReport) -> String {
    let mut text = String::new();
    for (label, findings) in [("error", &report.errors), ("warning", &report.warnings)] {
        for finding in findings {
            text.push_str(&format!(
                "{label} ({}){}: {}\n",
                finding.kind,
                location(finding),
                safe(&finding.message)
            ));
        }
    }
    if report.ok {
        let warnings = if report.warnings.is_empty() {
            String::new()
        } else {
            format!(" ({})", plural(report.warnings.len(), "warning"))
        };
        text.push_str(&format!(
            "ok: {} is acceptable{warnings}\n",
            safe(report.name.as_deref().unwrap_or(script))
        ));
    } else {
        text.push_str(&format!(
            "not acceptable: {} in {}\n",
            plural(report.errors.len(), "error"),
            safe(script)
        ));
    }
    if report.probe.ran {
        text.push_str(&format!(
            "probe: {} with stand-in agent answers; reached {}; none was run\n",
            report.probe.outcome,
            plural(report.probe.agent_calls as usize, "agent call")
        ));
    }
    if report.ok {
        text.push_str("not checked: real agent answers, whether models and providers exist (that needs the gateway), and code the probe did not reach\n");
    }
    text
}

fn read_input(
    args: &CheckArgs,
    workflows_dir: &std::path::Path,
) -> Result<(String, PathBuf), String> {
    let name = args.script.to_string_lossy();
    match resolve_script_arg(&name, workflows_dir)? {
        ScriptArg::Path(path) | ScriptArg::Named { path, .. } => fs::read_to_string(&path)
            .map(|source| (source, path.clone()))
            .map_err(|e| format!("cannot read workflow script {}: {e}", path.display())),
        ScriptArg::MissingBuiltin { name, path } => get(&name)
            .map(|builtin| (builtin.source.to_owned(), path))
            .ok_or_else(|| format!("built-in workflow '{name}' is unknown")),
    }
}

fn prepare(args: &CheckArgs) -> Result<(String, CheckOptions), String> {
    let workflows_dir = runner::workflows_dir(args.workflows_dir.as_deref());
    let (source, path) = read_input(args, &workflows_dir)?;
    let input = match (&args.args, &args.args_file) {
        (Some(text), _) => Some(text.clone()),
        (_, Some(path)) => Some(
            fs::read_to_string(path)
                .map_err(|e| format!("cannot read --args-file {}: {e}", path.display()))?,
        ),
        _ => None,
    };
    let input: Option<Value> = input
        .map(|text| serde_json::from_str(&text))
        .transpose()
        .map_err(|e| format!("invalid workflow arguments JSON: {e}"))?;
    let table_path = args.staffing.clone().unwrap_or_else(|| {
        StaffingTable::default_path(
            &staffing::talaria_home().unwrap_or_else(runner::runs::configuration_dir),
        )
    });
    let table = StaffingTable::load(&table_path).map_err(|e| e.to_string())?;
    let cwd = args
        .cwd
        .clone()
        .map_or_else(std::env::current_dir, Ok)
        .and_then(fs::canonicalize)
        .map_err(|e| format!("cannot use the working folder: {e}"))?;
    let absolute = std::path::absolute(&path).unwrap_or(path);
    let script_dir = absolute.parent().map(PathBuf::from).unwrap_or_default();
    Ok((
        source,
        CheckOptions {
            args: input,
            workflows_dir: std::path::absolute(&workflows_dir).unwrap_or(workflows_dir),
            script_dir,
            cwd,
            staffing: table,
        },
    ))
}

pub async fn execute(args: CheckArgs) -> u8 {
    let report = match prepare(&args) {
        Ok((source, options)) => script_check::check_script(&source, &options).await,
        Err(message) => script_check::input_problem(message),
    };
    if args.json {
        match serde_json::to_string(&report) {
            Ok(line) => println!("{line}"),
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        }
    } else {
        print!("{}", render_text(&args.script.to_string_lossy(), &report));
    }
    if report.ok { 0 } else { 2 }
}
