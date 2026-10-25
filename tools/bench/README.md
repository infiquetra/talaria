# Performance harness

Build both tools and the client with:

    cargo build --release -p talaria-cli -p talaria-bench
    target/release/talaria-bench run --binary target/release/talaria --report bench-report.json

The runner generates its scenarios from recorded templates in fixtures/frames into a
temporary directory. To inspect them separately:

    target/release/talaria-bench generate --out generated-scenarios

scenarios/queue.jsonl is regenerated from the repository root with
`target/release/talaria-bench generate-queue --out tools/bench/scenarios/queue.jsonl`.

The child runs in a 160 by 48 pseudo-terminal with a cleared environment and an empty
temporary home. Its bytes go only to the terminal emulator and are never printed.
The responder answers startup queries before counting the first visible synchronized
frame. The reader stamps bytes on arrival; the main thread parses, answers queries,
writes scheduled keys, and detects their complete typed prefixes on frame boundaries.
Every child is killed and reaped before a measurement returns.

budgets.toml keeps the architecture limits unchanged. Memory uses decimal megabytes
(1,000,000 bytes). A long session has 1,000 assistant turns and 1,000 tool rows, hence
2,000 transcript cells. The two frame-byte limits are explicitly labelled unit choices.
Timing rows allow up to three attempts and pass when any attempt passes. Bytes, idle,
memory, and the external frame-count check use one measurement.

Options for run include --fixtures, --budgets, --report, --attempts, --idle-seconds,
--report-grace-seconds, and repeated --only selectors: launch, keystroke, cpu, bytes,
worst-frame, idle, memory. Repeat --binary-arg for each verbatim child argument,
including arguments starting with a dash. Repeated --extra-fixture paths and every
JSONL file in --scenario-dir (default tools/bench/scenarios) add labelled streaming
cases. Relative scenario names appear in reports; full paths never do.

The table and JSON report name the operating system, architecture, processor count,
safe binary version, every attempt, both correctness checks, and diagnostic counts.
GITHUB_STEP_SUMMARY receives the same table. Exit 0 means no failing row, 1 means a
measurement failed, and 2 means an input or process-launch precondition was missing.
The live-gateway composer budget remains SKIPPED. The measured view is the streaming
tail and composer; it excludes status and prompt cards.

fake-tui is a deterministic child for checking the harness. Startup/echo delay,
missing keys, late queries, missing markers/reports, early exits/reports, busy CPU,
memory ballast, large/growing frames, idle repainting, slow passes, and reported frame
count skew each expose a distinct failure. It uses raw input through stty and writes
replay and key frames under one output lock.

The separate bench workflow builds release binaries on pull requests and supports a
manual Linux or macOS run. It retains bench-report.json even on failure. Local debug
integration tests verify behavior; only a release measurement establishes budgets.

The parallel pre-build uses the published base history and live-section projections
and a local draft buffer in the replay entry point. It calls the real reducer,
commit ledger, and terminal interfaces. The full Markdown and composer widgets must
be integrated before interpreting its release measurements as finished-UI budgets.

Starts the real `talaria` binary in a pseudo-terminal, replays a recorded reply into it,
types while the reply streams, and compares the measurements with `budgets.toml`.

Run it with `target/release/talaria-bench run --binary target/release/talaria`.
