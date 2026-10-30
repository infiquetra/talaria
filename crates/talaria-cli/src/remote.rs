//! Remote gateway command and configuration stubs.
#![allow(dead_code)]
use hermes_client::remote::RemoteConfig;
use std::path::Path;

pub struct Remotes {
    pub entries: Vec<RemoteConfig>,
}
impl Remotes {
    pub fn load(_home: &Path) -> Result<Self, String> {
        todo!("add remote gateway implementation")
    }
    pub fn save(&self, _home: &Path) -> Result<(), String> {
        todo!("add remote gateway implementation")
    }
    pub fn add(&mut self, _config: RemoteConfig) -> Result<(), String> {
        todo!("add remote gateway implementation")
    }
    pub fn remove(&mut self, _name: &str) -> Result<(), String> {
        todo!("add remote gateway implementation")
    }
}
pub fn run(_args: &[String]) -> u8 {
    todo!("add remote gateway implementation")
}
