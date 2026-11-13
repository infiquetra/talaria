#[cfg(feature = "mock")]
#[path = "builtins_common/mod.rs"]
mod builtins_common;
#[cfg(feature = "mock")]
#[path = "common/mod.rs"]
mod common;
#[path = "hermes_common/mod.rs"]
mod hermes_common;
#[path = "support/lines.rs"]
mod lines;
#[path = "../../hermes-client/tests/support/mock_kanban.rs"]
mod mock_kanban;
#[path = "support/replay_run.rs"]
mod replay_run;
#[path = "runner_support/mod.rs"]
mod runner_support;
#[path = "scheduler_support/mod.rs"]
mod scheduler_support;
#[path = "staffing_support/mod.rs"]
mod staffing_support;
#[cfg(feature = "fake-gateway")]
#[path = "support/mod.rs"]
mod support;

#[path = "integration/accounting.rs"]
mod accounting;
#[path = "integration/agent_call.rs"]
mod agent_call;
#[path = "integration/budget.rs"]
mod budget;
#[path = "integration/builtins_fix_failing_tests.rs"]
mod builtins_fix_failing_tests;
#[path = "integration/builtins_install.rs"]
mod builtins_install;
#[path = "integration/builtins_invariants.rs"]
mod builtins_invariants;
#[path = "integration/builtins_research.rs"]
mod builtins_research;
#[path = "integration/builtins_resume.rs"]
mod builtins_resume;
#[path = "integration/builtins_review_branch.rs"]
mod builtins_review_branch;
#[path = "integration/builtins_scripts.rs"]
mod builtins_scripts;
#[path = "integration/builtins_ultracode.rs"]
mod builtins_ultracode;
#[path = "integration/call_id.rs"]
mod call_id;
#[path = "integration/check.rs"]
mod check;
#[path = "integration/combinators.rs"]
mod combinators;
#[path = "integration/crate_boundaries.rs"]
mod crate_boundaries;
#[path = "integration/dry_run.rs"]
mod dry_run;
#[path = "integration/estimate.rs"]
mod estimate;
#[path = "integration/events.rs"]
mod events;
#[path = "integration/fake_gateway_rules.rs"]
mod fake_gateway_rules;
#[path = "integration/helpers.rs"]
mod helpers;
#[path = "integration/hermes_backend.rs"]
mod hermes_backend;
#[path = "integration/hermes_binding_smoke.rs"]
mod hermes_binding_smoke;
#[path = "integration/hermes_frames.rs"]
mod hermes_frames;
#[path = "integration/hermes_live.rs"]
mod hermes_live;
#[path = "integration/hermes_policy.rs"]
mod hermes_policy;
#[path = "integration/hermes_port.rs"]
mod hermes_port;
#[path = "integration/hermes_validate.rs"]
mod hermes_validate;
#[path = "integration/journal_bench.rs"]
mod journal_bench;
#[path = "integration/journal_blobs.rs"]
mod journal_blobs;
#[path = "integration/journal_crash_points.rs"]
mod journal_crash_points;
#[path = "integration/journal_keys.rs"]
mod journal_keys;
#[path = "integration/journal_live.rs"]
mod journal_live;
#[path = "integration/journal_meta.rs"]
mod journal_meta;
#[path = "integration/journal_privacy.rs"]
mod journal_privacy;
#[path = "integration/journal_reader.rs"]
mod journal_reader;
#[path = "integration/journal_records.rs"]
mod journal_records;
#[path = "integration/journal_replay.rs"]
mod journal_replay;
#[path = "integration/journal_restart.rs"]
mod journal_restart;
#[path = "integration/journal_resume.rs"]
mod journal_resume;
#[path = "integration/journal_retry.rs"]
mod journal_retry;
#[path = "integration/journal_store.rs"]
mod journal_store;
#[path = "integration/journal_unknown.rs"]
mod journal_unknown;
#[path = "integration/journal_wire.rs"]
mod journal_wire;
#[path = "integration/kanban_compile.rs"]
mod kanban_compile;
#[path = "integration/launch.rs"]
mod launch;
#[path = "integration/live_ultracode.rs"]
mod live_ultracode;
#[path = "integration/meta_parse.rs"]
mod meta_parse;
#[path = "integration/mock.rs"]
mod mock;
#[path = "integration/nesting.rs"]
mod nesting;
#[path = "integration/outcome.rs"]
mod outcome;
#[path = "integration/price.rs"]
mod price;
#[path = "integration/privacy_scan.rs"]
mod privacy_scan;
#[path = "integration/progress_control.rs"]
mod progress_control;
#[path = "integration/progress_live.rs"]
mod progress_live;
#[path = "integration/progress_races.rs"]
mod progress_races;
#[path = "integration/progress_tree.rs"]
mod progress_tree;
#[path = "integration/replay.rs"]
mod replay;
#[path = "integration/report.rs"]
mod report;
#[path = "integration/roster.rs"]
mod roster;
#[path = "integration/roster_envelope.rs"]
mod roster_envelope;
#[path = "integration/roster_live.rs"]
mod roster_live;
#[path = "integration/route.rs"]
mod route;
#[path = "integration/run_dir.rs"]
mod run_dir;
#[path = "integration/runner_execute.rs"]
mod runner_execute;
#[path = "integration/runner_resolver.rs"]
mod runner_resolver;
#[path = "integration/runner_runs.rs"]
mod runner_runs;
#[path = "integration/sandbox.rs"]
mod sandbox;
#[path = "integration/scheduler.rs"]
mod scheduler;
#[path = "integration/scheduler_composed.rs"]
mod scheduler_composed;
#[path = "integration/skill_files.rs"]
mod skill_files;
#[path = "integration/skill_install.rs"]
mod skill_install;
#[path = "integration/source.rs"]
mod source;
#[path = "integration/staffing_table.rs"]
mod staffing_table;
#[path = "integration/worktree_isolation.rs"]
mod worktree_isolation;
#[path = "integration/worktree_recovery.rs"]
mod worktree_recovery;
