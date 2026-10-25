//! Comforts demo: `--config <file>`, `--history-file <file>`, `--folder <key>`,
//! `--alert-after-submit <needs_you|done|failed>`.
//!
//! Stub: without a terminal it prints one line and exits 2, like the composer demo; the
//! wired demo (terminal, composer, editor, search, suspend, alerts) is built later.
//! Behavior contract: issue 228.

use std::io::IsTerminal;
use std::process::ExitCode;

fn main() -> ExitCode {
    let terminal = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    if terminal {
        println!("comforts demo: stub");
    } else {
        println!("comforts demo needs a terminal");
    }
    ExitCode::from(2)
}
