#[path = "../../talaria-app/tests/support/capabilities_fixture.rs"]
mod capabilities_fixture;
use capabilities_fixture::mock_gateway;
#[path = "support/mod.rs"]
mod support;

#[path = "integration/acp_cmd.rs"]
mod acp_cmd;
#[path = "integration/attach_cli.rs"]
mod attach_cli;
#[path = "integration/bench_replay.rs"]
mod bench_replay;
#[path = "integration/bg_cmd.rs"]
mod bg_cmd;
#[path = "integration/capabilities_cmd.rs"]
mod capabilities_cmd;
#[path = "integration/cli.rs"]
mod cli;
#[path = "integration/cli_args.rs"]
mod cli_args;
#[path = "integration/cli_process.rs"]
mod cli_process;
#[path = "integration/configure_cmd.rs"]
mod configure_cmd;
#[path = "integration/doctor_cli.rs"]
mod doctor_cli;
#[path = "integration/fleet_cmd.rs"]
mod fleet_cmd;
#[path = "integration/fleet_external_format.rs"]
mod fleet_external_format;
#[path = "integration/herdr_cmd.rs"]
mod herdr_cmd;
#[path = "integration/live_acp.rs"]
mod live_acp;
#[path = "integration/live_attach_cmd.rs"]
mod live_attach_cmd;
#[path = "integration/live_attachments.rs"]
mod live_attachments;
#[path = "integration/live_attention.rs"]
mod live_attention;
#[path = "integration/live_automation.rs"]
mod live_automation;
#[path = "integration/live_background.rs"]
mod live_background;
#[path = "integration/live_bg.rs"]
mod live_bg;
#[path = "integration/live_comforts.rs"]
mod live_comforts;
#[path = "integration/live_configure.rs"]
mod live_configure;
#[path = "integration/live_fleet.rs"]
mod live_fleet;
#[path = "integration/live_model.rs"]
mod live_model;
#[path = "integration/live_pager.rs"]
mod live_pager;
#[path = "integration/live_permissions.rs"]
mod live_permissions;
#[path = "integration/live_queue.rs"]
mod live_queue;
#[path = "integration/live_rewind.rs"]
mod live_rewind;
#[path = "integration/live_session_picker.rs"]
mod live_session_picker;
#[path = "integration/live_side.rs"]
mod live_side;
#[path = "integration/live_stream.rs"]
mod live_stream;
#[path = "integration/mcp_wiring.rs"]
mod mcp_wiring;
#[path = "integration/print_args.rs"]
mod print_args;
#[path = "integration/print_process.rs"]
mod print_process;
#[path = "integration/profiles_cmd.rs"]
mod profiles_cmd;
#[path = "integration/remote_cli.rs"]
mod remote_cli;
#[path = "integration/stream_args.rs"]
mod stream_args;
#[path = "integration/stream_process.rs"]
mod stream_process;
#[path = "integration/voice_vim.rs"]
mod voice_vim;
#[path = "integration/workflow_builtins_live.rs"]
mod workflow_builtins_live;
#[path = "integration/workflow_cli.rs"]
mod workflow_cli;
#[path = "integration/workflow_compile_cli.rs"]
mod workflow_compile_cli;
#[path = "integration/workflow_compile_empty.rs"]
mod workflow_compile_empty;
#[path = "integration/workflow_live.rs"]
mod workflow_live;
#[path = "integration/workflow_skill_cli.rs"]
mod workflow_skill_cli;
