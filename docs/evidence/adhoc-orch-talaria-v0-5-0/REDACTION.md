# Redaction record — Talaria v0.5.0 code-review evidence

These artifacts were produced by the Saga code-review engine in a private operator worktree and
published into this public repository. Operator-local and probe-planted strings were replaced with
marked placeholders before publication, and the custody chain was rebuilt over the redacted bytes so
the published artifacts verify against the published ledger.

## The standard applied

These bytes pass **the privacy gate this repository enforces** — the portable scanner in
`scripts/acceptance/v050_receipt.py`, imported and run directly rather than reimplemented. An
earlier draft of this publication was scanned with a purpose-written pattern set, returned clean,
and was then rejected by the repository's own scanner with eight errors across four files. That is
the reason no string is exempted here on the grounds that a reviewer judged it harmless: a
carve-out is invisible to the next person who scans, and a gate one exempts oneself from is not a
gate.

## What was replaced

| Class of string | Placeholder | Occurrences |
| --- | --- | ---: |
| operator account name | `<operator>` | 2 |
| operator home path | `<operator-home>` | 14 |
| operator temporary root | `<operator-temp-root>` | 2 |
| planted email-shaped probe string | `<planted-email>` | 2 |
| planted home-path-shaped probe string | `<planted-home-path>` | 2 |
| public hosted-runner home | `<hosted-runner-home>` | 2 |

Three of these classes are not operator data at all. Two are strings a security lens *planted* to
prove a privacy gate did or did not fire, and one is the public hosted-runner home used in the same
probe. They are replaced anyway, and nothing is lost: in each case the finding's force is that the
planted string **matches the shape the detector is meant to catch**, and the placeholder names that
shape directly. A reader learns exactly what the probe established.

## Content hashes before and after

An artifact's filename is its content hash, so redaction renames the artifacts.

| Cycle | Reviewed revision | Pre-redaction | Published |
| --- | --- | --- | --- |
| 1 | `122bd918e0056404e576ae5623ce9e97bfe1ad93` | `2b64a225506486bf…` | `68a86479d1b46a82…` |
| 2 | `83ffd27addc6df4cbdb73bc996baa7d11a2610f3` | `657c776e2ec70ded…` | `a7a009aed0ea2673…` |
| 3 | `3016f177a8b07949eb1e59a9b64f000b01a892b3` | `d6f674a2cf79cd35…` | `3e3a340b21fcbb9e…` |

Two of the three criteria files were unchanged by redaction and keep their original hashes
(`70b1a92088c03287…` for cycle 1, `399f6c30ce181b4e…` for cycle 2). The cycle-3 criteria file
changed from `bc179e93e0a1791e…` to `626e4f1f7fa3ee0c…`.

## What was not changed

No finding, score, dimension, measurement, evidence line, verdict or timestamp was altered, added or
removed. Every ledger entry keeps its original `seq`, `kind`, `check_id`, `reviewed_sha`, `verdict`,
`attempt` and `timestamp`. Three fields changed and only three: the content `hash`, the `prev`
linkage that depends on it, and the path fields — which recorded absolute operator paths and are now
repository-relative, so the chain verifies from the repository root on any machine rather than only
on the one that produced it.

## Verifying

From the repository root:

```
python3 <saga>/scripts/evidence_ledger.py verify --root docs/evidence/adhoc-orch-talaria-v0-5-0
```

At publication this reported 6 entries, 3 artifacts and 3 criteria verified, and the repository's
own privacy scanner reported zero errors across all 9 files.
