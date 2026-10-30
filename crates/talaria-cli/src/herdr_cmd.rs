//! Live terminal agent diagnostics.

use std::path::PathBuf;

pub fn run(args: &[String]) -> u8 {
    if args.len() < 2
        || args[0] != "herdr"
        || args[1] != "panes"
        || args[2..].iter().any(|arg| arg != "--probe")
        || args[2..].len() > 1
    {
        eprintln!("usage: talaria herdr panes [--probe]");
        return 2;
    }
    let loaded = match std::env::var_os("HOME") {
        Some(home) => talaria_app::config::load_user_config(&PathBuf::from(home)),
        None => talaria_app::config::parse(""),
    };
    let config = loaded.config.herdr.fleet_config();
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            eprintln!("talaria herdr: unable to start the async runtime");
            return 1;
        }
    };
    match runtime.block_on(talaria_app::herdr::list_panes(&config, args.len() == 3)) {
        Ok(panes) => {
            for pane in panes {
                let clean = |text: &str| talaria_core::text::sanitize(text).into_owned();
                println!(
                    "{}  {}  {}  {}  {}",
                    clean(&pane.pane_id),
                    clean(&pane.agent),
                    clean(&pane.status),
                    clean(&pane.workspace_id),
                    clean(&pane.tab_id)
                );
            }
            0
        }
        Err(error) => {
            eprintln!(
                "talaria herdr: {}",
                talaria_core::text::sanitize(&error.to_string())
            );
            1
        }
    }
}
