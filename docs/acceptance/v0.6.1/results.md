# Talaria v0.6.1 acceptance results

The live tests of the v0.6.1 run against candidate commit `c798be0016936c79e356fe0d82cfab7208869c88`, wheel `talaria-0.6.1-py3-none-any.whl` (`ae15f8f6107c…), gate `v0-6-1-daily-driver`. Each live case's receipt and evidence files live under `evidence/live-NN/`; the manifest binds every receipt to the candidate by digest.

| Live case | Verdict |
| --- | --- |
| live-01 | pass |
| live-02 | pass |
| live-03 | pass |
| live-04 | pass |
| live-05 | pass |
| live-06 | pass |
| live-07 | pass |
| live-08 | pass |
| live-09 | pass |
| live-10 | pass |
| live-11 | pass |
| live-12 | pass |
| live-13 | pass |
| live-14 | pass |
| live-15 | pass |
| live-16 | pass |
| live-17 | pass |
| live-18 | pass |
| live-19 | pass |
| live-20 | pass |
| live-21 | pass |
| live-22 | pass |
| live-23 | pass |

23 of 23 expected receipts recorded; the readiness verdict itself lives in the gate document, not here.

Live cases are source-checkout evidence unless a receipt says otherwise: twenty-two source-checkout receipts, two install probes, one wheel receipt — the wheel is proved by the probes and by Live 21 executed from the built wheel in a fresh tool environment.
