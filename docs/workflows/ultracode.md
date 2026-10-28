# Ultracode workflows

Ultracode routes substantive prompts through the `talaria-workflows` Hermes skill. The lead
uses the roster to choose configured tiers or models, proposes a workflow, asks for an explicit
yes for that run, then runs it with the Talaria workflow runner. Trivial requests can be answered
directly. A previous yes never authorizes another run.

Install the skill explicitly into a profile:

```sh
talaria workflow skill install --profile fixture
talaria workflow skill status --profile fixture --json
```

Installation never starts a gateway. When none is running it uses the existing local profile
folder and says so. The profile must already exist. A connected gateway's first profile row
supplies the authoritative home; a mismatched name is refused. The skill embeds that profile
and a root `HERMES_HOME` prefix so the lead's terminal can find the root gateway even when its
environment names a profile folder. Use `--command <absolute-path>` if the lead's terminal
cannot find `talaria` on PATH.

`install` supports `--command`, `--force`, `--dry-run`, and `--json`. Status distinguishes
missing, current, outdated, modified, and foreign. An unchanged current install is a no-op.
An unchanged older render can be updated. Edits, extra or missing files, and unrecorded folders
are refused until an explicit `--force`. Forced changes first keep a dated backup under
Talaria's `skills/backups/`; links are copied as links and their destinations remain untouched.
Backups are never pruned automatically. The install record is `skills/installs.json` under
Talaria's home. Publication and record updates hold two advisory locks and recover interrupted
changes; staging stays inside the profile's excluded `skills/.locks/` folder.

```sh
talaria workflow skill install --profile fixture --dry-run --json
talaria workflow skill uninstall --profile fixture
talaria workflow skill uninstall --profile fixture --force --json
```

Dry run and status take no lock and write nothing. Uninstall removes unchanged copies and their
records; edited or foreign copies need force and a backup. JSON is one version 1 object with
profile, pre-action state, installed and embedded versions, target, action, changed_files,
backup, and warnings. Differences carry changed:, added:, removed:, link:, or other: prefixes.
Exit 0 is success or no-op, 1 is refusal or an I/O failure, and 2 is invalid input or a missing
profile.

In an interactive Talaria session:

```text
/ultracode on
/ultracode status
/ultracode off
```

Bare `/ultracode` toggles. Turning on and checking status read a fresh command catalogue.
A missing skill prints its install command. Check that the skill is enabled in Hermes if it
is installed but absent from the catalogue. A quick command, plugin, or alias that takes
`/talaria-workflows` is refused. A project-local skill can take precedence over the installed
copy; status shows the catalogue's reported origin.

The toggle belongs to a session id and lives only in this Talaria process. The foreground
session controls the `ultracode` status segment, shown after the model and dropped first when
space is tight. Start interactive sessions on with `talaria --ultracode`. Print mode does not
use this flag.

With the toggle off, the whole word `ultracode` anywhere in a substantive prompt routes just
that prompt and shows a one-shot notice. Matching ignores ASCII case. Letters, digits,
underscore, and hyphen count as word characters: `ULTRACODE:` matches, `ultracoder`,
`pre-ultracode`, and `ultracode_x` do not. Non-ASCII letters are separators. Empty prompts,
slash commands, and shell commands keep their existing behavior.

A routed prompt becomes `/talaria-workflows <original text>` on the normal command service.
Only a skill directive named `talaria-workflows` is submitted. Hermes's message is sent byte
for byte, and its display string becomes the user cell. Errors and collisions send no prompt
and restore the original composer text. Inner tabs and newlines remain in the message; Hermes
may collapse them in the display. Busy sessions use the existing prompt queue.

The lead first reads:

```sh
talaria workflow roster --profile fixture --json
```

The version 1 envelope contains the unchanged staffing report plus paths for Talaria's home,
runs, workflows, and drafts; saved workflow names, paths, and descriptions; skill state;
warnings; and an error. The drafts directory is created privately before gateway work.
Broken or oversized scripts remain listed with a null description and a warning. Exit 2
retains the full report with staffing problems. Exit 1 has a null report, gateway-independent
local fields, and an error kind: home_unavailable, filesystem, staffing_invalid,
gateway_unavailable, profile_unserved, or roster_failed. Roster discovery never ensures a gateway.

The lead uses a listed built-in path or writes a draft script. It runs foreground workflows
in a background terminal with completion notification and waits using that process's returned
session id. The final `run_finished` JSON line supplies the run id, outcome, result, and spend.
Detached work is polled with `workflow list --json`; a completed run's result is retrieved with
`workflow resume --json <run-id>`. Interrupted work resumes under the same id. A parked call
requires the developer to choose the printed `workflow resolve` command's fail, rerun, or abort
resolution. The lead never chooses a new paid attempt alone.

Workflow agents cannot answer approval requests. Dangerous-command behavior depends on the
operator's profile approval settings; enabling ultracode does not change them. Code-reading
agents use worktrees and a no-edit instruction because the file toolset can write. A worktree
is a separate checkout, not a sandbox. Keep credentials out of script arguments and prompts.

Talaria uses several configured models and providers through staffing tiers. Its toggle is
`/ultracode` and `--ultracode`, rather than Claude Code's effort setting. A Hermes skill guides
the lead, and the detached runner owns durable execution. Script structure is checked by the
engine, while planning quality and asking before each run depend on the lead following the skill.
