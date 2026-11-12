# Release plan for Talaria v0.8.1 to v1.0.0

Written on 2026-11-11. This folder is the reference set for the issues that plan four releases: Talaria v0.8.1, v0.9.0, v0.9.1 and v1.0.0. Each issue links the files it needs from here.

| File | What it holds |
|---|---|
| [release-map.md](release-map.md) | The four releases, their parent issues and sub-issues, and what each one waits on. |
| [units/](units/) | One unit specification for each building issue, named by key. An issue links its own under `unit_spec`. |
| [hermes-signals.md](hermes-signals.md) | Which Hermes signals reach the socket Talaria opens, and what Talaria does with each one. |
| [hc-facts.md](hc-facts.md) | What herdr core (`hc` 0.1.20) was measured to do. The recorded replies are in [hc-recordings/](hc-recordings/). |
| [deferred-inventory.md](deferred-inventory.md) | The 78 deferred items of the 2026-11-08 product plan, and the issue that takes each one up, where one does. |

Keys such as `SEAT-2` are planning names for issues. The issues are created after this folder is merged. A later pull request adds `index.md`, which gives the issue number for each key, and writes the numbers into the unit specifications.
