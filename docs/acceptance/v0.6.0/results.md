# Talaria v0.6.0 acceptance results

Twelve controller-observed live-matrix rows against the verified tree, candidate commit `6e2baf16fbf6af7de68ea93c8f7fc19e38e98882`, wheel `talaria-0.6.0-py3-none-any.whl` (`05c8d9752cf1…), gate `v0-6-daily-driver`. Every row passed; the full matrix report lives on https://github.com/infiquetra/talaria/issues/127.

| Item | Row | Verdict |
| ---: | --- | --- |
| 1 | 119/F1 catalog truth | pass |
| 2 | 119/F2 honest 4018 | pass |
| 3 | 120/F3 nested toggle | pass |
| 4 | 120/F4 idle cancel | pass |
| 5 | 122/F5 inspector diagnostics | pass |
| 6 | 125/F6 bar pickup | pass |
| 7 | 124/F7 import round trip | pass |
| 8 | 123/F8 Homebrew | pass |
| 9 | 123/F9 inheritance | pass |
| 10 | 121/F10 picker parity | pass |
| 11 | 126 credential deny | pass |
| 12 | annex Hermes touch | pass |

The machine-readable receipts under `evidence/` carry each observation with its provenance; `artifact-manifest.json` binds them to the candidate.
