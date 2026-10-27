# Terminal views

Talaria keeps finished conversation output in native terminal scrollback. Full-screen views
temporarily cover it; closing a view restores the conversation and the draft.

## Capabilities

`/capabilities [skills|hub|toolsets|plugins]` opens the capabilities of the connected profile.
The title always names that profile. The same view is linked from the settings screen's More
category as **Skills, toolsets and plugins**. Hermes's `/skills`, `/toolsets`, `/tools` and
`/plugins` commands retain their Hermes meanings.

Tab and Shift+Tab change tabs; 1–4 select one directly. Arrow and page keys move through a list.
Enter reads a skill, previews a hub item, or opens details. At narrower widths the detail replaces
the list. Escape steps back through confirmations, details and inputs before closing the view.

Space toggles a skill, toolset or plugin. The view reads back the result and reports what Hermes
now holds. A second change waits for the current one, including its read-back. Closing the view
keeps an in-flight change alive; its result appears in the conversation. Reopening retains that
change and its target profile until it completes.

`/` filters installed lists as you type. In Skill hub it opens a search; Enter runs the search,
and `n` and `p` change browse pages. `i` scans the selected skill and shows the findings. Only an
allowed scan offers confirmation, and only `y` installs. A successful installation is checked
against the installed list, then the foreground session reloads skills when it has the same profile.

In Plugins, `i` accepts a lowercase catalog name or an HTTPS repository address. Talaria shows
the catalog source and commit, or identifies a repository outside the catalog, before confirmation.
`c` opens the catalog. `a` includes bundled plugins. `x` asks before removing a user install;
bundled plugins can be turned off, and package plugins must be removed outside Talaria. Plugin
settings and update information are displayed as details; secret settings show only set or not set.

`r` refreshes a tab. There is no background polling. The view needs at least 40 columns by 10 rows.
Browse keys can be changed in `[keys]`; text entry and confirmations use fixed controls.

For scripts, `talaria capabilities skills`, `toolsets`, or `plugins` prints a table. Add `--json`
for one JSON object and `--profile <name>` to select a served profile. These commands read only,
do not create a session, and do not start a gateway. They exit 0 on success, 1 on a failed read,
and 2 for an invalid list or arguments.

## Memory and curator

`/memory-settings` opens memory providers, built-in file sizes and the skill curator for the
foreground profile. Settings also links to it through **Memory and curator**. `/memory` and
`/curator` retain Hermes's own command meanings. The overlay needs at least 60 columns and 16 rows.

Select a provider and press Enter to read or edit its settings. `u` switches to a ready provider;
`Ctrl+S` saves edited fields and asks before activating a different provider. Talaria reads settings
back after writing them and reports each field's result. On the plain settings surface, Hermes
also stores defaults for untouched fields and can refuse activation after storing values. Secrets
stay masked, start empty when editing, and cannot be cleared here. `c` copies the displayed
`hermes -p <profile> memory setup <provider>` command through the terminal clipboard feature;
Talaria does not run setup or sign-in commands.

On a built-in file, `d` asks before deleting it. Anywhere in the overview, `D` asks before deleting
both `MEMORY.md` and `USER.md`. These deletes cannot be undone and do not touch external providers'
data. Empty files send no delete request. On the curator row, `p` pauses or resumes scheduled work; pausing leaves a
run already going alone. `R` checks for an existing run, then asks before starting a new one.
Talaria follows its last eight output lines, stopping after 240 polls, three failed polls, a
replaced process, or an unknown result. Closing the overlay stops following without stopping the run.

`s` opens `/config` filtered to `memory.`; `k` filters to `curator.`. Pending edits require a discard
decision before leaving. `Ctrl+R` reloads, and Esc closes the innermost editor, card or overlay.
Confirmation buttons accept Left, Right, Tab, Enter and Esc; Cancel starts selected. The fourteen
`memory_*` bindings in the [configuration reference](configuration.md) are rebindable outside editors. Printable keys remain literal editor
input. A dropped write is read back and never automatically resent.

## Scheduled jobs

`/cron` or `/cron list` opens scheduled jobs for the connected profile; `/cron new` or
`/cron add` opens the new-job form. The list includes paused jobs. Type to filter, use the
arrow keys to select a job, and press Enter for its details and recent runs. Tab switches
between details and run history; `o` opens a selected finished agent run when history has focus.

