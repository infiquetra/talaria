# Talaria v0.6.0 acceptance notes

## Provenance

Gate-0 ran on merged tree `c571590`: uv sync clean; ruff clean; mypy clean (201 files); pytest 2804 passed, 7 skipped; bandit exit 0; git diff --check clean. The live matrix ran in scratch against the same tree; existing sessions were preserved and no real secrets were involved.

## Limitations (controller-reported)

- Bare host tty unexercised.
- F2 slash.exec 4001 on listed sids (the honest-unavailable leg is the 4018).
- Marketplace fetch not re-attempted (prior: search live, downloads 404 safe refusal).
- Corrupt-file reload plus sub-dialog races unit-covered only.

## Release-time refresh

Candidate commit `6e2baf16fbf6af7de68ea93c8f7fc19e38e98882` names the verified bytes. Re-run this record flow on the release tree before tagging so the manifest binds the tag commit.
