# FIX-4: A failed skin restore can leave an unknown skin saved

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the code that exists today. The requirements, the tests and the acceptance checks are only in the issue.

## What exists today

`AppearanceService::set_skin` is `crates/talaria-app/src/appearance.rs:256`. `skin_command` at `:250` calls it when a name is present.

`validation` is `Some(true)` when the completion list contains the name, `Some(false)` when a short list does not, and `None` when the list cannot be fetched or the prefix search is inconclusive (`appearance.rs:267-278`). `Some(false)` returns at `:280` with `Hermes has no skin named "{name}".` and does not write.

`None` reads `configured_skin` (`:288-300`). A failed read returns `Talaria could not read the previous skin, so nothing was changed.` The write of the requested name is `config.set` key `skin` at `:302-308`. If `runtime.describe` then fails and `validation` is not `Some(true)`, the return at `:347-350` says the skin may have been saved and does not restore. If describe reports `AppearanceChange::UnknownSkin` and a previous value was read, a second `config.set` writes that previous name (`:361-373`). Any other result of that call returns at `:374` with `could not confirm restoration`, and the unknown name stays.

`skin_command_restores_the_previous_skin_when_the_list_cannot_be_fetched` (`appearance_flow.rs:411`) expects the restore sentence and a last `config.set` value of `default`. `skin_command_does_not_change_an_unrestorable_previous_value` (`:451`) expects no `config.set` when the previous value cannot be read. Helpers `set_skin_list_unavailable`, `set_config_skin`, and `set_config_skin_null` are on `MockGateway` in `tests/support/mock_gateway.rs`. `new_service` is `appearance_flow.rs:61`.
