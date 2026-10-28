Small recorded frame fixtures will live here. P1-04 owns this folder; see the [roadmap](../../docs/handoff/2026-10-22-next-steps/07-roadmap-and-work-units.md#p1-04--recorder-and-replay-source). Recorded frame logs must be redacted before committing; whole corpora never enter version control.

`p5-04-model-options.jsonl` records a profile-bound model roster exchange. Provider addresses,
credential references, account labels, local paths, and other private values must be removed
before committing it; replay and privacy tests verify the fixture.

The inventory contains 31 JSONL fixtures. Journal fixtures use the workflow journal reader; the other fixtures use the gateway frame replay reader.

| Fixture | Reader |
| --- | --- |
| [approval.jsonl](approval.jsonl) | Gateway frame replay |
| [attachments-mention-image.jsonl](attachments-mention-image.jsonl) | Gateway frame replay |
| [background-work.jsonl](background-work.jsonl) | Gateway frame replay |
| [bg-launch.jsonl](bg-launch.jsonl) | Gateway frame replay |
| [clarification.jsonl](clarification.jsonl) | Gateway frame replay |
| [context-compaction.jsonl](context-compaction.jsonl) | Gateway frame replay |
| [fleet-peek.jsonl](fleet-peek.jsonl) | Gateway frame replay |
| [fleet-registry.jsonl](fleet-registry.jsonl) | Gateway frame replay |
| [hermes-commands.jsonl](hermes-commands.jsonl) | Gateway frame replay |
| [interrupted.jsonl](interrupted.jsonl) | Gateway frame replay |
| [model-switch.jsonl](model-switch.jsonl) | Gateway frame replay |
| [p5-02-idempotency.jsonl](p5-02-idempotency.jsonl) | Gateway frame replay |
| [p5-04-model-options.jsonl](p5-04-model-options.jsonl) | Gateway frame replay |
| [p5-05-progress.jsonl](p5-05-progress.jsonl) | Workflow journal |
| [permissions-cd.jsonl](permissions-cd.jsonl) | Gateway frame replay |
| [plain-answer.jsonl](plain-answer.jsonl) | Gateway frame replay |
| [profiles.jsonl](profiles.jsonl) | Gateway frame replay |
| [queue-edit-remove.jsonl](queue-edit-remove.jsonl) | Gateway frame replay |
| [queue-move-send-now.jsonl](queue-move-send-now.jsonl) | Gateway frame replay |
| [queue-steer.jsonl](queue-steer.jsonl) | Gateway frame replay |
| [queued-prompt.jsonl](queued-prompt.jsonl) | Gateway frame replay |
| [review-oneshot.jsonl](review-oneshot.jsonl) | Gateway frame replay |
| [review-session-info.jsonl](review-session-info.jsonl) | Gateway frame replay |
| [rewind-and-rollback.jsonl](rewind-and-rollback.jsonl) | Gateway frame replay |
| [session-picker.jsonl](session-picker.jsonl) | Gateway frame replay |
| [shell-and-btw.jsonl](shell-and-btw.jsonl) | Gateway frame replay |
| [slash-palette.jsonl](slash-palette.jsonl) | Gateway frame replay |
| [subagent-batch.jsonl](subagent-batch.jsonl) | Gateway frame replay |
| [subagent-control.jsonl](subagent-control.jsonl) | Gateway frame replay |
| [todo-checklist.jsonl](todo-checklist.jsonl) | Gateway frame replay |
| [tool-diff.jsonl](tool-diff.jsonl) | Gateway frame replay |
