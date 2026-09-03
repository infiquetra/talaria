# Terminal UI

Talaria 0.5.0 adds a responsive true-bottom status bar, a right inspector, and a strictly read-only
diff viewer. Each projects state the process already holds; opening these surfaces does not add a
gateway request, a filesystem scan, or a polling loop.

## Screen layout

The transcript body and optional inspector share the main area. Below them are the composer, the
one-row help bar, and the one-row bottom status bar. The older multi-row `StatusRegion` remains
inside the body for configured command output and notices; it is not the bottom bar.

## Bottom status bar

The default segment order is:

1. `cwd`: the launch working directory;
2. `git_branch`: the launch Git branch, or an honest unknown/detached value;
3. `agent_model`: the provider and model already held for the current session;
4. `context`: held input/output token counts against the held context-window size;
5. `task_progress`: subagent completion/total and the needs-you queue's literal `!N` attention count;
6. `connection`: a non-color state such as `[ok]`, `[..]`, `[~]`, `[x]`, or `[!]`;
7. `version`: the running Talaria version.

The working directory and Git branch are captured once when the app is built. The other values come
from the serialized render-boundary snapshot. The bar never reads a transport object or polls for
fresh state.

`/bar` lists the currently enabled segments. `/bar SEGMENT` toggles one known segment for this
process and does not edit configuration. The command is deliberately not `/status`, because Hermes
owns that command name. Configure persistent order, visibility, caps, and the separate status-command
region as described in [Configuration](configuration.md).

### Responsive status bar

The bottom bar is always exactly one row and never wraps. Widths are terminal columns after Textual
has handled the resize.

| Width | Default result |
| ---: | --- |
| 144 and wider | All seven segments in full form |
| 120–143 | All seven segments in compact form |
| 96–119 | Drop `version`; compact remaining segments |
| 80–95 | Also drop `cwd` |
| 64–79 | Also drop `git_branch` |
| 48–63 | Also drop `context` |
| 20–47 | Also drop `agent_model` |
| Fewer than 20 | Drop `task_progress`; minimum `connection` form, clipped only as a final safeguard |

Within a band, overflowing values shorten and then drop by fixed priority: `version`, `cwd`,
`git_branch`, `context`, `agent_model`, and `task_progress`. `connection` is never deliberately
dropped. Separators next to a dropped segment disappear with it.

At 20–47 columns, `task_progress` and `connection` start compact. Actual overflow may shorten or
drop `task_progress`; below 20 columns the breakpoint drops it and keeps minimum `connection`.

## Right inspector

`Ctrl+O` or `/inspector` toggles the inspector. `Ctrl+B` was the previous default; Herdr
captures it before Talaria sees it when nested, so it is documented as replaced rather than
bound. At 120 columns and wider it docks on the right. It
starts at 36 columns, resizes in four-column steps, and clamps to 28–48 columns. While focus is in a
docked inspector, `Shift+Left` narrows it and `Shift+Right` widens it.

Below 120 columns, an open inspector auto-collapses without changing the requested dock state.
Toggling it opens an overlay rather than reflowing the transcript. From 32–119 columns the overlay
uses the saved width or terminal width minus two, whichever is smaller. Below 32 it uses the full
terminal width. `Escape` closes the overlay and restores the prior focus. Widening back to 120
restores a panel that was requested open; a manually closed panel stays closed.

The four sections are always present:

- Tasks
- Context
- Changed files
- Operation details

They are derived from the current session's held transcript, queue, model/context, connection,
session, and tool-change state. A section with no state says `[none available from this session]`
instead of disappearing or inventing data. The empty-state row wraps to two rows at inspector widths
28 and 36 and fits on one row at width 48, keeping the complete sentence visible at every supported
width. `Up` and `Down` move among task and changed-file rows. `Enter` on a changed file opens its held
diff.

Width, requested open/collapsed state, automatic narrow state, overlay state, current row, and file
selection live only in the running process. Restart restores the default geometry. A full-screen
diff temporarily hides a docked inspector without changing its requested state.

