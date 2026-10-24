//! Run a program to completion in a pseudo-terminal.

use std::path::Path;

use crate::error::BenchError;

#[derive(Debug)]
pub struct PtyRun {
    pub output: Vec<u8>,
    pub exit_code: Option<i32>,
}

pub fn run_in_pty(
    binary: &Path,
    args: &[String],
    env: &[(String, String)],
    size: (u16, u16),
) -> Result<PtyRun, BenchError> {
    let _ = (binary, args, env, size);
    todo!("bench")
}
