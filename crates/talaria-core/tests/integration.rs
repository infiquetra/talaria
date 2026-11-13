#[path = "support/bake.rs"]
mod bake;
#[path = "support/catalogue.rs"]
mod catalogue;
#[path = "support/configure_ctx.rs"]
mod configure_ctx;
#[path = "connectors_common/mod.rs"]
mod connectors_common;
#[path = "cron_support/mod.rs"]
mod cron_support;
#[path = "support/drive.rs"]
mod drive;
#[path = "support/events.rs"]
mod events;
#[path = "support/frames.rs"]
mod frames;
#[path = "support/model_drive.rs"]
mod model_drive;
#[path = "support/providers.rs"]
mod providers_support;
#[path = "support/snapshot_json.rs"]
mod snapshot_json;

#[path = "integration/acp_fleet_isolation.rs"]
mod acp_fleet_isolation;
#[path = "integration/acp_mapping.rs"]
mod acp_mapping;
#[path = "integration/alert_gate.rs"]
mod alert_gate;
#[path = "integration/appearance.rs"]
mod appearance;
#[path = "integration/approval_mode.rs"]
mod approval_mode;
#[path = "integration/approval_segment.rs"]
mod approval_segment;
#[path = "integration/attention.rs"]
mod attention;
#[path = "integration/automation_fold.rs"]
mod automation_fold;
#[path = "integration/automation_lines.rs"]
mod automation_lines;
#[path = "integration/automation_present.rs"]
mod automation_present;
#[path = "integration/automation_reduce.rs"]
mod automation_reduce;
#[path = "integration/automation_snapshot.rs"]
mod automation_snapshot;
#[path = "integration/background_bg.rs"]
mod background_bg;
#[path = "integration/background_clock.rs"]
mod background_clock;
#[path = "integration/background_model.rs"]
mod background_model;
#[path = "integration/background_output.rs"]
mod background_output;
#[path = "integration/background_process.rs"]
mod background_process;
#[path = "integration/background_schedule.rs"]
mod background_schedule;
#[path = "integration/capabilities_props.rs"]
mod capabilities_props;
#[path = "integration/capabilities_rows.rs"]
mod capabilities_rows;
#[path = "integration/capabilities_state.rs"]
mod capabilities_state;
#[path = "integration/capabilities_text.rs"]
mod capabilities_text;
#[path = "integration/cd_state.rs"]
mod cd_state;
#[path = "integration/cd_target.rs"]
mod cd_target;
#[path = "integration/checklist_model.rs"]
mod checklist_model;
#[path = "integration/checklist_snapshot.rs"]
mod checklist_snapshot;
#[path = "integration/checklist_tracker.rs"]
mod checklist_tracker;
#[path = "integration/commands_catalog.rs"]
mod commands_catalog;
#[path = "integration/commands_local.rs"]
mod commands_local;
#[path = "integration/commands_local_agents.rs"]
mod commands_local_agents;
#[path = "integration/commands_local_layout.rs"]
mod commands_local_layout;
#[path = "integration/commands_outcome.rs"]
mod commands_outcome;
#[path = "integration/commands_palette.rs"]
mod commands_palette;
#[path = "integration/commands_parse.rs"]
mod commands_parse;
#[path = "integration/commands_state.rs"]
mod commands_state;
#[path = "integration/composer_chips.rs"]
mod composer_chips;
#[path = "integration/configure_brief.rs"]
mod configure_brief;
#[path = "integration/configure_props.rs"]
mod configure_props;
#[path = "integration/configure_reply.rs"]
mod configure_reply;
#[path = "integration/configure_state.rs"]
mod configure_state;
#[path = "integration/configure_validate.rs"]
mod configure_validate;
#[path = "integration/connectors_accounts.rs"]
mod connectors_accounts;
#[path = "integration/connectors_link.rs"]
mod connectors_link;
#[path = "integration/connectors_props.rs"]
mod connectors_props;
#[path = "integration/connectors_rows.rs"]
mod connectors_rows;
#[path = "integration/connectors_rules.rs"]
mod connectors_rules;
#[path = "integration/context_breakdown.rs"]
mod context_breakdown;
#[path = "integration/context_compact.rs"]
mod context_compact;
#[path = "integration/context_tracker.rs"]
mod context_tracker;
#[path = "integration/context_usage.rs"]
mod context_usage;
#[path = "integration/cron_form.rs"]
mod cron_form;
#[path = "integration/cron_history.rs"]
mod cron_history;
#[path = "integration/cron_list.rs"]
mod cron_list;
#[path = "integration/cron_props.rs"]
mod cron_props;
#[path = "integration/cron_run_now.rs"]
mod cron_run_now;
#[path = "integration/cron_time.rs"]
mod cron_time;
#[path = "integration/diff_parse.rs"]
mod diff_parse;
#[path = "integration/directory_export.rs"]
mod directory_export;
#[path = "integration/directory_rows.rs"]
mod directory_rows;
#[path = "integration/directory_title.rs"]
mod directory_title;
#[path = "integration/doctor_diagnose.rs"]
mod doctor_diagnose;
#[path = "integration/doctor_restart.rs"]
mod doctor_restart;
#[path = "integration/drop_path.rs"]
mod drop_path;
#[path = "integration/fleet_attention.rs"]
mod fleet_attention;
#[path = "integration/fleet_external_seam.rs"]
mod fleet_external_seam;
#[path = "integration/fleet_queue.rs"]
mod fleet_queue;
#[path = "integration/fleet_registry.rs"]
mod fleet_registry;
#[path = "integration/fleet_rows.rs"]
mod fleet_rows;
#[path = "integration/fleet_subscribed.rs"]
mod fleet_subscribed;
#[path = "integration/fleet_view_keys.rs"]
mod fleet_view_keys;
#[path = "integration/fleet_view_pane.rs"]
mod fleet_view_pane;
#[path = "integration/fleet_view_rows.rs"]
mod fleet_view_rows;
#[path = "integration/herdr_mapping.rs"]
mod herdr_mapping;
#[path = "integration/kanban_state.rs"]
mod kanban_state;
#[path = "integration/layout_plan.rs"]
mod layout_plan;
#[path = "integration/live_reduce.rs"]
mod live_reduce;
#[path = "integration/mcp_apply.rs"]
mod mcp_apply;
#[path = "integration/mcp_catalogue.rs"]
mod mcp_catalogue;
#[path = "integration/mcp_form.rs"]
mod mcp_form;
#[path = "integration/mcp_servers.rs"]
mod mcp_servers;
#[path = "integration/mcp_signin.rs"]
mod mcp_signin;
#[path = "integration/memory_command.rs"]
mod memory_command;
#[path = "integration/memory_fields.rs"]
mod memory_fields;
#[path = "integration/memory_state.rs"]
mod memory_state;
#[path = "integration/memory_view.rs"]
mod memory_view;
#[path = "integration/mention_query.rs"]
mod mention_query;
#[path = "integration/messaging_edit.rs"]
mod messaging_edit;
#[path = "integration/messaging_pairing.rs"]
mod messaging_pairing;
#[path = "integration/messaging_rows.rs"]
mod messaging_rows;
#[path = "integration/model_browse.rs"]
mod model_browse;
#[path = "integration/model_catalog.rs"]
mod model_catalog;
#[path = "integration/model_effort.rs"]
mod model_effort;
#[path = "integration/model_pick.rs"]
mod model_pick;
#[path = "integration/model_picker.rs"]
mod model_picker;
#[path = "integration/model_return.rs"]
mod model_return;
#[path = "integration/model_roles_fallbacks.rs"]
mod model_roles_fallbacks;
#[path = "integration/model_roles_helpers.rs"]
mod model_roles_helpers;
#[path = "integration/model_roles_moa.rs"]
mod model_roles_moa;
#[path = "integration/model_roles_state.rs"]
mod model_roles_state;
#[path = "integration/model_roles_subagents.rs"]
mod model_roles_subagents;
#[path = "integration/model_saved.rs"]
mod model_saved;
#[path = "integration/mutation_fences.rs"]
mod mutation_fences;
#[path = "integration/permissions_command.rs"]
mod permissions_command;
#[path = "integration/permissions_profile.rs"]
mod permissions_profile;
#[path = "integration/permissions_state.rs"]
mod permissions_state;
#[path = "integration/picker_state.rs"]
mod picker_state;
#[path = "integration/profile_names.rs"]
mod profile_names;
#[path = "integration/profile_roster.rs"]
mod profile_roster;
#[path = "integration/profiles_state.rs"]
mod profiles_state;
#[path = "integration/providers_browse.rs"]
mod providers_browse;
#[path = "integration/providers_endpoints.rs"]
mod providers_endpoints;
#[path = "integration/providers_keys.rs"]
mod providers_keys;
#[path = "integration/providers_props.rs"]
mod providers_props;
#[path = "integration/providers_signin.rs"]
mod providers_signin;
#[path = "integration/providers_steps.rs"]
mod providers_steps;
#[path = "integration/queue_intents.rs"]
mod queue_intents;
#[path = "integration/queue_reorder.rs"]
mod queue_reorder;
#[path = "integration/queue_state.rs"]
mod queue_state;
#[path = "integration/review_parse.rs"]
mod review_parse;
#[path = "integration/review_pr.rs"]
mod review_pr;
#[path = "integration/review_props.rs"]
mod review_props;
#[path = "integration/review_ship.rs"]
mod review_ship;
#[path = "integration/review_state.rs"]
mod review_state;
#[path = "integration/rewind_checkpoints.rs"]
mod rewind_checkpoints;
#[path = "integration/rewind_command.rs"]
mod rewind_command;
#[path = "integration/rewind_state.rs"]
mod rewind_state;
#[path = "integration/rewind_state_scenarios.rs"]
mod rewind_state_scenarios;
#[path = "integration/rewind_text.rs"]
mod rewind_text;
#[path = "integration/rewind_turns.rs"]
mod rewind_turns;
#[path = "integration/seat_status.rs"]
mod seat_status;
#[path = "integration/session_cost.rs"]
mod session_cost;
#[path = "integration/session_history_truncated.rs"]
mod session_history_truncated;
#[path = "integration/session_properties.rs"]
mod session_properties;
#[path = "integration/session_queue.rs"]
mod session_queue;
#[path = "integration/session_replay.rs"]
mod session_replay;
#[path = "integration/session_rules.rs"]
mod session_rules;
#[path = "integration/session_snapshot.rs"]
mod session_snapshot;
#[path = "integration/settings_command.rs"]
mod settings_command;
#[path = "integration/settings_diff.rs"]
mod settings_diff;
#[path = "integration/settings_effects.rs"]
mod settings_effects;
#[path = "integration/settings_filter.rs"]
mod settings_filter;
#[path = "integration/settings_readback.rs"]
mod settings_readback;
#[path = "integration/settings_schema.rs"]
mod settings_schema;
#[path = "integration/settings_secrets.rs"]
mod settings_secrets;
#[path = "integration/settings_state.rs"]
mod settings_state;
#[path = "integration/settings_values.rs"]
mod settings_values;
#[path = "integration/shared_text_seam.rs"]
mod shared_text_seam;
#[path = "integration/side_classify.rs"]
mod side_classify;
#[path = "integration/side_reduce.rs"]
mod side_reduce;
#[path = "integration/side_text.rs"]
mod side_text;
#[path = "integration/speakable_props.rs"]
mod speakable_props;
#[path = "integration/speakable_summary.rs"]
mod speakable_summary;
#[path = "integration/subagent_batch.rs"]
mod subagent_batch;
#[path = "integration/subagent_tree.rs"]
mod subagent_tree;
#[path = "integration/tool_generating.rs"]
mod tool_generating;
#[path = "integration/tool_view.rs"]
mod tool_view;
#[path = "integration/ultracode_notices.rs"]
mod ultracode_notices;
#[path = "integration/ultracode_route.rs"]
mod ultracode_route;
#[path = "integration/voice_barge_in.rs"]
mod voice_barge_in;
#[path = "integration/voice_client_core.rs"]
mod voice_client_core;
#[path = "integration/voice_client_state.rs"]
mod voice_client_state;
#[path = "integration/voice_commands.rs"]
mod voice_commands;
#[path = "integration/voice_target.rs"]
mod voice_target;