## Read-only diff viewer

Open the viewer with `/diffs` or `Enter` on an inspector changed-file row. It renders only unified
diff text already reported in the current session and retained by Talaria. It does not read Git,
open working-tree files, call the gateway, or write anything.

The preferred mode starts side-by-side. At 112 columns and wider that preference is honored; at 111
or fewer the effective mode is forced to unified without overwriting the preference. Widening back
to 112 restores side-by-side. Pressing `s` while too narrow leaves unified active and says that 112
columns are required in the existing header. `u` selects unified at any width.

The viewer indexes the held diff once. Each render is bounded to a viewport window plus fixed
overscan; intraline comparison is limited to visible paired lines and capped source lengths. Long
lines do not wrap, preserving old/new line correspondence. An empty held document says
`no session-reported changes`.

The boundary is strict: there is no edit, stage, unstage, apply, revert, discard, checkout, or other
working-tree mutation command, key, or helper. The header and key hint both say `read only`.

Diff preference, effective narrow fallback, file/hunk selection, and viewport anchor are session
state only. They are not written to configuration.

### Diff keys

| Key | Action |
| --- | --- |
| `n` / `p` | Next / previous hunk |
| `N` / `P` | Next / previous file |
| `f` | Open the held changed-file list |
| `s` | Prefer side-by-side mode |
| `u` | Prefer unified mode |
| `Escape` | Close the viewer |

## Global bindings

These are the bindings shipped by the application. Slash commands are the reliable primary route
for palette surfaces on macOS. Function keys are either replay-specific controls or secondary
aliases where the desktop delivers them.

| Key or route | Action | Availability |
| --- | --- | --- |
| `Ctrl+Q` | Quit the client | live and replay |
| `Ctrl+O`, `/inspector` | Toggle inspector | live and replay |
| `/diffs` | Open held diffs | live and replay |
| `/` or `F3` | Open/list commands | live and replay |
| `Ctrl+G` or `F2`, `/agents` | Toggle subagent rows | live and replay |
| `Ctrl+S` or `F4` | Cancel the in-flight turn (never quits the client) | live; visibly refused in replay |
| `End` or `F5` | Follow the newest transcript line | live and replay |
| `/models`; `F11` from every focus; `F6` only outside composer focus | Open models | live; gateway-changing actions are refused in replay |
| `/profiles`; `F12` from every focus; `F7` only outside composer focus | Open profiles | live; gateway-changing actions are refused in replay |
| `F8` | Pause/resume playback | replay only |
| `F9` / `F10` | Slower / faster playback | replay only |

`F1` has no Talaria action. The shipped help bar reports `F1` and `F2` as eaten on macOS before the
application receives them; `Ctrl+G` is the primary subagent-row binding for that reason.

The inspector toggle and the turn-cancel chord are configurable because no single default
survives every terminal multiplexer: see the `keys` table in [Configuration](configuration.md).
The help footer always labels cancel-turn beside quit-client, so the two can never be mistaken
for one another. `Ctrl+C` left the interrupt action; pressed out of habit it reaches the text
area's copy binding or the framework's quit hint, never the turn and never the exit.

## Focus, motion, and scroll

Focus changes alter existing border cells and reserved `>` gutters without mounting a row or
changing widget height. Transcript kinds retain both a literal label/gutter and a dedicated
background, and connection, task, approval, and diff states retain glyphs or words when color is
unavailable.

With `ui.reduced_motion = true`, nonessential spinners use a static `[..]` form. All scrolling jumps
directly to the same destination, including scrolling driven by arrow keys, Page Up, Page Down, Home,
and End. Protocol progress and elapsed-time text still update.

When the transcript is following the bottom, new content stays followed. When the operator has
scrolled away, render updates preserve the held entry identifier and source-line offset as closely
as wrapping permits. `End` or `F5` explicitly returns to follow-bottom mode.
