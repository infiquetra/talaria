# Talaria v0.6.1 acceptance notes

## Sequence

The manifest was generated after the version bump to 0.6.1, on the candidate commit it binds — never before, which is why the record command refuses a tree still reporting an earlier version. Every live receipt passed the v0.6.1 verifier at record time: the evidence inventory, digests, role-label tester, and full harness commit are machine-checked, and the manifest carries each receipt's `applies_to_candidate` attestation for the reviewer to inspect sentence by sentence.

The tag commit and the candidate commit `c798be0016936c79e356fe0d82cfab7208869c88` differ only by the record commit and the gate commit — the two commits these documents and the manifest themselves ride in — and by nothing release-relevant, which the release workflow's candidate check re-verifies by digest.
