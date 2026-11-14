//! Terminal integration suites with shared test-free helpers.

#[path = "support/color.rs"]
mod color;
#[path = "composer_support/mod.rs"]
mod composer_support;
#[path = "../../talaria-core/tests/support/events.rs"]
mod events;
#[path = "markdown_support/mod.rs"]
mod markdown_support;
mod pager_support;
#[cfg(unix)]
#[path = "status_support/mod.rs"]
mod status_support;
mod support;
#[path = "support/view_colours.rs"]
mod view_colours_support;

#[path = "integration/acp_fleet_render.rs"]
mod acp_fleet_render;
#[path = "integration/alert_bytes.rs"]
mod alert_bytes;
#[path = "integration/attention_keys.rs"]
mod attention_keys;
#[path = "integration/automation_render.rs"]
mod automation_render;
#[path = "integration/background_view.rs"]
mod background_view;
#[path = "integration/capabilities_keys.rs"]
mod capabilities_keys;
#[path = "integration/capabilities_render.rs"]
mod capabilities_render;
#[path = "integration/checklist_keys.rs"]
mod checklist_keys;
#[path = "integration/checklist_render.rs"]
mod checklist_render;
#[path = "integration/checklist_replay.rs"]
mod checklist_replay;
#[path = "integration/comfort_keys.rs"]
mod comfort_keys;
#[path = "integration/comforts_pty.rs"]
mod comforts_pty;
#[path = "integration/composer_custody.rs"]
mod composer_custody;
#[path = "integration/composer_keys.rs"]
mod composer_keys;
#[path = "integration/composer_paste.rs"]
mod composer_paste;
#[path = "integration/composer_pty.rs"]
mod composer_pty;
#[path = "integration/composer_render.rs"]
mod composer_render;
#[path = "integration/composer_vim.rs"]
mod composer_vim;
#[path = "integration/composer_vim_pty.rs"]
mod composer_vim_pty;
#[path = "integration/composer_vim_render.rs"]
mod composer_vim_render;
#[path = "integration/configure_render.rs"]
mod configure_render;
#[path = "integration/connectors_render.rs"]
mod connectors_render;
#[path = "integration/context_render.rs"]
mod context_render;
#[path = "integration/cron_render.rs"]
mod cron_render;
#[path = "integration/doctor_view.rs"]
mod doctor_view;
#[path = "integration/fleet_alert_bytes.rs"]
mod fleet_alert_bytes;
#[path = "integration/fleet_external_launch.rs"]
mod fleet_external_launch;
#[path = "integration/fleet_pane_screens.rs"]
mod fleet_pane_screens;
#[path = "integration/fleet_pr_column.rs"]
mod fleet_pr_column;
#[path = "integration/fleet_view_screens.rs"]
mod fleet_view_screens;
#[path = "integration/herdr_peek.rs"]
mod herdr_peek;
#[path = "integration/history_search.rs"]
mod history_search;
#[path = "integration/history_text.rs"]
mod history_text;
#[path = "integration/inline_setup.rs"]
mod inline_setup;
#[path = "integration/kanban_view.rs"]
mod kanban_view;
#[path = "integration/keymap_doc.rs"]
mod keymap_doc;
#[path = "integration/keys_approvals.rs"]
mod keys_approvals;
#[path = "integration/keys_completion.rs"]
mod keys_completion;
#[path = "integration/launch_dialog.rs"]
mod launch_dialog;
#[path = "integration/layout.rs"]
mod layout;
#[path = "integration/live_checklist.rs"]
mod live_checklist;
#[path = "integration/live_region.rs"]
mod live_region;
#[path = "integration/markdown_adapters.rs"]
mod markdown_adapters;
#[path = "integration/markdown_highlight.rs"]
mod markdown_highlight;
#[path = "integration/markdown_replay.rs"]
mod markdown_replay;
#[path = "integration/markdown_sanitize.rs"]
mod markdown_sanitize;
#[path = "integration/markdown_vt100.rs"]
mod markdown_vt100;
#[path = "integration/mcp_props.rs"]
mod mcp_props;
#[path = "integration/mcp_render.rs"]
mod mcp_render;
#[path = "integration/memory_render.rs"]
mod memory_render;
#[path = "integration/mention_popup.rs"]
mod mention_popup;
#[path = "integration/menu_actions.rs"]
mod menu_actions;
#[path = "integration/menu_bar.rs"]
mod menu_bar;
#[path = "integration/menu_keys.rs"]
mod menu_keys;
#[path = "integration/model_keys.rs"]
mod model_keys;
#[path = "integration/model_picker_render.rs"]
mod model_picker_render;
#[path = "integration/model_roles_render.rs"]
mod model_roles_render;
#[path = "integration/no_blink.rs"]
mod no_blink;
#[path = "integration/overlay.rs"]
mod overlay;
#[path = "integration/overlay_cursor.rs"]
mod overlay_cursor;
#[path = "integration/pager_nav.rs"]
mod pager_nav;
#[path = "integration/pager_prop.rs"]
mod pager_prop;
#[path = "integration/pager_render.rs"]
mod pager_render;
#[path = "integration/pager_replay.rs"]
mod pager_replay;
#[path = "integration/pager_search.rs"]
mod pager_search;
#[path = "integration/palette_render.rs"]
mod palette_render;
#[path = "integration/picker_frame_render.rs"]
mod picker_frame_render;
#[path = "integration/picker_frame_states.rs"]
mod picker_frame_states;
#[path = "integration/probe.rs"]
mod probe;
#[path = "integration/profiles_keys.rs"]
mod profiles_keys;
#[path = "integration/profiles_render.rs"]
mod profiles_render;
#[path = "integration/prompt_cards.rs"]
mod prompt_cards;
#[path = "integration/providers_details_key.rs"]
mod providers_details_key;
#[path = "integration/providers_frame_render.rs"]
mod providers_frame_render;
#[path = "integration/providers_input.rs"]
mod providers_input;
#[path = "integration/providers_render.rs"]
mod providers_render;
#[path = "integration/queue_keys.rs"]
mod queue_keys;
#[path = "integration/queue_preview.rs"]
mod queue_preview;
#[path = "integration/reasoning_label.rs"]
mod reasoning_label;
#[path = "integration/replay_commit.rs"]
mod replay_commit;
#[path = "integration/resize_anchor.rs"]
mod resize_anchor;
#[path = "integration/review_render.rs"]
mod review_render;
#[path = "integration/rewind_render.rs"]
mod rewind_render;
#[path = "integration/scrollback.rs"]
mod scrollback;
#[path = "integration/session_picker_draw.rs"]
mod session_picker_draw;
#[path = "integration/settings_keys.rs"]
mod settings_keys;
#[path = "integration/settings_render.rs"]
mod settings_render;
#[path = "integration/shared_text_seam.rs"]
mod shared_text_seam;
#[path = "integration/side_render.rs"]
mod side_render;
#[path = "integration/status_approvals.rs"]
mod status_approvals;
#[path = "integration/status_command.rs"]
mod status_command;
#[path = "integration/status_contract.rs"]
mod status_contract;
#[path = "integration/status_descriptors.rs"]
mod status_descriptors;
#[path = "integration/status_fleet_segments.rs"]
mod status_fleet_segments;
#[path = "integration/status_inline.rs"]
mod status_inline;
#[path = "integration/status_line.rs"]
mod status_line;
#[path = "integration/status_pr_segment.rs"]
mod status_pr_segment;
#[path = "integration/status_probe.rs"]
mod status_probe;
#[path = "integration/status_record.rs"]
mod status_record;
#[path = "integration/subagent_view.rs"]
mod subagent_view;
#[path = "integration/theme_builtin.rs"]
mod theme_builtin;
#[path = "integration/theme_color.rs"]
mod theme_color;
#[path = "integration/theme_detect.rs"]
mod theme_detect;
#[path = "integration/theme_file_contrast.rs"]
mod theme_file_contrast;
#[path = "integration/theme_file_depth.rs"]
mod theme_file_depth;
#[path = "integration/theme_file_resolve.rs"]
mod theme_file_resolve;
#[path = "integration/theme_file_surfaces.rs"]
mod theme_file_surfaces;
#[path = "integration/theme_probe.rs"]
mod theme_probe;
#[path = "integration/theme_resolve.rs"]
mod theme_resolve;
#[path = "integration/theme_screens.rs"]
mod theme_screens;
#[path = "integration/theme_snapshots.rs"]
mod theme_snapshots;
#[path = "integration/tmux_scrollback.rs"]
mod tmux_scrollback;
#[path = "integration/tool_cells.rs"]
mod tool_cells;
#[path = "integration/ultracode_status.rs"]
mod ultracode_status;
#[path = "integration/view_chrome.rs"]
mod view_chrome;
#[path = "integration/view_colours_default.rs"]
mod view_colours_default;
#[path = "integration/view_colours_off.rs"]
mod view_colours_off;
#[path = "integration/view_colours_themed.rs"]
mod view_colours_themed;
#[path = "integration/voice_view.rs"]
mod voice_view;
#[path = "integration/work_panel_content.rs"]
mod work_panel_content;
#[path = "integration/workflow_view.rs"]
mod workflow_view;
#[path = "integration/workspace_keys.rs"]
mod workspace_keys;
#[path = "integration/workspace_render.rs"]
mod workspace_render;
#[path = "integration/workspace_terminal.rs"]
mod workspace_terminal;
#[path = "integration/workspace_tmux.rs"]
mod workspace_tmux;
