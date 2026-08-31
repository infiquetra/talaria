# talaria-t1 event scripts for Talaria v0.5.0

Pass each JSON file to `uv run python -m scripts.acceptance.v050_pty_driver --event-script
<file>` with the installed-artifact receipt and tester `talaria-t1`; user-interface legs use a
36-row by 132-column pseudo-terminal unless the item receipt says otherwise. The four-second launch
interval lets a real Hermes-backed session and the first Textual frame settle, theme previews remain
visible for at least 1.25 seconds, modal or final-theme frames remain visible for at least two
seconds, and every interactive leg exits through `CTRL_Q`. Command-line import legs use the same
item script: the installed command exits before its first scheduled user-interface event.

## 4. Refined Default

Run `04-refined-default.json` from a clean item configuration with Refined Default active and a wide
session that visibly contains transcript, Composer, status bar, and the docked inspector. The drive
opens the theme dialog, previews Dark Green Terminal, moves back to Refined Default, accepts it, and
waits before quitting; judge the accepted final frame and the earlier picker frame for a light,
readable palette with themed text, borders, focus, status glyphs, and inspector surfaces rather than
stock Textual colors.

## 5. Dark Green Terminal

Run `05-dark-green-terminal.json` from the same clean Refined Default baseline. The drive opens the
theme dialog, highlights Dark Green Terminal long enough to preserve its immediate preview, accepts
it, and holds the complete wide interface for three seconds; compare the pre-preview and final raw
frames and require transcript content, the Composer focus marker, palette rows, status glyphs, and
inspector content to remain present and readable in the dark green palette.

## 6. Neutral Dark

Run `06-neutral-dark.json` from the clean Refined Default baseline. The drive pauses on Dark Green
Terminal, advances to Neutral Dark, accepts it, and holds the final wide frame; judge that the final
transcript, Composer, picker/dialog, status bar, and inspector all use the low-saturation palette,
and compare screen geometry before and after the previews to ensure rows, columns, and surface bounds
do not move.

## 7. Accessible High Contrast

Run `07-accessible-high-contrast.json` in a wide real Hermes-backed session whose held transcript
already includes a completed operation with a unified diff, so `/diffs` opens actual additions,
removals, context, and syntax rather than an empty modal. The drive previews each intervening built-in,
accepts Accessible High Contrast, opens the read-only diff for four seconds, closes it with Escape,
and quits; judge focus, selection, transcript groups, status, inspector, diff boundaries and marks,
then use the captured runtime colors for the required 6.22:1 text and 6.08:1 non-text measurements.

## 8. Preview cancellation

Run `08-preview-cancellation.json` with Refined Default active and an existing scratch
`TALARIA_CONFIG_DIR/config.toml` whose bytes, modification time, and hash have been recorded. The
drive previews Dark Green Terminal and Neutral Dark for 1.5 seconds each, presses Escape, holds the
restored Refined Default frame, and quits; require both immediate preview transitions, an exact
visual return to the pre-picker theme, and byte-for-byte, timestamp-for-timestamp equality of the
scratch config before and after the drive.

## 9. Explicit save and precedence

Item 9 is a three-launch script set because the landed configuration and theme registry are loaded
only at process start: run `09-explicit-save-and-precedence-user.json` first from the built-in default
to select Dark Green Terminal and save user scope, restart in the same scratch configuration and
working directory with `09-explicit-save-and-precedence-repository.json` to observe the user winner,
select Neutral Dark, and save repository scope, then restart with
`09-explicit-save-and-precedence-session.json` to observe the repository winner and select Accessible
High Contrast without saving it. Inspect the user and repository TOML before and after every leg;
the evidence passes only if unrelated bytes stay exact, only the two explicit-save legs change their
respective `[theme]` table, and the last session selection changes neither file.

## 10. Theme fallback notice

Begin item 10 with an empty scratch `themes/` directory, then run the installed command with
`10-theme-fallback-notice.json -- theme import
<worktree>/tests/fixtures/vscode-themes/unsupported-dark.json --name partial-fallback`; retain that
command-line capture because it must list all 56 Refined Default fallback tokens and all 19 warnings.
Next put `name = "not-installed"` in the scratch config's `[theme]` table and run the same script with
no Talaria command arguments in a fresh user-interface process: it shows the unknown-theme startup
notice, opens the four built-ins plus `partial-fallback`, selects the imported theme, holds the frame,
and quits. Judge the two captures together for the named unknown-theme error, every filled token,
and a visible imported result rather than any silent substitution.

## 11. Visual Studio Code import

Begin item 11 with an empty scratch `themes/` directory and use the installed command plus
`11-visual-studio-code-import.json -- theme import
<worktree>/tests/fixtures/vscode-themes/unsupported-dark.json --name vscode-import-evidence` twice,
with distinct capture/result paths and a hash of the stored theme after each run. The fixture supplies
supported `editor.background` and `comment` values, unsupported color paths, unsupported
`fontStyle`, and missing Talaria extension tokens; require all 19 path warnings, all 56 fallback
lines, and identical stored bytes. Run the same script a third time with no command arguments in a
fresh process whose held diff contains a comment line: it selects the sole imported theme, opens the
diff, and lets the capture show the mapped canvas and comment syntax values before exiting cleanly.

## 22. Composer caret location

Run `22-composer-caret-location.json` at 36 rows by 132 columns against a settled session containing
transcript content, exactly one interruptible agent row, one free-text prompt control, and at least
one inspector task or changed-file row; a sanitized frame log recorded from a real Hermes session
may be run as `replay <corpus> --speed 0` to establish that fixed state. Shift+Tab first reaches the inspector,
then the forward Tab sequence returns through Composer, transcript, agent, prompt region and prompt
control with 1.5 seconds per focus frame. Judge every transition for the `compose [*] caret here` or
`compose [ ] caret elsewhere` title, active border and reserved `>` cues, while comparing Composer
height and top row, transcript height and anchor, HelpBar row, and BottomStatusBar row for exact
invariance before the clean exit.

