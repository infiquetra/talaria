# Built-in skins for the theme tests

`default.json` and `ares.json` are whole entries copied from
`hermes_cli/skin_engine.py` at Hermes tag `v0.22.4`
(commit `4d04303818b61bf3aa09278af48356315515965b`):
the `default` entry and the `ares` entry, each in the payload shape
`resolved_skin()` sends (`gateway/session_skin.py`): `name`, `colors`,
`light_colors`, `dark_colors`. Overlays the entry does not define are `{}`.

JSON has no comments, so the source lives here. The `default` colours were
checked byte-equal against the `runtime.describe` skin recorded in
`fixtures/frames/plain-answer.jsonl`.
