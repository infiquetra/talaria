use std::process::ExitCode;

use talaria_app::capabilities::CapabilitiesService;
use talaria_core::capabilities::CapData;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilitiesArgs {
    pub list: String,
    pub json: bool,
    pub profile: String,
}

pub fn parse(_args: &[String]) -> Result<CapabilitiesArgs, String> {
    todo!("capabilities stub")
}

pub fn format_list(
    _args: &CapabilitiesArgs,
    _data: &CapData,
    _hermes_home: &str,
    _user_home: &str,
) -> String {
    todo!("capabilities stub")
}

pub async fn run(_args: CapabilitiesArgs) -> ExitCode {
    todo!("capabilities stub")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutput {
    pub status: u8,
    pub stdout: String,
    pub stderr: String,
}

pub async fn collect(
    _args: &CapabilitiesArgs,
    _service: &CapabilitiesService,
    _hermes_home: &str,
    _user_home: &str,
) -> CommandOutput {
    todo!("capabilities stub")
}
