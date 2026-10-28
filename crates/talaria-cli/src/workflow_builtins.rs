//! Offline management of the embedded workflow library.
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::ExitCode,
};
use talaria_workflow::{
    builtins::{self, BUILTINS, Builtin, InstallAction, InstallMode, InstalledState},
    engine::parse_script,
    runner::runs::workflows_dir,
    staffing::{StaffingTable, talaria_home},
};

#[derive(Parser)]
#[command(name="talaria workflow builtins", color=clap::ColorChoice::Never)]
struct Command {
    #[command(subcommand)]
    command: Action,
}
#[derive(Clone, Debug, clap::Args)]
pub struct BuiltinArgs {
    #[command(subcommand)]
    command: Action,
}
#[derive(Clone, Debug, Subcommand)]
enum Action {
    List {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        workflows_dir: Option<PathBuf>,
    },
    Show {
        name: String,
    },
    Install {
        #[arg(long)]
        force: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        workflows_dir: Option<PathBuf>,
    },
    Check {
        name: String,
        #[arg(long)]
        args: Option<String>,
        #[arg(long)]
        staffing: Option<PathBuf>,
    },
}
fn get(name: &str) -> Result<&'static Builtin, (String, u8)> {
    builtins::get(name).ok_or_else(|| (format!("no built-in workflow named '{name}'; built-ins: fix-failing-tests, research, review-branch, ultracode"),2))
}
fn phases(builtin: &Builtin) -> Result<Vec<String>, (String, u8)> {
    parse_script(builtin.source)
        .map(|s| s.meta.phases.into_iter().map(|p| p.title).collect())
        .map_err(|e| (e.to_string(), 1))
}
fn show_json(value: &impl serde::Serialize) -> Result<(), (String, u8)> {
    println!(
        "{}",
        serde_json::to_string(value).map_err(|e| (e.to_string(), 1))?
    );
    Ok(())
}
fn list(dir: &Path, json_output: bool) -> Result<(), (String, u8)> {
    if json_output {
        let rows = BUILTINS.iter().map(|b| Ok(json!({"name":b.name,"summary":b.summary,"phases":phases(b)?,"roles":b.roles.iter().map(|r| json!({"role":r.role,"default_tiers":r.default_tiers,"list":r.list,"toolsets":r.toolsets,"worktree":r.worktree})).collect::<Vec<_>>(),"max_agents_default":b.max_agents_default,"installed":builtins::installed_state(dir,b)}))).collect::<Result<Vec<Value>,(String,u8)>>()?;
        return show_json(&rows);
    }
    for (i, b) in BUILTINS.iter().enumerate() {
        if i > 0 {
            println!();
        }
        println!(
            "{}: {}\n  phases: {}",
            b.name,
            b.summary,
            phases(b)?.join(", ")
        );
        for role in b.roles {
            println!(
                "  {}: {} via {}{}",
                role.role,
                role.default_tiers.join(", "),
                role.toolsets.join(", "),
                if role.worktree { " (worktree)" } else { "" }
            );
        }
        println!(
            "  agents with default arguments: at most {}\n  installed: {}",
            b.max_agents_default,
            match builtins::installed_state(dir, b) {
                InstalledState::NotInstalled => "not installed",
                InstalledState::Installed => "installed",
                InstalledState::Modified => "modified",
            }
        );
    }
    Ok(())
}
fn install(dir: &Path, force: bool, json_output: bool) -> Result<(), (String, u8)> {
    let created = !dir.exists();
    let reports = builtins::install(
        dir,
        if force {
            InstallMode::Overwrite
        } else {
            InstallMode::MissingOnly
        },
    )
    .map_err(|e| (e.to_string(), 1))?;
    if json_output {
        return show_json(&reports);
    }
    if created {
        println!("created folder {}", dir.display());
    }
    for report in reports {
        match report.action {
            InstallAction::Created => println!("created {}", report.path.display()),
            InstallAction::Unchanged => println!("unchanged {}", report.path.display()),
            InstallAction::Overwritten => println!(
                "overwritten {} (backup {})",
                report.path.display(),
                report.backup.as_deref().unwrap_or(&report.path).display()
            ),
            InstallAction::KeptModified => {
                let reason = match std::fs::symlink_metadata(&report.path) {
                    Ok(m) if m.file_type().is_symlink() => "symbolic link; not replaced",
                    Ok(m) if !m.is_file() => "not a regular file; not replaced",
                    _ => "modified; use --force to replace it",
                };
                println!("kept {} ({reason})", report.path.display());
            }
        }
    }
    Ok(())
}
fn check(name: &str, args: Option<&str>, staffing: Option<PathBuf>) -> Result<u8, (String, u8)> {
    let builtin = get(name)?;
    let args: Option<Value> = args
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| (format!("--args is not valid JSON: {e}"), 2))?;
    let slots = builtins::tiers_for(builtin, args.as_ref()).map_err(|e| (e, 2))?;
    let path = staffing
        .or_else(|| talaria_home().map(|home| StaffingTable::default_path(&home)))
        .ok_or_else(|| {
            (
                "no staffing path: set TALARIA_CONFIG_DIR or pass --staffing".into(),
                2,
            )
        })?;
    let exists = path.is_file();
    if !exists {
        println!("staffing file {} does not exist", path.display());
    }
    let table = StaffingTable::load(&path).map_err(|e| (e.to_string(), 2))?;
    let mut missing = BTreeSet::new();
    for (slot, tier) in slots {
        if let Some(spec) = table.tiers.get(&tier).filter(|_| exists) {
            println!("{slot}: tier '{tier}' -> {}/{}", spec.provider, spec.model);
        } else {
            println!("{slot}: tier '{tier}' is not defined in {}", path.display());
            missing.insert(tier);
        }
    }
    for role in builtin.roles {
        println!(
            "{} uses {}{}",
            role.role,
            role.toolsets.join(", "),
            if role.worktree { " in a worktree" } else { "" }
        );
    }
    if missing.is_empty() {
        println!("ok");
        Ok(0)
    } else {
        println!(
            "missing tiers: {}; define them in {}",
            missing.into_iter().collect::<Vec<_>>().join(", "),
            path.display()
        );
        Ok(2)
    }
}
pub fn run(args: &[String]) -> ExitCode {
    let command = match Command::try_parse_from(
        std::iter::once("talaria workflow builtins".to_owned()).chain(args.iter().cloned()),
    ) {
        Ok(parsed) => parsed,
        Err(e) => {
            let code = if e.use_stderr() { 2 } else { 0 };
            let _ = e.print();
            return ExitCode::from(code);
        }
    };
    ExitCode::from(execute(BuiltinArgs {
        command: command.command,
    }))
}
pub fn execute(args: BuiltinArgs) -> u8 {
    let result = match args.command {
        Action::List {
            json,
            workflows_dir: dir,
        } => list(&workflows_dir(dir.as_deref()), json).map(|_| 0),
        Action::Show { name } => get(&name).map(|b| {
            print!("{}", b.source);
            0
        }),
        Action::Install {
            force,
            json,
            workflows_dir: dir,
        } => install(&workflows_dir(dir.as_deref()), force, json).map(|_| 0),
        Action::Check {
            name,
            args,
            staffing,
        } => check(&name, args.as_deref(), staffing),
    };
    match result {
        Ok(code) => code,
        Err((e, code)) => {
            eprintln!("{e}");
            code
        }
    }
}
