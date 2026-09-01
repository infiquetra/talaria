# talaria-t2 event scripts

## Item 3 — Main hierarchy

`3-main-hierarchy.json` waits four seconds for the installed client to initialize, fixes the terminal at 144 by 36 cells, and runs `/bar` so StatusRegion has visible content before the capture settles for 2.7 seconds. Judge the capture by row placement: transcript and StatusRegion stay in the flexible body, Composer remains below them, HelpBar is directly above BottomStatusBar, and BottomStatusBar is the one-cell final row.

## Item 12 — All status segments

`12-all-status-segments.json` waits four seconds, resizes to the full-form breakpoint of 144 by 36, and leaves three seconds for the repaint to be captured. Run it with the default status configuration and judge that one final row contains `cwd`, `git_branch`, `agent_model`, `context`, `task_progress`, `connection`, and `version` in that order with exactly six one-cell separators and no wrap.

## Item 13 — Status configuration

`13-status-configuration.json` waits four seconds, resizes to 144 by 36, and runs `/bar` so the normalized active segment list and startup notice are visible for 2.7 seconds. At execution, first close a baseline process, then configure `connection`, `task_progress`, `context`, `agent_model`, and `git_branch` in that order, omit `cwd` and `version`, append one unknown name, and launch this restarted process; judge that the five recognized segments render in exactly that order, the two omitted segments do not render, and the unknown name produces a visible notice without suppressing recognized segments.

## Item 14 — Status responsive sequence

`14-status-responsive-sequence.json` waits four seconds and then applies every required width—144, 143, 120, 119, 112, 111, 96, 95, 80, 79, 64, 63, 48, 47, 32, 31, 20, and 19 columns—with one second between resizes for a complete synchronous layout and capture repaint, followed by a two-second final hold. Judge each recorded repaint against `_breakpoint`: forms compact first, segments drop in the specified order, 32 and 31 remain inside the same 20–47 band, connection remains, adjacent separators disappear with their segment, and BottomStatusBar stays one row without wrapping.

## Item 16 — Inspector dock and resize

`16-inspector-dock-and-resize.json` waits four seconds, sets 120 by 36, closes and reopens the default dock so focus enters it, then sends four xterm `Shift+Right` sequences and six `Shift+Left` sequences; 0.75 seconds separates resize actions and one second separates direction changes so each four-column repaint is visible. Judge the border titles and body data: widths progress 36→40→44→48 and clamp on the extra widen, then 48→44→40→36→32→28 and clamp on the extra shrink, while the panel stays right-docked and its session-derived rows do not change.

## Item 17 — Inspector content and empty states

`17-inspector-content-and-empty-states.json` waits four seconds, sets 132 by 36, closes and reopens the 36-column dock to focus it, widens through 40 and 44 to 48 columns, shrinks through every four-column step to 28, restores 36, and then walks three focusable task/file rows. The 0.75-second resize spacing and two-second holds at both bounds expose the cycle-2 auto-height wrapping at each required width. Run the same script once with held session state containing tasks, context, at least one changed file, and operation details, then once in a fresh session containing none of them; judge that all four headings remain, populated values match the held session, the complete `[none available from this session]` sentence wraps honestly at widths 28, 36, and 48, and opening or resizing the inspector causes no new gateway request or filesystem scan.

## Item 18 — Inspector responsive state

`18-inspector-responsive-state.json` waits four seconds, sets 120 by 36, reopens and widens the focused dock to 40 columns, then gives each breakpoint change at least one second to settle while driving open 120→119→120, manually closed 120→119→120, and a 119-column overlay opened with `ctrl+b` and closed with Escape before returning to 120. Judge that auto-collapse restores only the open preference, manual close survives its round trip, the overlay title is present without transcript reflow, and the final 120-column frame remains closed because the manual-close preference is preserved; rerun the script in a new process and use its pre-action frame to prove process geometry reset to the 36-column expanded default.

## Item 19 — Side-by-side diff

`19-side-by-side-diff.json` waits four seconds, sets 132 by 36 so the default inspector is docked, opens the held diff with `/diffs`, and holds the modal for 2.7 seconds before closing it and cleanly quitting. Use a session-reported change containing base and working-tree line numbers, multiple hunks, syntax-bearing content, and an intraline replacement; judge two aligned panes, `+`, `-`, and `@@` markers, syntax and intraline treatment, file/hunk position, `read only` in the header and hint, temporary inspector collapse while the modal is open, and inspector restoration after Escape.

## Item 20 — Unified fallback

`20-unified-fallback.json` waits four seconds, opens a diff at exactly 112 columns, advances a hunk and pages down twice, then allows at least one second after each 112↔111 resize while exercising the automatic fallback, a refused `s` below threshold, explicit `u`/`s` preference changes, and restoration at 112. Use a held diff with at least two hunks and enough rows to scroll; judge that file, hunk, and visible source anchor survive mode changes, 111 columns forces unified, 112 restores the side-by-side preference, and the refusal text replaces the existing one-row header rather than growing the layout.

## Item 21 — Diff navigation and boundary

`21-diff-navigation-and-boundary.json` waits four seconds, leaves the exact `/diffs` palette filter visible for one second, submits it, cycles next/next/previous hunk and next/next/previous file at 0.75-second intervals, opens the file picker and selects its previous row, moves horizontally across a clipped line, and finally sends unbound `e`, `a`, `r`, and `d` keys before quitting. Use held changes with at least two files, two hunks in the first file, and an overlong line; judge deterministic wraparound, the file picker and read-only hint text, a clipped rather than wrapped long line with bounded horizontal movement, stable data after unbound keys, and no edit, stage, revert, discard, apply, or other writing command in the palette, key hints, or reachable behavior.

## Item 29 — Wide screenshot

`29-wide-screenshot.json` waits four seconds, sets the required 132 by 36 geometry, closes and reopens the default inspector to guarantee a focused 36-column dock, and holds the final frame for three seconds. Capture the settled frame before `ctrl+q` and judge it against the wide mockup: the main body and right inspector hierarchy match, the inspector is 36 columns, HelpBar and BottomStatusBar each remain one row, and all seven default status segments use compact forms.

## Item 30 — Narrow screenshot

`30-narrow-screenshot.json` waits four seconds, sets 78 by 36, briefly opens the inspector overlay and closes it after 1.5 seconds, then opens the held diff and holds the unified view for 3.2 seconds. Capture the settled diff frame before Escape and judge that the inspector cannot dock, its overlay does not reflow the transcript, only `agent_model`, `context`, `task_progress`, and `connection` remain in the one-row status bar, the HelpBar does not wrap, and the diff is unified below 112 columns.

## Item 32 — Session-only status toggle

`32-session-only-status-toggle.json` waits four seconds at 144 by 36 so the configured `cwd` segment is visible, runs `/bar cwd`, and leaves 3.2 seconds to capture its immediate removal and the visible session-only notice. At execution, record the scratch config content and modification time before and after this drive, then launch a fresh process and capture its pre-action frame (or rerun the script and inspect the four-to-six-second interval); judge that the file bytes and timestamp never change and that restart restores `cwd` before the command hides it again.
