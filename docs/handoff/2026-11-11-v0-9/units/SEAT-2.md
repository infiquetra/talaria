# SEAT-2: Probe, parse and write status records in the terminal module

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the probe code that exists today and for why a status answer must not be handed to the key parser. The requirements, the tests and the acceptance checks are only in the issue. This page points at them by id. The byte strings are REQ-5. They are not repeated here.

## What exists today

`PROBE_QUERY` (`crates/talaria-tui/src/terminal.rs:80`) is six queries in one write: cursor position, keyboard flags, synchronized output, Operating System Command 11, Operating System Command 10, and device attributes (`ESC [ c`). The comment above it says the device-attributes reply marks the end, so a terminal that skips a colour query costs no extra wait. REQ-1 inserts the 7501 probe immediately before that end marker and updates the comment to seven queries.

`parse_probe_reply` (`terminal.rs:167`) walks the bytes. An Operating System Command is sliced by `osc_reply` (`terminal.rs:149`), which already accepts BEL and ST (`ESC \`). Only colour indexes 10 and 11 are stored (`terminal.rs:177-180`). Any other index is ignored. The function returns as soon as it sees a device-attributes final byte `c` (`terminal.rs:213`). That is why REQ-2 stores a 7501 answer only when it ends before that marker, and why test 4 expects `status` to stay `None` when the answer is after the marker. Do not move the return. A colour answer after the marker is already specified the same way by `probe_asks_for_colours_before_device_attributes` in `crates/talaria-tui/tests/theme_probe.rs`.

`split_probe_input` (`terminal.rs:261`) copies recognized replies into one buffer and everything else into the input buffer. It already drops an unfinished Operating System Command 10 or 11 instead of returning those bytes as input (`terminal.rs:280-288`). A complete colour reply is recognized only for indexes 10 and 11 (`terminal.rs:309-316`). A 7501 reply today falls through to the input side. REQ-3 adds 7501 beside those two indexes. An unfinished 7501 at the end of the buffer uses the same `break` as an unfinished colour reply.

`read_probe_reply_with_input` (`terminal.rs:334`) reads one byte at a time and stops when the device-attributes marker completes (`terminal.rs:358-366`). Bytes after that marker stay in the descriptor. `ordinary_input_survives_probe_replies_and_late_replies` in `crates/talaria-tui/tests/probe.rs` writes `world` after the marker and asserts those five bytes are still in the pipe. REQ-3 does not read past the marker, so that test stays as it is. When no marker arrives, the function drains for a short quiet period (`terminal.rs:369-381`) and then splits. A late 7501 with no marker is inside that drained buffer. Test 5 asserts it is not returned as input and is not left in the pipe. The 50 millisecond deadline is the one that test file already passes to the same function.

`ProbeReply` (`terminal.rs:69`) is `Copy`. REQ-4 adds `status: Option<StatusSupport>` with a `Copy` support value so the derive stays. `Default` is `None`. Literals that end with `..Default::default()` compile unchanged. Three literals in `crates/talaria-tui/tests/release.rs` (the closures at lines 73, 107 and 138) name `colors` and do not use `..`. Each gains `status: None` and no other edit.

`InlineTerminal::new` (`terminal.rs:732`) keeps `probe.caps` and `probe.cursor`. It does not keep a slot for a later probe answer. REQ-6 copies `probe.status` there. `reacquire` (`terminal.rs:1996`) already replaces the cursor from the new reply (`terminal.rs:2017-2019`). REQ-6 replaces the stored support in that same function. While `released` is set, `report_seat` writes nothing and remembers the state. After `reacquire` clears `released`, it writes the record for the remembered state, or else the last state written, whether or not it changed. If no state was ever reported, it writes no record. It does not clear. Clearing stays on restore, which is REQ-7.

`InlineTerminal::restore` (`terminal.rs:1187`) claims the shared `RestoreState` and writes `restore_sequence` (`terminal.rs:520`) once. `RestoreLink::restore_into` (`terminal.rs:582`) is the same claim. `restore_on_panic` (`terminal.rs:593`) uses that claim when it can lock, and `minimal_restore` (`terminal.rs:550`) when the mutex is already held. The flag cannot live only inside the mutex, or the panic arm that cannot lock would not know a record was written. REQ-7 puts an `AtomicBool` beside the mutex in the same `Arc`.

`probe_reacquire` (`terminal.rs:2112`) flushes stdin and calls `probe_startup`. It needs no separate edit once `PROBE_QUERY` changes.

`crates/talaria-tui/tests/escape_custody.rs::only_terminal_module_writes_escape_sequences` scans `src/` and fails if an escape literal is outside `terminal.rs`. The scan root is the crate's `src` directory, not `examples/`. REQ-8 still forbids an escape literal in the example, so the bytes have one definition.

The disallowed terminal calls are the `disallowed-methods` list in `clippy.toml`. This issue does not call them. The example uses `std::io` and the functions REQ-5 and REQ-6 add.

No Rust source contains the text `7501` as this escape. Recorded JSON uses the digits inside timestamps. Do not treat those as a probe.

## What the example is for

`seat_probe` is not Talaria. It does not open a session and it does not draw a viewport. It writes records to stdout and the `seat-probe` lines to stderr, then sleeps 2 seconds, which is long enough to run `hc get --json` once. The stderr word is the word in the bytes just written. The clear record is written once after the walk. If the probe answer is missing, it writes nothing and exits 0. Redirected stdout is not a terminal, so the example returns before `probe_startup` and the capture file stays free of `]7501`. That is UAT-2.

## Tests

Tests 1 and 5 stay in `crates/talaria-tui/tests/probe.rs` so they compile before `ProbeReply.status` exists. Tests 2 to 4 are `crates/talaria-tui/tests/status_probe.rs` (new) and fail to compile until that field exists. Tests 6 to 9 are `crates/talaria-tui/tests/status_record.rs` (new). They read bytes through a local `ByteSink` of the same shape as `status_inline.rs`, because `InlineTerminal`'s writer field is private. `status_written` is a new `AtomicBool` beside the restore mutex. `restore_sequence` and `minimal_restore` take the loaded bool. The answer text the tests copy is settled decision 2 in the issue. There is no fixture file. `release` writes no clear record. While released, `report_seat` writes nothing and remembers the state. `reacquire` writes the remembered record, or else the last one written, and test 9 asserts that those bytes contain that record. `InlineTerminal::new` writes setup bytes into the same sink, so tests 8 and 9 drain that sink before the first `report_seat`. `restore` writes the clear record as the start of the existing restore sequence, so test 9 asserts that prefix.
