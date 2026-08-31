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

## 31. Malformed Visual Studio Code import

Run `31-malformed-visual-studio-code-import.json` with `--accept-exit 2 --expect "talaria: theme
import failed:" -- theme import <worktree>/tests/fixtures/vscode-themes/malformed.json --name
malformed-acceptance` against a clean scratch themes directory. This command is non-interactive, so
the event array is intentionally empty and the installed command exits on its own; judge the raw
capture for the strict-JSON failure and the pseudo-terminal result for child exit 2, then confirm
that `TALARIA_CONFIG_DIR/themes/malformed-acceptance.json` did not exist before or after the drive.
