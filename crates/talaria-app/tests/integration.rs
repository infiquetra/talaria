#[path = "support/acp_harness.rs"]
mod acp_harness;

// The fixture keeps nested helpers for consumers in other test binaries.
#[allow(clippy::duplicate_mod)]
#[path = "support/capabilities_fixture.rs"]
mod capabilities_fixture;

#[path = "../../hermes-client/tests/support/mod.rs"]
// The public fixtures also include the REST helper for other test binaries.
#[allow(clippy::duplicate_mod)]
mod client_support;

#[path = "support/convert.rs"]
mod convert;

#[path = "support/directory_drive.rs"]
mod directory_drive;

#[path = "../../talaria-core/tests/support/events.rs"]
mod events;

#[path = "support/fake_fleet.rs"]
mod fake_fleet;

#[path = "support/gh_stub.rs"]
mod gh_stub;

#[path = "support/git_fixture.rs"]
mod git_fixture;

#[path = "support/herdr_harness.rs"]
mod herdr_harness;

#[path = "../../talaria-workflow/tests/support/lines.rs"]
mod lines;

#[path = "support/lockfile_guard.rs"]
mod lockfile_guard;

#[path = "support/mock_gateway.rs"]
// The public capability fixture also includes its own gateway helper.
#[allow(clippy::duplicate_mod)]
mod mock_gateway;

#[path = "../../talaria-workflow/tests/support/replay_run.rs"]
mod replay;

#[path = "support/replay_tools.rs"]
mod replay_tools;

#[path = "../../talaria-core/tests/support/snapshot_json.rs"]
mod snapshot_json;

#[path = "support/voice_gateway.rs"]
mod voice_gateway;

#[path = "integration/acp_wire.rs"]
mod acp_wire;

#[path = "integration/appearance_flow.rs"]
mod appearance_flow;

#[path = "integration/appearance_replay.rs"]
mod appearance_replay;

#[path = "integration/attach_resolve.rs"]
mod attach_resolve;

#[path = "integration/attach_wire.rs"]
mod attach_wire;

#[path = "integration/attachments_budget.rs"]
mod attachments_budget;

#[path = "integration/attachments_replay.rs"]
mod attachments_replay;

#[path = "integration/attachments_routing.rs"]
mod attachments_routing;

#[path = "integration/attachments_service.rs"]
mod attachments_service;

#[path = "integration/attention_fleet.rs"]
mod attention_fleet;

#[path = "integration/attention_fleet_replay.rs"]
mod attention_fleet_replay;

#[path = "integration/attention_replay.rs"]
mod attention_replay;

#[path = "integration/automation_replay.rs"]
mod automation_replay;

#[path = "integration/automation_service.rs"]
mod automation_service;

#[path = "integration/background_replay.rs"]
mod background_replay;

#[path = "integration/background_service.rs"]
mod background_service;

#[path = "integration/background_wire.rs"]
mod background_wire;

#[path = "integration/capabilities_replay.rs"]
mod capabilities_replay;

#[path = "integration/capabilities_service.rs"]
mod capabilities_service;

#[path = "integration/checklist_wiring.rs"]
mod checklist_wiring;

#[path = "integration/clipboard_reader.rs"]
mod clipboard_reader;

#[path = "integration/commands_mock.rs"]
mod commands_mock;

#[path = "integration/commands_replay.rs"]
mod commands_replay;

#[path = "integration/config_comforts.rs"]
mod config_comforts;

#[path = "integration/config_doc.rs"]
mod config_doc;

#[path = "integration/config_fleet_attention.rs"]
mod config_fleet_attention;

#[path = "integration/config_vim.rs"]
mod config_vim;

#[path = "integration/configure_service.rs"]
mod configure_service;

#[path = "integration/configure_wire.rs"]
mod configure_wire;

#[path = "integration/connectors_service.rs"]
mod connectors_service;

#[path = "integration/context_commands.rs"]
mod context_commands;

#[path = "integration/context_replay.rs"]
mod context_replay;

#[path = "integration/context_wiring.rs"]
mod context_wiring;

#[path = "integration/cron_service.rs"]
mod cron_service;

#[path = "integration/directory_replay.rs"]
mod directory_replay;

#[path = "integration/directory_wire.rs"]
mod directory_wire;

#[path = "integration/doctor_service.rs"]
mod doctor_service;

#[path = "integration/export_flow.rs"]
mod export_flow;

#[path = "integration/fleet_app.rs"]
mod fleet_app;

#[path = "integration/fleet_foreground.rs"]
mod fleet_foreground;

#[path = "integration/fleet_pane_flow.rs"]
mod fleet_pane_flow;

#[path = "integration/fleet_replay.rs"]
mod fleet_replay;

#[path = "integration/fleet_view_flow.rs"]
mod fleet_view_flow;

#[path = "integration/fleet_wire.rs"]
mod fleet_wire;

#[path = "integration/headless_flow.rs"]
mod headless_flow;

#[path = "integration/headless_stream.rs"]
mod headless_stream;

#[path = "integration/headless_wire.rs"]
mod headless_wire;

#[path = "integration/herdr.rs"]
mod herdr;

#[path = "integration/herdr_config.rs"]
mod herdr_config;

#[path = "integration/herdr_wire.rs"]
mod herdr_wire;

#[path = "integration/launch_machine.rs"]
mod launch_machine;

#[path = "integration/launch_names.rs"]
mod launch_names;

#[path = "integration/launch_outcome.rs"]
mod launch_outcome;

#[path = "integration/launch_replay.rs"]
mod launch_replay;

#[path = "integration/launch_tui_flow.rs"]
mod launch_tui_flow;

