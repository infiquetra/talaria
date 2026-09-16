# Talaria v0.6.3 acceptance notes

This acceptance record binds product candidate
`a11d4261ffb15b8ac7b596c7170ea9faea152f28` to gate
`v0-6-3-configuration-residuals`.

The manifest records three passing configuration-residual receipts: the
accepted CFG-P2-CR3 review, the project check on the pre-version-bump
candidate, and the U1V version/docs successor commit. The project-check
receipt preserves the one expected v0.6.2 HEAD-gate failure and does not
classify it as a product failure.

The record is complete for release gating. `install_receipts` is empty by
design: no post-tag install identity or remote-login acceptance is claimed.
