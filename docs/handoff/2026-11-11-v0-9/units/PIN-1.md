# PIN-1: Contract generator accepts the Hermes v0.23 contracts

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document. Read the issue for REQ-1 through REQ-4 and the one updated test. This page maps the removal list and the check that reads it. It does not restate the requirements.

## What exists today

`SCHEMA_REMOVALS` in `tools/contract-gen/contract_gen.py` is a tuple of four-field rows: schema, property, keyword, reason. Nine rows are there today (`contract_gen.py:31-41`). The six title rows are `CanonicalCorrectionAck.status`, `PromptResponseResult.status`, `ProviderLimit.scope`, `ProviderUsageWindow.scope`, `SessionAutomationParams.kind`, and `CanonicalPendingPrompt.kind`. Each title reason says distinct inline enums of that title must not collapse. There is no `CanonicalBotCancelResult` row and no `ImageRenderResult` row.

`rejected_keyword_sites` returns the first three fields of each row (`contract_gen.py:273-274`). `preprocess_schemas` pops each listed keyword (`contract_gen.py:236`). `validate` walks the preprocessed definitions and raises `ContractError("inline string enum title collision: {}")` when one title has two different value sets (`contract_gen.py:199-205`). `validate_contract` is that function (`contract_gen.py:285`). `generate` calls `validate` before it writes files (`contract_gen.py:509`).

`REJECTED_ROWS` in `tools/contract-gen/tests/test_lift.py` is the same nine triples, without the reason (`test_lift.py:17-27`). `test_rejected_defaults_dropped_only_at_listed_sites` loads the vendored contract, asserts the two lists match, and then looks up every schema in the lifted document (`test_lift.py:101-115`). `CanonicalBotCancelResult` and `ImageRenderResult` are not in the vendored contract, so a lookup of either name fails until the test skips a schema that is absent.

`test_inline_string_enums_sharing_a_title_must_agree` plants two enums titled `Collision Probe` on `GatewayReadyPayload` (`test_lift.py:193-217`). Agreeing values pass. Differing values raise. It does not load a newer contract.

`test_shared_names_point_at_their_own_schemas` asserts the only shared method with identical schema refs is `clarify.lock` (`tools/contract-gen/tests/test_methods.py:32-53`). That assertion matches the vendored contract. `PIN-3` updates it.

The README sentence at `tools/contract-gen/README.md:31-32` says "the nine individually named keyword exceptions". The following sentences keep the discriminator count of 13.

## Why the two rows are enough

At the vendored pin the two new schemas do not exist, so `validate` does not see their titles and the generated files stay as they are. On a Hermes v0.23 contract, `CanonicalBotCancelResult.status` is a second `Status` title and `ImageRenderResult.mode` is a second `Mode` title. Removing only the Status title still leaves the Mode collision. Removing both titles clears both. `SharedMetricsDesktopModeDay.mode` is the Mode title that stays. This unit does not vendor that contract. `PIN-3` is the generate that runs it.

## Edge cases

| Case | Result |
| --- | --- |
| The two new schemas are absent from the loaded contract | the lists still include both triples, and the property lookup skips them |
| The two new schemas are present | the listed keyword is absent from that property |
| Two probe enums share a title and differ | `validate` still raises |
| A file under `src/generated/` would change | stop. Do not commit it |
