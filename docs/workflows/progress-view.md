# Workflow progress

Open `/workflows <run-folder>` to inspect a workflow journal. The view uses the
manifest and complete journal lines before subscribing to running sessions.
Closing it with Escape leaves the run running and retains the selection for
reopening; reopening unfolds every row and clears any message or confirmation.
The view opens with the first agent selected. The open view polls the journal every 250 ms, including while disconnected.
Gateway subscriptions, agent controls and opening a session use the manifest's profile, even when Talaria was launched with a different profile.
Run discovery and the picker become available with the runner integration.

The tree groups journal keys under their workflow and phase, with child
workflows and Hermes subagents nested below their parent. Subagents do not
increase the run's agent count. Rows show the journal's model, provider,
duration and usage; token counts, here and in the run list, read in thousands and
millions with one decimal (`950 tokens`, `412.3k tokens`, `5.8M tokens`), while JSON
output keeps the raw numbers; selected agents show their prompt, last tool and recent
output. Filters cycle through all, working, needs-you and failed. The header
names any filter other than all, and an empty list says which filter hides the
agents and that `f` changes it.

Agent states are done, failed, skipped, unknown (unresolved journal attention),
ended (engine ended without a journal outcome), waiting (an outstanding
approval or clarification), stopping, running and queued. A recorded outcome
wins over later live activity. A running agent's row ends its state with
`step N`, from Hermes's `agent.step` event. An agent Hermes stopped at its step cap keeps
its reply and its done state, and its row says `cut short (60 of 60 steps)`; the header
counts them as `cut short`. Run states are running, completed, completed with failures
(the script returned, but at least one agent failed; each failed agent and its reason is
listed above the rows), budget exhausted, failed and aborted. Agents the budget stopped count as `stopped` in the
header instead of failed, calls it refused count as `refused`, and each refusal
adds an `agent <label> refused: <reason>` line to the log. A resumed execution keeps the previous outcome separately. The view
checks the run lock on every poll. When no runner holds it and the journal has
been quiet for ten seconds (a minute before the first record, while a new runner
starts), a running view adds
`runner not running — talaria workflow resume <run-id> continues it`. A lock the
view cannot read makes no claim either way. The runner journals its admission
gate only when the gate opens or pauses, so script logs stay visible.

Use Up and Down to select rows, Left to fold the selected phase or agent and
Right to unfold it (the run header never folds); the key-hint row names
`← fold · → unfold`. Use Enter to open an agent, `s` to
steer, `x` to stop an agent, `r` to request restart on the next resume, `p` to
pause or resume, `shift+x` to stop the run, and `f` to cycle filters. Restart
and stop-run require a second matching key or `y` within three seconds. The
`workflow_*` entries in `[keys]` remap these defaults.

Agent controls use Hermes's canonical session calls. Run controls publish
create-new request files under `control/`; a request is not proof that a
runner acted. Pause needs journal evidence; stop finishes only when the
journal records the outcome. A warning after ten seconds says when no runner
answered. Restart remains pending for the next resume. Completed and skipped
work cannot be restarted from this view. Messages in the key-hint row last
five seconds, then the hints return.

Gateway events without a session, or for a session the run does not own, never
reach the view. Event types the view does not draw are ignored; a type this
Talaria cannot decode is recorded once per connection in
`~/.talaria/logs/client.log` and never shown.

The compact request files are `pause.json` and `stop.json`, containing
`{"v":1,"kind":"pause","at_ms":123}` (or `"stop"`), and
`restart-<key>.json`, containing
`{"v":1,"kind":"restart","key":"<32 lower-case hex>","at_ms":123}`.
There is no trailing newline. Resume removes the held pause file. Restart
files remain until the runner's next resume; the viewer never starts a run.

Unknown costs display `cost unknown`. Settled admission usage and cumulative
live usage share no additive scope. A damaged journal opens a close-only error
view and is never repaired by the viewer.

For offline inspection:

```sh
cargo run -p talaria-workflow --example progress_dump -- --run-dir <run-folder>
cargo run -p talaria-workflow --example progress_dump -- --journal <journal.jsonl>
cargo run -p talaria-tui --example progress_dump -- --run-dir <run-folder>
cargo run -p talaria-tui --example progress_dump -- --journal <journal.jsonl> --json
cargo run -p talaria-tui --example progress_dump -- --journal <journal.jsonl> --out <parent-folder>
```

`--out` retains the replay beneath that parent using the recorded run id.
Neither offline inspection nor opening an agent submits a prompt.
