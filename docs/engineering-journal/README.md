# Engineering Journal - talaria

Living documentation for `talaria`. This journal prevents repo-local knowledge loss across sessions, maintainers, and AI agents.

The journal follows the [Infiquetra engineering journal standard](https://github.com/infiquetra/infiquetra-context-library/blob/main/docs/repositories/engineering-journal-standard.md).

## Files

| File                         | What it holds                                                               | When to update                                                 |
| ---------------------------- | --------------------------------------------------------------------------- | -------------------------------------------------------------- |
| [LEARNINGS.md](LEARNINGS.md) | Empirical findings, mechanisms, fixes, validations, and generalizable rules | Significant runs, bugs, audits, or surprising test results     |
| [DECISIONS.md](DECISIONS.md) | Repo-scoped tactical decisions with rationale and revisit conditions        | Choices between alternatives or local architecture decisions   |
| [QUEUED.md](QUEUED.md)       | Deferred future work with priority and worth-it-when triggers               | Useful ideas not being built immediately                       |
| [ARCHIVE.md](ARCHIVE.md)     | Shipped, rejected, and superseded items                                     | Queued work ships or a prior entry is invalidated              |
| [narratives/](narratives)    | Longer standalone companion write-ups                                       | Design walkthroughs, migrations, or post-incident notes        |
| [audits/](audits)            | Deep reviews and investigation snapshots                                    | Security reviews, integration investigations, or drift reviews |

## Update triggers

Update the journal in the same change when work creates a durable learning, decision, deferred item, shipped/rejected item, superseded entry, narrative, or audit.

Keep entries public-safe. Do not add secrets, private operational details, or copied private policy text.
