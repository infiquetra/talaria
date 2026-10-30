//! Loopback board for acceptance panes.
//!
//! Prints the origin and board, then reads `advance` lines from standard input.

use std::io::{BufRead, Write};

#[path = "../tests/support/mock_kanban.rs"]
mod mock_kanban;

use mock_kanban::{MockKanban, MockOptions};

fn main() {
    let mut options = MockOptions::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--parents-open" => options.parents_open = true,
            "--tail-burst" => options.tail_burst = true,
            "--default-assignee" => {
                options.default_assignee = args.next().unwrap_or_default();
            }
            _ => {}
        }
    }
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let server = runtime.block_on(MockKanban::start(options));
    println!("MOCK_URL={}", server.origin());
    println!("MOCK_BOARD=mock");
    let _ = std::io::stdout().flush();
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim() == "advance" {
            server.advance();
        }
    }
}
