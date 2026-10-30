//! Diagnostics for configured agent processes.

use std::path::Path;

use talaria_app::acp::{AcpConfig, probe_agent};

pub fn run(args: &[String]) -> u8 {
    let probe = match args {
        [command, subcommand] if command == "acp" && subcommand == "agents" => false,
        [command, subcommand, flag]
            if command == "acp" && subcommand == "agents" && flag == "--probe" =>
        {
            true
        }
        _ => {
            eprintln!("Usage: talaria acp agents [--probe]");
            return 2;
        }
    };
    let Some(home) = std::env::var_os("HOME") else {
        eprintln!("Talaria could not read ~/.talaria/config.toml (home is unavailable).");
        return 2;
    };
    let path = talaria_app::config::user_config_path(Path::new(&home));
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return 0,
        Err(error) => {
            eprintln!(
                "Talaria could not read ~/.talaria/config.toml ({}). Using the default settings.",
                error.kind()
            );
            return 2;
        }
    };
    let config = match AcpConfig::parse(&text) {
        Ok(config) => config,
        Err(_) => {
            for notice in talaria_app::config::parse(&text).notices {
                eprintln!("{notice}");
            }
            return 2;
        }
    };
    let runtime = if probe {
        match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => Some(runtime),
            Err(_) => {
                eprintln!("Could not start the agent probe.");
                return 1;
            }
        }
    } else {
        None
    };
    let mut status = 0;
    for entry in &config.agents {
        let id = talaria_core::text::sanitize(&entry.id);
        let name = talaria_core::text::sanitize(&entry.display_name);
        if !command_exists(&entry.command) {
            let command = talaria_core::text::sanitize(&entry.command);
            println!("{id}: {name} - missing: command \"{command}\" not found");
            status = 1;
            continue;
        }
        if let Some(runtime) = &runtime {
            match runtime.block_on(probe_agent(entry)) {
                Ok(report) => println!(
                    "{id}: {name} - ok - {}, protocol {}",
                    report.agent_name, report.protocol_version
                ),
                Err(error) => {
                    eprintln!("{id}: probe failed: {error}");
                    status = 1;
                }
            }
        } else {
            println!("{id}: {name} - ok");
        }
    }
    status
}

fn command_exists(command: &str) -> bool {
    if command.contains('/') {
        return executable(Path::new(command));
    }
    std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join(command))
                .any(|candidate| executable(&candidate))
        })
        .unwrap_or(false)
}

fn executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}