The `cron_*` bindings in the [configuration reference](configuration.md) apply on this screen. Ctrl+N opens the form and Ctrl+S saves it.
The form asks for a schedule and prompt, with an optional name and repeat count, and a delivery
target. Ctrl+E pauses or resumes the selected job; Ctrl+D asks before removing it. Ctrl+T asks
before running it now; running a paused job also resumes its schedule. Ctrl+R reloads. Esc
clears the filter or goes back, and asks before discarding a changed form. Run now and history
need Hermes's dashboard routes. The screen needs a terminal at least 60 columns by 12 rows.

## MCP servers

`/mcp` opens the connected profile's Model Context Protocol servers. `/mcp catalogue` browses
Hermes's catalogue; `/mcp apply` asks Hermes to apply saved changes. Tab switches lists, typing
filters, Enter opens details or an install confirmation, and Esc clears the filter before closing.

On Servers, Ctrl+T tests, Ctrl+E enables or disables, Ctrl+K saves a key, Ctrl+O starts browser
sign-in, Ctrl+D asks before removal, and Ctrl+N adds a remote or local server. Plugin servers are
read-only here. A local server runs a program on the gateway machine: add only programs you trust.
Keys and environment values are masked and saved through Hermes's key API, outside server config.

Saving affects new sessions. Ctrl+Y shows Hermes's warning before applying changes to the current
session; a busy session may need another apply after its turn ends. Ctrl+R refreshes the list
without applying anything. Removing a server keeps its saved key and sign-in in the profile.

Browser sign-in opens a link only when you press O; C copies it. Over SSH, P accepts the browser's
loopback callback address when the redirect cannot reach the gateway machine. The detail card
also gives the command for device sign-in. Talaria shows and copies these commands; it never
runs them. Catalogue entries that need extra values provide a terminal install command.

The `mcp_*` actions can be rebound under `[keys]`: `mcp_next_tab`, `mcp_prev_tab`, `mcp_reload`,
`mcp_add`, `mcp_test`, `mcp_toggle`, `mcp_set_key`, `mcp_sign_in`, `mcp_remove`, `mcp_apply`,
`mcp_save`, `mcp_open_link`, `mcp_copy`, and `mcp_paste_redirect`. These bindings apply only
inside the MCP screen.

## Connectors and messaging

`/connectors` and `/messaging` open two tabs for the foreground profile. Settings links to the
same screen through **Connectors and messaging**. `/platforms` keeps Hermes's own meaning.

Connectors use Nous Research's hosted tool gateway. Before the first connector call, Talaria
asks for consent: continuing may create a free Nous guest identity for a profile without a Nous
sign-in. Consent lasts for this process and profile, including reconnects. Opening Messaging
does not grant it. If connectors are unavailable, the screen offers
`hermes -p <profile> auth upgrade`; Talaria does not sign in for you.

Select a connector and press Enter for its accounts. `a` adds an account, `r` renames, `x` asks
before removing, and `h` shows retired accounts. A connection link is a credential: `o` opens an
HTTPS link and `c` copies it only when requested. Escape stops waiting without canceling the
link. During that wait Talaria polls every two seconds; lists otherwise refresh on changes or
`Ctrl+R`. Space switches a connector using the member policy layer. Organization and role locks
remain read-only. `t` opens tools, Space drafts a switch, and `Ctrl+S` shows the changes before
`y` saves them.

Messaging cards show credential presence as `set` or `not set`; stored previews are never shown.
Non-list fields use an empty masked editor. Enter keeps a new value; Escape preserves the earlier
draft. List fields show allowlist identifiers inside the overlay, split pasted lines or commas,
and remove duplicates. `d` marks a field to clear; `A` shows advanced fields. `Ctrl+S` reviews
changed keys and `y` saves. Space asks before turning a platform on or off. Saving does not restart
the gateway; when a restart is needed, `R` opens the doctor. `t` tests the selected platform.

On a waiting pairing row, `p` asks before approval. On a paired row, `x` asks before removing its
pairing approval. Other access rules may still admit that person. A request without a request ID
shows the command to run with its pairing code. Pairing names, account labels, and allowlist
identifiers stay inside the overlay.

One write runs at a time through its read-back. An uncertain result triggers a reload and never
an automatic resend. Drafts survive a refusal. A successful earlier save leaves newer edits in
place. Use Tab to switch tabs, `/` to filter, and Escape to close one layer. Editor letters remain
literal text; confirmation cards accept only their confirm, cancel, and close actions.
