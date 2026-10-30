# Compile a workflow to Kanban

`talaria workflow compile review.js --profile fixture --board work` captures a static
workflow twice without submitting prompts, checks its routes and the existing graph,
then creates dependency-linked cards. The Hermes gateway dispatches the cards after
Talaria closes. The board must already exist and `kanban.default_assignee` must be unset.
Talaria reads that setting and never changes it.

Use `--args '{"items":["parser","tests"]}'` for an inline JSON object, `--tenant name`
for a tenant, and `--repo path` when a call requests worktree isolation. The repository
anchor must be an existing, non-bare Git repository or a directory inside one. Linked
worktrees are accepted. `--no-dispatch` leaves dispatch to the gateway's regular tick.
`--dry-run` prints the planned graph and warnings without writing cards.

Fixed calls, parallel argument fan-out and prompt interpolation of earlier results are
supported. Result-dependent branches, options, computations, transformations and loop
bounds are refused. Clock and random function calls are refused too. A refusal names
`talaria workflow run`, which retains the full script semantics. Two agreeing captures
are an additional check, not a proof about arbitrary JavaScript.

Each call gets a stable key derived from the source, canonical arguments, resolved
route, assignee, tenant, workspace and child sources. Board names are deliberately
excluded because each board has its own database. Repeating the same compile checks
the complete payload and makes no writes when the graph already matches. After a
partial failure, repeat the same command to create only the missing cards. Conflicting
existing cards are never overwritten. Archived keys are eligible for recreation.

Calls outstanding together form parallel batches. Later batches depend on every call
in the preceding batch within the same workflow path; siblings do not depend on each
other. Sentinels and numeric schema markers in later prompts create data edges and
become instructions referring to real parent card IDs. Avoid literal numbers in the
900000000 marker range: they can create false edges. Child workflows have independent
control lanes; cross-lane ordering needs an explicit data dependency.

Card bodies put the result contract before the prompt. Workers complete with a summary
and metadata, place schema results in `result_json`, and declare deliverable files as
completion artifacts. Results exceeding 3,500 UTF-8 bytes belong in an artifact named
by `result_artifact`. A missing required parent handoff requires a comment and a blocked
card. Hermes does not enforce the schema. Workspaces can be removed on completion;
files left in them are not durable results. Card bodies and comments are shared across
profiles on the machine and must contain nothing secret.

Open `/kanban [board]` inside Talaria. Use `h`/`l` for columns, `j`/`k` for cards,
Enter for detail, `m` for moves, `c` for a comment, `d` to nudge the dispatcher, `b`
to choose a board, `r` to refresh and `/` to filter title or assignee. Esc clears the
filter or closes detail before closing the overlay. Done moves require completion
evidence; Done, Blocked and Archived ask for confirmation. Archiving an unfinished
parent can release children without its result. The view always refetches a move's
actual server status. If workflow columns cannot be read, all board writes are disabled.

HTTP calls and socket upgrades each use a fresh native grant. The view connects the
socket only after the snapshot mounts the gateway's plugin application. Events coalesce
into one board and open-detail refetch per 250 milliseconds. Three consecutive failed
connections park the tail while preserving the snapshot; `r` retries from the refreshed
cursor. No board-switch operation changes Hermes's shared current-board pointer.

The local mock and automated suites exercise transport, compilation and rendering.
A cold-gateway probe and a throwaway-board live completion check still require the
operator's configured gateway and dispatcher. An ignored live test is not acceptance.
