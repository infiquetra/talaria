//! Discovery of live terminal agents.

mod config;
mod service;

pub use config::HerdrConfig;
pub use service::{HerdrClient, HerdrError, list_panes};
