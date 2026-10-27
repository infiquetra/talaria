# Doctor JSON format

`talaria doctor --json` writes one `talaria-doctor-v1` object on one line, followed by a newline.
`overall` is `ok`, `warn`, or `fail`; `profile` is the selected profile. `talaria` contains
`version`, `hermes_tag`, and the full pinned `hermes_commit`.

`checks` contains seven objects in order: `talaria`, `gateway`, `hermes_version`, `capabilities`,
`profiles`, `attach`, and `runtime_health`. Each has `id`, `severity` (`ok`, `warn`, `fail`,
`skip`, or `info`), `summary`, an array of `details`, and a nullable `fix`.

`restart` is null unless a restart was requested. Otherwise it contains `outcome`, `seconds`,
`http_status`, and `text`. Outcomes are `restarted`, `refused`, `changed`, `did_not_restart`,
`not_back`, `not_ready`, `declined`, and `needs_confirmation`. `seconds` is numeric only for
`restarted`; `http_status` is numeric only for a refusal with a status. Both are null otherwise.
After a successful restart, `checks` describes the new report; otherwise it is the first report.

Displayed identities and commits use eight characters. Paths become `<path>`, and contiguous
runs of at least 40 letters, digits, underscores or hyphens become `<redacted>` before text is
trimmed to its display limit. The trusted pinned commit in `talaria.hermes_commit` is the sole
full-commit exception. Tickets, session keys and the gateway's per-install identity are omitted.
