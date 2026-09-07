# Talaria v0.6.1 is **NOT READY** as a daily driver

This document gates the v0.6.1 release the way the v0.6.0 verdict gated its
own: one fenced block states the verdict, and the evidence table below is the
authority behind it. The twenty-three rows are the run's live tests: the twenty-one of parent
issue #139's live-test ledger, plus Live 22 and Live 23, which the operator
added on child issue #150 (comment 5561683073) and which appear in no ledger
on #139; a row clears only when its live test
carries a passing receipt on the final candidate with reviewer inspection.
The machine checks the rows as receipts are filed — the v0.6.1 verifier
branch in `scripts/acceptance/v050_receipt.py` validates every receipt against
the shape the manifest binds, and this gate flips to READY only when all
twenty-three rows clear, with no waiver path for a blocked or reserved case.

```gate
id: v0-6-1-daily-driver
verdict: NOT READY
review-by: 2026-10-31
blocks-on: row-01 pending
blocks-on: row-02 pending
blocks-on: row-03 pending
blocks-on: row-04 pending
blocks-on: row-05 pending
blocks-on: row-06 pending
blocks-on: row-07 pending
blocks-on: row-08 pending
blocks-on: row-09 pending
blocks-on: row-10 pending
blocks-on: row-11 pending
blocks-on: row-12 pending
blocks-on: row-13 pending
blocks-on: row-14 pending
blocks-on: row-15 pending
blocks-on: row-16 pending
blocks-on: row-17 pending
blocks-on: row-18 pending
blocks-on: row-19 pending
blocks-on: row-20 pending
blocks-on: row-21 pending
blocks-on: row-22 pending
blocks-on: row-23 pending
```

## Evidence table

| Row | Live test | Status | Evidence |
| ---: | --- | --- | --- |
| 01 | Custom theme lifecycle: derive, select, edit, reload live | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #140, `evidence/live-01/` |
| 02 | Malformed theme keeps the last good appearance with actionable feedback | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #140, `evidence/live-02/` |
| 03 | Selection persists automatically across restart | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #140, `evidence/live-03/` |
| 04 | Category text colors apply independently and reload | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #141, `evidence/live-04/` |
| 05 | Transcript on the canvas: imported dark theme, no-fill representation | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #141, `evidence/live-05/` |
| 06 | Bar visibility under theme control across two widths | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #141, `evidence/live-06/` |
| 07 | Import reports lead with appearance-changing fallbacks; provenance at point of use | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #142, `evidence/live-07/` |
| 08 | Status-bar setup path renders honest data; failing script stays visible | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #143, `evidence/live-08/` |
| 09 | Diagnostics inspector-only across a real freshness-to-staleness transition | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #144, `evidence/live-09/` |
| 10 | Caret-location label in the inspector across region navigation | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #144, `evidence/live-10/` |
| 11 | `session.usage` decoded and projected over two turns | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #145, `evidence/live-11/` |
| 12 | Slash-command inventory discoverable with verified provenance | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #146, `evidence/live-12/` |
| 13 | Displayed launch directory, the agent's session directory, and the adoption status between them | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #157, `evidence/live-13/` |
| 14 | Assistant label removal through streaming, completion, and resume | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #141, `evidence/live-14/` |
| 15 | File attachments list, send, and prove receipt | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #147, `evidence/live-15/` |
| 16 | Image attachment preview and content proof on an image-capable route | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #147, `evidence/live-16/` |
| 17 | Attachment failure cases explain themselves and never claim delivery | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #147, `evidence/live-17/` |
| 18 | Mixture of Agents progress with honest fallback | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #148, `evidence/live-18/` |
| 19 | Ordinary route without rich events; supported cancellation | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #148, `evidence/live-19/` |
| 20 | Configuration views: effective values, labelled apply-or-restart | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #149, `evidence/live-20/` |
| 21 | Combined daily-use sequence: resize, reconnect, resume | pending — the live test has not run; it is an attended session and the only case with no receipt on file | Child issue #150 |
| 22 | Keyboard defaults under nested terminal capture | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #150, `evidence/live-22/` |
| 23 | Compatibility drift and refusal | pending — passing receipt filed on the candidate `c798be0`; clears when the record flow binds the manifest and `verify-run` confirms the set | Child issue #150, `evidence/live-23/` |

## How a row clears, and how the verdict flips

A row clears when its live test carries a passing v0.6.1 receipt —
`docs/acceptance/v0.6.1/evidence/live-NN/receipt.json` — on the final
candidate, with reviewer inspection recorded on the owning child. The receipts
are the tester's products, machine-checked as they are filed; the C12-R
record flow runs the generator only after the version bump, binds every
receipt to the candidate by digest, and carries each receipt's
`applies_to_candidate` attestation for why evidence from an earlier head
still applies. When all twenty-three rows clear, C12-R re-reads this document,
flips the verdict to READY, and sets `review-by` to the tag date plus thirty
days. A blocked or reserved live test keeps this gate NOT READY; the tooling
has no waiver path by design, and the operator's rule that no live test is
waived is enforced by the machine.

The initial horizon of 2026-10-31 is the architect ruling's own: it places
this gate's first mandatory re-read a month past the expected tag window,
deliberately beyond the v0.6.0 and v0.5.0 gates' 2026-09-30 horizon so the
three re-reads do not expire together, and C12-R resets it at the flip.

## Limitations carried, not hidden

The interim receipts this gate was written against are gone: the evidence
tree now holds twenty-two final receipts, every one bound to the candidate
`c798be0` and inspected by the reviewer. They are graded `pending` above
rather than `met` because the row rule in "How a row clears" is not yet
satisfied for them. A row needs a passing receipt on the final candidate
**and reviewer inspection recorded on the owning child**, and it is that
second leg that is outstanding: every inspection on file was recorded on
2026-09-05 or 2026-09-06 and attests receipts these have since superseded,
while all twenty-two current receipts were recorded on 2026-09-07. Child
issue #157, which owns row 13, carries no inspection record at all. The
reviewer has in fact inspected the current tree; what is missing is the
recorded form the rule requires.

Separately, and downstream of the rows rather than part of them, the record
flow has not bound the manifest and `verify-run` has not been run over the
whole tree. Those are conditions on the verdict flip, not on a row, and the
flip is the tooling's to make.

Row 21 is the outstanding case. It is an attended live session and cannot
be run unattended, so no row above can be graded clear and no waiver exists
for it.

Row 13 previously named child issue #151 and described its second half as
waiting on an operator decision. Both were wrong. The decision was taken on
2026-09-06 and delivered as child issue #157, merged through pull requests
#160 and #163 before the candidate was cut. The row names #157 now, and the
filed receipt tests #157's contract.