#[path = "integration/launch_wire.rs"]
mod launch_wire;

#[path = "integration/layout_state.rs"]
mod layout_state;

#[path = "integration/live_context.rs"]
mod live_context;

#[path = "integration/live_fleet_view.rs"]
mod live_fleet_view;

#[path = "integration/live_review_gh.rs"]
mod live_review_gh;

#[path = "integration/live_tool_cells.rs"]
mod live_tool_cells;

#[path = "integration/mcp_service.rs"]
mod mcp_service;

#[path = "integration/memory_service.rs"]
mod memory_service;

#[path = "integration/model_replay.rs"]
mod model_replay;

#[path = "integration/model_roles_service.rs"]
mod model_roles_service;

#[path = "integration/model_saved.rs"]
mod model_saved;

#[path = "integration/model_service.rs"]
mod model_service;

#[path = "integration/mutate_wire.rs"]
mod mutate_wire;

#[path = "integration/mutation_client.rs"]
mod mutation_client;

#[path = "integration/mutation_namespaces.rs"]
mod mutation_namespaces;

#[path = "integration/no_audio_deps.rs"]
mod no_audio_deps;

#[path = "integration/pager_keys.rs"]
mod pager_keys;

#[path = "integration/pager_wiring.rs"]
mod pager_wiring;

#[path = "integration/palette_routing.rs"]
mod palette_routing;

#[path = "integration/panes_exec.rs"]
mod panes_exec;

#[path = "integration/panes_plan.rs"]
mod panes_plan;

#[path = "integration/peek_replay.rs"]
mod peek_replay;

#[path = "integration/peek_wire.rs"]
mod peek_wire;

#[path = "integration/permissions_app.rs"]
mod permissions_app;

#[path = "integration/permissions_replay.rs"]
mod permissions_replay;

#[path = "integration/permissions_service.rs"]
mod permissions_service;

#[path = "integration/permissions_yolo.rs"]
mod permissions_yolo;

#[path = "integration/profiles_replay.rs"]
mod profiles_replay;

#[path = "integration/profiles_service.rs"]
mod profiles_service;

#[path = "integration/prompt_history.rs"]
mod prompt_history;

#[path = "integration/prompts_props.rs"]
mod prompts_props;

#[path = "integration/prompts_replay.rs"]
mod prompts_replay;

#[path = "integration/prompts_respond.rs"]
mod prompts_respond;

#[path = "integration/prompts_secrecy.rs"]
mod prompts_secrecy;

#[path = "integration/prompts_server_requests.rs"]
mod prompts_server_requests;

#[path = "integration/provider_service.rs"]
mod provider_service;

#[path = "integration/queue_replay.rs"]
mod queue_replay;

#[path = "integration/queue_service.rs"]
mod queue_service;

#[path = "integration/queue_waiting_age.rs"]
mod queue_waiting_age;

#[path = "integration/review_draft.rs"]
mod review_draft;

#[path = "integration/review_flow.rs"]
mod review_flow;

#[path = "integration/review_gh.rs"]
mod review_gh;

#[path = "integration/review_git.rs"]
mod review_git;

#[path = "integration/review_locate.rs"]
mod review_locate;

#[path = "integration/rewind_replay.rs"]
mod rewind_replay;

#[path = "integration/rewind_service.rs"]
mod rewind_service;

#[path = "integration/saved_default_launch.rs"]
mod saved_default_launch;

#[path = "integration/session_flow.rs"]
mod session_flow;

#[path = "integration/session_picker_keys.rs"]
mod session_picker_keys;

#[path = "integration/settings_service.rs"]
mod settings_service;

#[path = "integration/side_replay.rs"]
mod side_replay;

#[path = "integration/side_service.rs"]
mod side_service;

#[path = "integration/status_effort.rs"]
mod status_effort;

#[path = "integration/subagent_replay.rs"]
mod subagent_replay;

#[path = "integration/subagent_wire.rs"]
mod subagent_wire;

#[path = "integration/switch_flow.rs"]
mod switch_flow;

#[path = "integration/theme_builtin_select.rs"]
mod theme_builtin_select;

#[path = "integration/theme_command.rs"]
mod theme_command;

#[path = "integration/theme_config.rs"]
mod theme_config;

#[path = "integration/theme_file_catalogue.rs"]
mod theme_file_catalogue;

#[path = "integration/theme_file_load.rs"]
mod theme_file_load;

#[path = "integration/theme_file_parse.rs"]
mod theme_file_parse;

#[path = "integration/tool_replay.rs"]
mod tool_replay;

#[path = "integration/ultracode_app.rs"]
mod ultracode_app;

#[path = "integration/vim_routing.rs"]
mod vim_routing;

#[path = "integration/voice_barge_in_flow.rs"]
mod voice_barge_in_flow;

#[path = "integration/voice_client_backend.rs"]
mod voice_client_backend;

#[path = "integration/voice_client_live.rs"]
mod voice_client_live;

#[path = "integration/voice_client_transport.rs"]
mod voice_client_transport;

#[path = "integration/voice_flow.rs"]
mod voice_flow;

#[path = "integration/voice_multi_backend.rs"]
mod voice_multi_backend;

#[path = "integration/voice_routing.rs"]
mod voice_routing;

#[path = "integration/wire_params.rs"]
mod wire_params;

#[path = "integration/workflow_progress_live.rs"]
mod workflow_progress_live;

#[path = "integration/workflow_progress_settlement.rs"]
mod workflow_progress_settlement;

#[path = "integration/workspace_service.rs"]
mod workspace_service;

#[path = "integration/worktree_git.rs"]
mod worktree_git;

#[path = "integration/worktree_guards.rs"]
mod worktree_guards;
