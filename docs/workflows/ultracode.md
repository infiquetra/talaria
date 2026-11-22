# Ultracode workflows

With Ultracode on, a plain prompt goes to the `talaria-workflows` Hermes skill, and the agent
proposes a workflow and asks before starting it. A prompt never starts a run by itself. The lead
uses the roster to choose configured tiers or models, proposes a workflow, asks for an explicit
yes for that run, then runs it with the Talaria workflow runner. Trivial requests can be answered
directly. A previous yes never authorizes another run.

Install the skill explicitly into a profile:

```sh
talaria workflow skill install --profile fixture
talaria workflow skill status --profile fixture --json
```

Installation never starts a gateway. When none is running it uses the existing local profile
folder and says so. The profile must already exist. A connected gateway confirms the folder
through its served-profile entry on the control socket. A gateway that serves the profile from a
different folder than the Hermes home gives is refused with advice to restart the gateway. A
gateway that does not serve the profile falls back to the local folder and says so. The skill
embeds that profile and a root `HERMES_HOME` prefix so the lead's terminal can find the root
gateway even when its environment names a profile folder. Use `--command <absolute-path>` if the
lead's terminal cannot find `talaria` on PATH.

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

On a local gateway Talaria also checks the workflow setup when Ultracode turns on, when a
session opens with it on, and before each routed prompt. Each finding is reported once until it
changes:

- The installed skill files are compared by hash with the install record. When another program
  changed them, most often Hermes's background self-improvement review, which rewrites skills it
  has used, Talaria says which files changed, that the agent may not follow Talaria's
  instructions (for example it may build the project itself instead of proposing a workflow),
  and asks `Reinstall Talaria's talaria-workflows skill into profile <name> now? The installed
  copy is backed up first. [y/N]`. An install from an older Talaria gets the same question.
  `y` reinstalls with `--force`. Any other answer keeps the installed copy and prints the
  command; a prompt typed instead of an answer goes back to the composer.
- Missing staffing tiers and missing built-in workflows give one notice that names what is
  missing and the commands: add `[tiers.<name>]` tables to `staffing.toml` and check them with
  `talaria workflow roster --profile <name>`; install the built-ins with
  `talaria workflow builtins install`.

To stop Hermes from changing the skill again, lock it in Hermes. Talaria does not do this for
you, because the lock lives in Hermes's own state (`skills/.usage.json`):

```sh
hermes curator lock talaria-workflows            # the default profile
hermes -p <profile> curator lock talaria-workflows
```

A locked skill stays readable and usable; the agent and the background review can no longer
edit it. `hermes curator unlock talaria-workflows` undoes it.
A missing skill on a local session asks once: `Install talaria-workflows into profile <name> now? [y/N]`. `y` installs it and prints the installer's result. Any other answer prints the install command. A session started with `--remote` does not ask; its notice says the skill must be installed on the machine that runs the gateway. Check that the skill is enabled in Hermes if it
is installed but absent from the catalogue. A quick command, plugin, or alias that takes
`/talaria-workflows` is refused. A project-local skill can take precedence over the installed
copy; status shows the catalogue's reported origin.

The toggle belongs to a stored session. Talaria saves it in `workflow-mode.json` in its home
folder (`$TALARIA_CONFIG_DIR`, or `~/.talaria`), keyed by the stored session id, so it is kept
after a reconnect, after a return from another session (such as an agent's session opened from
the workflow view), when Hermes compacts the session into a new stored id, and after a restart
with `talaria --resume`. Sessions not opened for 30 days are dropped from the file. A `/branch`
copy starts with its parent's setting and says `Ultracode stays on in this branch.` Other
sessions keep their own setting. A session that opens with Ultracode on prints
`Ultracode is on in this session: a plain prompt is handled as a workflow. /ultracode off to stop.`
After you interrupt a turn in a session that has routed a prompt, Talaria prints
`Ultracode is still on: your next plain prompt is handled as a workflow too.` The foreground
session controls the `ultracode` status segment, shown after the model and dropped first when
space is tight. Start interactive sessions on with `talaria --ultracode`. Print mode does not
use this flag.

