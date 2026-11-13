#[path = "concurrency_probe/mod.rs"]
mod concurrency_probe;
#[path = "config_probe/mod.rs"]
mod config_probe;
#[path = "support/fault_relay.rs"]
mod fault_relay;
#[path = "support/memory_mock.rs"]
mod memory_mock;
#[path = "support/mock_gated.rs"]
mod mock_gated;
#[path = "support/mock_kanban.rs"]
mod mock_kanban;
#[path = "remote_round_trip.rs"]
mod remote_round_trip;
#[path = "support/replay_mock.rs"]
mod replay_mock;
#[path = "support/mod.rs"]
mod support;
use support::mock_rest;

#[path = "integration/audio_relay.rs"]
mod audio_relay;
#[path = "integration/capabilities_api.rs"]
mod capabilities_api;
#[path = "integration/channel.rs"]
mod channel;
#[path = "integration/channel_props.rs"]
mod channel_props;
#[path = "integration/config_api.rs"]
mod config_api;
#[path = "integration/config_surface_matrix.rs"]
mod config_surface_matrix;
#[path = "integration/connect.rs"]
mod connect;
#[path = "integration/control_query.rs"]
mod control_query;
#[path = "integration/control_verbs.rs"]
mod control_verbs;
#[path = "integration/cron_api.rs"]
mod cron_api;
#[path = "integration/fixture_scan.rs"]
mod fixture_scan;
#[path = "integration/fixtures_replay.rs"]
mod fixtures_replay;
#[path = "integration/format_doc.rs"]
mod format_doc;
#[path = "integration/gateway_concurrency_results.rs"]
mod gateway_concurrency_results;
#[path = "integration/kanban_rest.rs"]
mod kanban_rest;
#[path = "integration/kanban_updates.rs"]
mod kanban_updates;
#[path = "integration/live_reconnect.rs"]
mod live_reconnect;
#[path = "integration/live_remote.rs"]
mod live_remote;
#[path = "integration/mcp_api.rs"]
mod mcp_api;
#[path = "integration/memory_api.rs"]
mod memory_api;
#[path = "integration/messaging_api.rs"]
mod messaging_api;
#[path = "integration/mock_rest_merge.rs"]
mod mock_rest_merge;
#[path = "integration/model_roles_rest.rs"]
mod model_roles_rest;
#[path = "integration/native_method.rs"]
mod native_method;
#[path = "integration/providers_api.rs"]
mod providers_api;
#[path = "integration/reconnect.rs"]
mod reconnect;
#[path = "integration/record_file.rs"]
mod record_file;
#[path = "integration/record_hook.rs"]
mod record_hook;
#[path = "integration/record_redaction_disk.rs"]
mod record_redaction_disk;
#[path = "integration/redaction_frames.rs"]
mod redaction_frames;
#[path = "integration/redaction_names.rs"]
mod redaction_names;
#[path = "integration/redaction_server_requests.rs"]
mod redaction_server_requests;
#[path = "integration/redaction_urls.rs"]
mod redaction_urls;
#[path = "integration/remote_auth.rs"]
mod remote_auth;
#[path = "integration/remote_connect.rs"]
mod remote_connect;
#[path = "integration/replay_source.rs"]
mod replay_source;
#[path = "integration/rest_client.rs"]
mod rest_client;
#[path = "integration/rest_fixtures.rs"]
mod rest_fixtures;
#[path = "integration/scan.rs"]
mod scan;
#[path = "integration/voice_api.rs"]
mod voice_api;
