//! Headless print mode: run one prompt without a screen and print the answer.

pub mod input;
pub mod output;
pub mod run;
pub mod service;
pub mod wire;

/// How the answer is printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

/// The headless options one run needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintOptions {
    pub prompt_arg: Option<String>,
    pub format: OutputFormat,
    pub timeout: Option<std::time::Duration>,
}
