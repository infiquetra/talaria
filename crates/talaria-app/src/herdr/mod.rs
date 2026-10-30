//! Discovery and actions for live terminal agents.

mod config;
mod service;

pub use config::HerdrConfig;
pub use service::{HerdrError, HerdrService};
