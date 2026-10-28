# Workflow progress

Open `/workflows <run-folder>` to inspect a workflow journal. The view uses the
manifest and complete journal lines before subscribing to running sessions.
Closing it with Escape leaves the run running and retains the selection for
reopening. The open view polls the journal every 250 ms, including while disconnected.
Gateway subscriptions, agent controls and opening a session use the manifest's profile, even when Talaria was launched with a different profile.
Run discovery and the picker become available with the runner integration.

The tree groups journal keys under their workflow and phase, with child
workflows and Hermes subagents nested below their parent. Subagents do not
increase the run's agent count. Rows show the journal's model, provider,
duration and usage; selected agents show their prompt, last tool and recent
output. Filters cycle through all, working, needs-you and failed.

Agent states are done, failed, skipped, unknown (unresolved journal attention),
ended (engine ended without a journal outcome), waiting (an outstanding
approval or clarification), stopping, running and queued. A recorded outcome
wins over later live activity. Run states are running, completed, failed and
aborted. A resumed execution keeps the previous outcome separately. After a
minute without new journal lines, a running view adds `no live runner detected`.

Use the arrow keys to select and collapse rows, Enter to open an agent, `s` to
steer, `x` to stop an agent, `r` to request restart on the next resume, `p` to
pause or resume, `shift+x` to stop the run, and `f` to cycle filters. Restart
and stop-run require a second matching key or `y` within three seconds. The
`workflow_*` entries in `[keys]` remap these defaults.

Agent controls use Hermes's canonical session calls. Run controls publish
create-new request files under `control/`; a request is not proof that a
runner acted. Pause needs journal evidence; stop finishes only when the
journal records the outcome. A warning after ten seconds says when no runner
answered. Restart remains pending for the next resume. Completed and skipped
work cannot be restarted from this view.

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
