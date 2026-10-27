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
