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

`pin` validates and prints PINNED. Bumping a pin remains a reviewed manual change.
`verify-vendor` compares both the contract and license to the pinned commit using
the GitHub CLI's raw media type. Neither generation nor checking needs network.

`generate --hermes-checkout <clone>` extracts native owner methods from the
specified Hermes handler functions using Python's abstract syntax tree. The
checkout must match PINNED. The sorted committed list is the offline input.
Fallback eligibility assumes the full interactive grant and follows the pinned
`tui_gateway/ws_legacy_fallback.py`; drift does not refresh that fallback rule.

`drift --tag latest --summary <file> [--hermes-checkout <clone>]` builds in an
isolated temporary workspace. Binding tests decide success; recording tests
are reported separately. The source workspace is never modified.

Schema preprocessing removes closed-object markers and the nine individually
named keyword exceptions in `SCHEMA_REMOVALS`. The 13 discriminator constants
become singleton enums without titles. The existing WaitBarrierTarget enum and
unrelated constants stay intact. Remaining inline string enums with the same
title must have the same values. At MessageCompletePayload.text only, a union
that includes an empty schema is normalized to the equivalent unrestricted
schema, avoiding typify's flattened scalar representation.

Unknown discriminator sites or received
constrained string types require a deliberate generator change. Schema root
references include every definition. No method summaries enter generated code.
