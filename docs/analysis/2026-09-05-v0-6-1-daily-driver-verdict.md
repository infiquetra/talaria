# Talaria v0.6.1 is **NOT READY** as a daily driver

This document gates the v0.6.1 release the way the v0.6.0 verdict gated its
own: one fenced block states the verdict, and the evidence table below is the
authority behind it. The twenty-one rows are the run's twenty-one live tests
(parent issue #139's live-test ledger); a row clears only when its live test
carries a passing receipt on the final candidate with reviewer inspection.
The machine checks the rows as receipts are filed — the v0.6.1 verifier
branch in `scripts/acceptance/v050_receipt.py` validates every receipt against
the shape the manifest binds, and this gate flips to READY only when all
twenty-one rows clear, with no waiver path for a blocked or reserved case.

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
```

## Evidence table

| Row | Live test | Status | Evidence |
| ---: | --- | --- | --- |
| 01 | Custom theme lifecycle: derive, select, edit, reload live | pending — interim receipt filed at e6270de; final-candidate run outstanding | Child issue #140, `evidence/live-01/` |
| 02 | Malformed theme keeps the last good appearance with actionable feedback | pending — interim receipt filed at e6270de; final-candidate run outstanding | Child issue #140, `evidence/live-02/` |
| 03 | Selection persists automatically across restart | pending — interim receipt filed at e6270de; final-candidate run outstanding | Child issue #140, `evidence/live-03/` |
| 04 | Category text colors apply independently and reload | pending — interim receipt filed at e6270de; final-candidate run outstanding | Child issue #141, `evidence/live-04/` |
| 05 | Transcript on the canvas: imported dark theme, no-fill representation | pending — interim receipt filed at e6270de; final-candidate run outstanding | Child issue #141, `evidence/live-05/` |
| 06 | Bar visibility under theme control across two widths | pending — interim receipt filed at e6270de; final-candidate run outstanding | Child issue #141, `evidence/live-06/` |
| 07 | Import reports lead with appearance-changing fallbacks; provenance at point of use | pending — interim receipt filed at e6270de; final-candidate run outstanding | Child issue #142, `evidence/live-07/` |
| 08 | Status-bar setup path renders honest data; failing script stays visible | pending — interim receipt filed at bfa8090; final-candidate run outstanding | Child issue #143, `evidence/live-08/` |
| 09 | Diagnostics inspector-only across a real freshness-to-staleness transition | pending — interim receipt filed without a harness_commit; refiling required | Child issue #144, `evidence/live-09/` |
| 10 | Caret-location label in the inspector across region navigation | pending — interim receipt filed without a harness_commit; refiling required | Child issue #144, `evidence/live-10/` |
| 11 | `session.usage` decoded and projected over two turns | pending — interim receipt filed at bfa8090; final-candidate run outstanding | Child issue #145, `evidence/live-11/` |
| 12 | Slash-command inventory discoverable with verified provenance | pending — no receipt filed yet; the live test has not run | Child issue #146 |
| 13 | Displayed project versus the agent's actual tool directory | pending — interim receipt filed at dd4e87d; the post-decision half follows the operator's decision | Child issue #151, `evidence/live-13/` |
| 14 | Assistant label removal through streaming, completion, and resume | pending — interim receipt filed at e6270de; final-candidate run outstanding | Child issue #141, `evidence/live-14/` |
| 15 | File attachments list, send, and prove receipt | pending — no receipt filed yet; the live test has not run | Child issue #147 |
| 16 | Image attachment preview and content proof on an image-capable route | pending — no receipt filed yet; the live test has not run | Child issue #147 |
| 17 | Attachment failure cases explain themselves and never claim delivery | pending — no receipt filed yet; the live test has not run | Child issue #147 |
| 18 | Mixture of Agents progress with honest fallback | pending — no receipt filed yet; the live test has not run | Child issue #148 |
| 19 | Ordinary route without rich events; supported cancellation | pending — no receipt filed yet; the live test has not run | Child issue #148 |
| 20 | Configuration views: effective values, labelled apply-or-restart | pending — no receipt filed yet; the live test has not run | Child issue #149 |
| 21 | Combined daily-use sequence: resize, reconnect, resume | pending — no receipt filed yet; the live test has not run | Child issue #150 |

## How a row clears, and how the verdict flips

A row clears when its live test carries a passing v0.6.1 receipt —
`docs/acceptance/v0.6.1/evidence/live-NN/receipt.json` — on the final
candidate, with reviewer inspection recorded on the owning child. The receipts
are the tester's products, machine-checked as they are filed; the C12-R
record flow runs the generator only after the version bump, binds every
receipt to the candidate by digest, and carries each receipt's
`applies_to_candidate` attestation for why evidence from an earlier head
still applies. When all twenty-one rows clear, C12-R re-reads this document,
flips the verdict to READY, and sets `review-by` to the tag date plus thirty
days. A blocked or reserved live test keeps this gate NOT READY; the tooling
has no waiver path by design, and the operator's rule that no live test is
waived is enforced by the machine.

The initial horizon of 2026-10-31 is the architect ruling's own: it places
this gate's first mandatory re-read a month past the expected tag window,
deliberately beyond the v0.6.0 and v0.5.0 gates' 2026-09-30 horizon so the
three re-reads do not expire together, and C12-R resets it at the flip.

## Limitations carried, not hidden

The thirteen interim receipts on file today were filed before a v0.6.1
validator existed; the validator names their defects per its contract as
this gate was written, and the final receipts C12-R binds must conform. No
row above clears on an interim receipt, whichever head it rode.