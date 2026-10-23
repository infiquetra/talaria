# ADR-0012: Verification is proportionate

Status: `accepted`
Date: 2026-10-22
Accepted: 2026-10-23
Deciders: operator
Supersedes: none (it replaces the per-release acceptance-receipt practice, which no ADR recorded)
Affected components: tests, continuous integration, release process, documentation

## Context

About 44% of all lines added to Talaria were acceptance records, acceptance scripts, process
documents and the journal; the product package was 20%. The tracked `docs/` tree holds 2,234 files of
acceptance evidence. The journal records that two hours of real use found seven defects the acceptance
apparatus missed, and that the apparatus produced defects of its own (receipts that forbade squash
merges, an old release's test turning red when an acceptance script changed). The gate that passed in
August did not measure launch time, keystroke latency or streaming CPU, which is what the operator
feels.

## Decision

1. Each work unit is accepted on automated tests: unit tests on the reducer, replay tests driven by
   recorded frame logs, render snapshot tests, and contract regeneration.
2. Performance budgets (launch, keystroke latency while streaming, streaming CPU, bytes per frame,
   idle repaints, memory) run in continuous integration with a pseudo-terminal harness.
3. Before each release, a person or an agent drives a real terminal and a real multiplexer through a
   short smoke script against a live Hermes, and records what failed in the release notes.
4. No per-release receipt schemas, digest-bound evidence trees or release-record tests. Acceptance
   evidence from the Python era stays in git history under a tag; it is removed from the main branch.
5. Documents stay short: an ADR per durable decision, a journal entry per non-obvious learning, a
   changelog entry per release. Planning codes do not appear in source code comments.

## Consequences

- Effort moves back to the product.
- Fewer artifacts to reason about for agents working in the repository.
- Risk: a regression that only a live drive would catch can ship between releases. Mitigation: record
  fixtures from live sessions often, and add a replay test for every live-found defect.

## Revisit when

- The project gains users other than the operator who need formal release evidence.
