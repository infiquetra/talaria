# Performance harness

Starts the real `talaria` binary in a pseudo-terminal, replays a recorded reply into it,
types while the reply streams, and compares the measurements with `budgets.toml`.

Run it with `target/release/talaria-bench run --binary target/release/talaria`.