## 23. Connection non-colour states

Run `23-connection-non-colour-states.json` only as a controlled live Hermes-backed leg, rendered by
a real terminal configured for a monochrome capture. Coordinate the throwaway gateway so the initial
launch visibly moves from connecting to connected, stop it after the eight-second resize so
reconnecting and then disconnected settle, and restart it with the tester scratch credential made
stale before the 18-second narrow frame so authentication failure settles before the final wide
frame; never alter the operator's real credential. The widths expose full, compact and minimum
forms, and the capture passes only if `[ok]`, `[..]`, `[~]`, `[x]`, and `[!]` remain paired with the
honest state text rather than relying on colour; the script leaves fourteen seconds after returning
wide before `CTRL_Q` so the terminal state is bounded and readable.

## 24. Agent and queue non-colour states

Run `24-agent-and-queue-non-colour-states.json` with `replay <corpus> --speed 0.25`, where the corpus
is a sanitized recording from a real Hermes session whose timed plateaus expose queued, running,
completed, error, failed, interrupted and timeout agent rows between the four `/needs` dialogs, and
whose queue plateaus at those openings are respectively empty, waiting, blocked and possibly
duplicate. The 2.5-to-3-second dialog holds make complete queue detail readable; the 31-column leg
drops `task_progress` before the final `/needs` opening, while the preceding wide waiting frame must
show its literal `!N` attention count. Judge all seven fixed agent glyph-and-word forms plus `[ok]`,
`[!]`, `[x]`, `[?]` and `[..]` queue forms in the raw capture, then require the same full drill-down
detail at narrow width before the script restores 132 columns and exits.

## 25. Transcript identity without colour

Run `25-transcript-identity-without-colour.json` in a monochrome real-terminal profile against a
maximum-speed replay of a sanitized real Hermes recording containing six short consecutive entries:
operator, assistant, reasoning, tool or subagent activity, system or prompt session record, and fault.
All six must fit together in the settled 36-by-132 frame before the eight-second clean exit. Judge
the first rows for `> You`, `A Talaria`, `. Reasoning`, `$ Tool/Subagent`, `- Session`, and `! Error`,
the reserved gutter shape, and the absence of blank spacer rows; runtime theme evidence remains tied
to the visual specification's six transcript marker/background token pairs and contrast figures.

## 26. Reduced motion

Item 26 is two drives of the same `26-reduced-motion.json` events and the same controlled live
Hermes-backed session: first prepare scratch `TALARIA_CONFIG_DIR/config.toml` with a `[ui]` table
whose `reduced_motion` value is `false`, then change that value to `true` and restart because the
landed setting has no command-line or environment override and never reloads in-process. Each drive
needs a long transcript, one visible working or waiting state, populated agents, and an externally
coordinated gateway stop/restart between seconds 15 and 23. The mouse-wheel and F5 events compare scroll motion,
the picker previews and cancels a theme, the agent toggle preserves current state, and the gateway
bounce proves reconnection updates continue; compare captures for ordinary `working…` versus static
`[..] working`, immediate reduced scrolling and single repaint transitions, with no lost elapsed-time
or connection updates, and record the exact scratch config bytes used by each restart.

## 27. Stable unpinned scroll

Run `27-stable-unpinned-scroll.json` as `replay <corpus> --speed 0.25` with a sanitized frame log
created by the installed `talaria record` from a real Hermes session, not a hand-authored or mocked
frame sequence. The recording must fill at least three transcript viewports, place a distinctive
safe entry and source-line offset near the middle, continue appending after second 5, and carry an
agent update while a configured two-second bounded status command refreshes StatusRegion. The three
`hex_bytes` actions are real XTerm SGR mouse-wheel-up events at transcript column 20, row 8, which is
the landed path that releases follow state while leaving Composer focus usable; the drive then
collapses/expands agents, previews and cancels a theme, docks/undocks the inspector, and resizes
132→119→132 with settle time after every layout change. Pass only if the distinctive entry and
source offset remain on the same screen row throughout and no append or chrome repaint jumps to the
bottom before the clean exit.

## 28. Stable pinned scroll

Run `28-stable-pinned-scroll.json` against the same kind of long, still-streaming sanitized real
Hermes recording at `--speed 0.25`, with safe visible lines that identify both the middle anchor and
the newest bottom entry and with the same two-second StatusRegion refresh. Two real mouse-wheel-up
events first move away from the bottom, F5 at second 6 establishes follow mode before later appends
and the 132→119→132 resize, and the single wheel-up event at second 13 exercises the landed manual
scroll path that clears follow state; subsequent corpus frames must append while that reading row
stays put until F5 at second 17 follows the newest bottom again. Judge the capture for predictable
bottom following while pinned, a stable non-bottom row after manual scrolling, continued status
updates in both states, and a clean exit after the final bottom frame settles.

## 31. Malformed Visual Studio Code import

Run `31-malformed-visual-studio-code-import.json` with `--accept-exit 2 --expect "talaria: theme
import failed:" -- theme import <worktree>/tests/fixtures/vscode-themes/malformed.json --name
malformed-acceptance` against a clean scratch themes directory. This command is non-interactive, so
the event array is intentionally empty and the installed command exits on its own; judge the raw
capture for the strict-JSON failure and the pseudo-terminal result for child exit 2, then confirm
that `TALARIA_CONFIG_DIR/themes/malformed-acceptance.json` did not exist before or after the drive.
