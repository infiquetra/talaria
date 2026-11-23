# Hermes contract generation

Python 3.9 or later, standard library only. Install the exact generator backend:

```sh
cargo install cargo-typify --version 0.8.0 --locked
python3 tools/contract-gen/contract_gen.py generate
python3 tools/contract-gen/contract_gen.py generate --check
```

`check --require-typify` is equivalent to `generate --check`. Without that flag,
`check` skips when cargo-typify is unavailable. `CONTRACT_GEN_REQUIRE_TYPIFY=1`
disables the skip. The default typify mode includes builders. All generated files
are formatted with the workspace rustfmt; `RUSTFMT` is resolved from PATH when
the environment does not already select it.

Talaria does not pin Hermes. `PINNED` records the one Hermes revision the contract is generated from
and tested against; the generator needs exactly one. `pin` validates and prints it. Moving it to a newer
release is a reviewed manual change that nothing forces.
`verify-vendor` compares both the contract and license to the recorded commit using
the GitHub CLI's raw media type. Neither generation nor checking needs network.

`generate --hermes-checkout <clone>` extracts native owner methods from the
specified Hermes handler functions using Python's abstract syntax tree. The
checkout must match PINNED. The sorted committed list is the offline input.
Fallback eligibility assumes the full interactive grant and follows the recorded
`tui_gateway/ws_legacy_fallback.py`; drift does not refresh that fallback rule.

`config-choices --hermes-checkout <clone> [--check]` writes or checks
`config-choices.json`, the proof behind the value lists `/config` offers for keys the
gateway schema types as plain text. Each recipe in `CONFIG_CHOICE_SOURCES` names where the
recorded source states a set: a module constant, the literals in one function, the comment at
a `DEFAULT_CONFIG` key or anchor, or the desktop app's suggestion list. A missing name or
value fails. The file also records every schema `_select`. The checkout must match PINNED;
the Rust parity test in `crates/talaria-core/tests/integration/settings_choices.rs` reads
only the committed file. With `--hermes-checkout`, `drift` also lists changed choice values.

`drift --tag latest --summary <file> [--hermes-checkout <clone>]` builds in an
isolated temporary workspace. It is a report for the maintainer; a newer Hermes release is not a failure
of Talaria, and the scheduled workflow step does not fail the workflow. Binding tests decide success; recording tests
are reported separately. The source workspace is never modified.

Schema preprocessing removes closed-object markers and the keyword exceptions
listed in `SCHEMA_REMOVALS`. The 13 discriminator constants
become singleton enums without titles. The existing WaitBarrierTarget enum and
unrelated constants stay intact. Remaining inline string enums with the same
title must have the same values. At MessageCompletePayload.text only, a union
that includes an empty schema is normalized to the equivalent unrestricted
schema, avoiding typify's flattened scalar representation.

Unknown discriminator sites or received
constrained string types require a deliberate generator change. Schema root
references include every definition. No method summaries enter generated code.