Each routed prompt prints one line, `Ultracode: handling this prompt as a workflow.`, before it
is sent. The agent then proposes a workflow and asks before it starts one. While Ultracode
is on, a slash command that carries a prompt (`/goal`, `/bg`, `/queue`, `/steer`, `/btw` and
`/plan`, with their aliases) still goes to Hermes as typed, after a one-line notice that names
the command and says to send the prompt without it to have the task handled as a workflow.
`/goal` subcommands such as `/goal status` print no notice.

After a routed prompt, Talaria checks the local runs folder every two seconds for a run of
the same profile that started after the prompt. It keeps checking for an hour after the last
routed prompt, because the agent asks before it starts a run and the answer can come much
later; a run still going after that hour is followed until it ends. Each routed prompt starts
a fresh hour and forgets runs that already ended. When one starts it prints
`Workflow run started: <name> (<run id>). Press Alt+W or type /workflows to see its agents.`,
shows a dock line above the composer (`⚙ Workflow <name> · running · 3/10 agents · Alt+W to
open`), and adds the agent count to the status segment (`ultracode · 3/10 agents`), which is
then kept on a narrow line. When the run ends it prints `Workflow run <outcome>: …` and the
dock line goes away. The view never opens by itself. `workflow_view` (Alt+W) opens the latest
run that this Talaria window saw start after an Ultracode-routed prompt in the foreground
session, or the run list when it saw none. Runs are matched by profile and start time, not
by the prompt that asked for them, so for an hour after a routed prompt a run started by hand
or in another window on the same profile is also announced here and can become Alt+W's
target, and two Talaria windows on one profile can both announce the same run. Any other run
is reached through the run list. A session started with `--remote` runs workflows on the
gateway's machine, so Talaria does not watch for them and the routing notice says so.

With the toggle off, the whole word `ultracode` anywhere in a plain prompt sends just that
prompt to the talaria-workflows skill and shows a one-shot notice. Matching ignores ASCII case. Letters, digits,
underscore, and hyphen count as word characters: `ULTRACODE:` matches, `ultracoder`,
`pre-ultracode`, and `ultracode_x` do not. Non-ASCII letters are separators. Empty prompts,
slash commands, and shell commands keep their existing behavior.

The first routed prompt of a session in this Talaria process becomes
`/talaria-workflows <original text>` on the normal command service. Only a skill directive named
`talaria-workflows` is submitted, and Hermes's message, the whole skill with the request at its
end, is sent byte for byte. Errors and collisions send no prompt and restore the original
composer text. Inner tabs and newlines remain in the message. Later routed prompts in the same
conversation skip the command service: they send the typed text after one model-facing line
saying that the workflow mode is on and the skill already loaded in the conversation applies.
The whole skill goes again with the first routed prompt after Hermes compacts the session,
after the session is opened under a new stored id, after a rewind or a restore, after
`/ultracode on` is typed again, and after a prompt that carried it is removed from the queue,
refused, or never confirmed.
A prompt containing the keyword with the toggle off always carries the whole skill. Busy
sessions use the existing prompt queue.

The screen shows only what was typed. The user cell, a prompt waiting in the queue, its
preview and edit, the rewind list, and the history replayed after a resume, a reconnect or a
return to the session never show the skill command, the skill text or the reminder line. The
proposed workflow and the approval question show as usual. The prompt history file records the
typed text. Hermes replays a routed prompt as one line, so a multi-line request comes back with
its line breaks collapsed.

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

The lead uses a listed built-in path or writes a draft script named `<name>.workflow` in the
drafts folder. The `.workflow` extension keeps Hermes's file-write lint, which checks `.js`
files with `node --check` and flags the top-level `return` every workflow script ends with as
`Illegal return statement`, from reporting an error on every draft; `talaria workflow run`
accepts any extension. The skill tells the lead never to write the deliverable in the
authoring session: anything the workers must share goes in the workflow's first phase. It runs foreground workflows
in a background terminal with completion notification and waits using that process's returned
session id. The final `run_finished` JSON line supplies the run id, outcome, result, and spend.
Detached work is polled with `workflow list --json`; a completed run's result is retrieved with
`workflow resume --json <run-id>`, which the lead also uses after a background completion whose
output was cut short. Interrupted work resumes under the same id. A parked call
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
