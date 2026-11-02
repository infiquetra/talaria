# Talaria layout study: optional right panel and top menu bar

Talaria's default conversation should stay clean and light. This study recommends **Option A: a one-line menu and a compact, unified work panel**, with both off by default. This is a screen-review proposal for #473, not approval to implement the layout or the voice capabilities in #424, #427 and #428.

Operator guardrail: “The interface is good, clean and light; preserve that in future updates.”

Studied baseline: `3f6ea58ed8c3da9742b64a8e59a4d9c91e01a2bd`. Prototype branch: `study/473-layout-panel-menubar`. The prototype is an offline example in `crates/talaria-tui/examples/layout_study.rs`; application startup, key handling, transport, audio, and terminal mode ownership are unchanged. No PR, merge, release, or build card accompanies this study.

## Options and recommendation

| | Option A: compact work panel | Option B: tabbed inspector |
|---|---|---|
| Top menu | One line, no permanent border | Two lines: session identity, then commands |
| Wide panel | 32 content columns plus one divider | 40 content columns plus one divider |
| Panel contents | Goal, turn/verdict, tasks, agents together | Tasks and Agents tabs; one section at a time |
| Narrow layout | Temporary right overlay; underlying text keeps its width | Inspector becomes an 11-line bottom section |
| Benefit | One glance shows task and delegation context | Longer task text; more detail per selected tab |
| Cost | Short labels; details need the existing views | More navigation and less conversation height at 80x24 |

**Recommend Option A.** Its one-line bar costs one row. Its panel answers “what is running, what remains, who needs me?” without tabs. Option B uses two top rows and an 11-row inspector on an 80x24 screen, leaving seven rows for conversation above the inspector and four for status/composer. That is visibly heavier. Neither option adds decoration, a second status bar, a voice toolbar, artificial percentages, or a dashboard on startup.

## Q1: panel contents, ordering, and existing views

Option A shows the focused session only, in this order:

1. **Attention count**, followed by the current **goal**, **running turn/activity**, and **verdict**. The sample says `Turn: running / compare layouts` and `Verdict: pending`. A completed checklist is not evidence that a goal or turn succeeded. Show the explicit goal verdict when available; otherwise say `unknown` or `pending`. Keep the last explicit verdict labelled as belonging to the previous turn if a new turn has begun.
2. **Tasks**, using the existing checklist projection, counts, hierarchy, and completed/in-progress/pending/cancelled states. Keep the focused unfinished item in the visible window; count overflow as `+N more`. Turning the right panel on hosts the checklist here instead of duplicating the live checklist above the composer. Turning it off restores the previous inline checklist preference. Ctrl+T continues to expand/collapse these same tasks in whichever location hosts them. It does not toggle the whole right panel. This preserves the checklist/goal correctness work associated with #463 and #464.
3. **Subagent activity**, with waiting requests first, then running, queued, and a bounded recent-completion summary. Each child gets its short name/goal and state, followed by model and effort. Missing model or effort is visibly unknown, never inherited from the parent by assumption. The existing dock's activity and counts can supply summaries; the full Alt+S panel still owns details, tails, steering and stop confirmation. Alt+A retains “next waiting request.” Suppress the duplicate inline dock while its summary is hosted in the right panel.

The captures use a deliberately mixed example: a known model/effort and another child with unknown values. Their values and work are synthetic. Per-child effort is not a field on the current `SubagentRow`; a future build must obtain it from an authoritative source or keep `?`. No new protocol claim is made. Use an activity label for indeterminate work. Only show measured progress when a real numerator and denominator exist. No progress bars are recommended for the initial layout.

At 80x24, 19 body rows remain with the one-line bar and four footer rows. The panel uses 17 of them for goal, checklist and agents. More items require an explicit overflow count and existing detail views. The panel has no silent scrolling or hidden second copy of the tasks.

Option B uses the same state and shortcuts but separates Tasks and Agents into tabs. The extra captures show its Agents tab, including model and effort. Its narrower-screen Tasks header still exposes the attention count even while the Agents tab is hidden.

## Q2: top bar, contents and navigation

Option A uses exactly **one row**: `Talaria   Session   Model   View   Voice   Settings   Help`. It spans the whole terminal, including the panel, so the two additions read as one layout. It carries navigation only; model status and voice phase stay in the footer.

Menus reuse existing actions: Session opens session selection/fleet navigation; Model opens model/effort selection; View contains right-panel and bar toggles, tasks, agents and transcript; Voice contains record/stop and read-aloud settings; Settings opens Talaria settings and only available Hermes configuration actions; Help opens shortcuts. Unsupported backend operations are disabled with a reason. Configuration is an existing overlay, not a configuration form squeezed into the bar. Any destructive action retains its existing confirmation.

Alt+I toggles the bar. When showing it, focus its first menu. Once shown, `/layout menu` focuses it again without changing visibility; if hidden, this command opens a transient menu and leaves the bar preference off. Left/Right switch headings; Down or Enter opens a dropdown; Up/Down select; Enter activates; Tab/Shift+Tab move headings. Esc first closes the dropdown, then returns focus to the composer. Escape from a menu never interrupts a running turn. Plain letters are consumed only while a menu is focused; the composer keeps all its editing keys otherwise. These are proposed interactions; the example renders the focused menu but does not implement input dispatch.

Option B adds a session-identity line above that same navigation row. Its panel tabs use Tab/Shift+Tab when the inspector is focused; Up/Down select rows and Enter opens the existing detail view. Both options preserve the draft and composer selection when moving focus.

## Q3: proposed toggle keys and collision boundaries

| Action | Proposed key | Keyboard fallback |
|---|---|---|
| Show/hide right panel | Alt+D | `/layout panel` then Enter |
| Show/hide top bar | Alt+I | `/layout bar` then Enter |
| Focus menu without toggling preference | No extra global chord | `/layout menu` then Enter |
| Checklist expand/collapse | Existing Ctrl+T | Existing task view |
| Full subagent controls | Existing Alt+S | Existing agents view |
| Next waiting request | Existing Alt+A | Existing attention navigation |
| Start/stop push-to-talk | Existing Alt+R | Existing voice command |

Alt+D and Alt+I are absent from the baseline `docs/keybindings.md` and default key declarations. They avoid Ctrl+B, the documented default prefix for tmux and Herdr, and the cmux terminal multiplexer prefix. The published cmux desktop shortcuts use Command-modified combinations for the corresponding application actions. Herdr-owned text dialogs can use Alt+D for word deletion; Talaria must receive it only while its terminal pane has focus. Multiplexer dialogs retain their keys. Public references checked: tmux Getting Started, Herdr Configuration, cmux Keyboard Shortcuts and terminal multiplexer documentation.

This is a default-key comparison, not a guarantee about arbitrary personal overrides or terminal Option-key translation. Make both actions configurable; detect conflicts with Talaria's effective keymap rather than silently shadowing another action. The slash-command fallbacks require no new modifier chord and remain available if an outer terminal consumes an Alt key. Do not rebind the multiplexer, consume its prefix, or claim that the offline example tested physical key delivery. Commands in this table are design proposals, not commands already implemented on the baseline.

## Q4: exact narrow-width behavior

For Option A, dock only at **at least 112 columns and 28 rows**. This gives a minimum 79-column conversation beside the 33-column panel allocation. At 120x40 the conversation is 87 columns, the panel content is 32, and the divider is one. The bar is one row; status and composer use four. The other 35 rows form the body.

Below either threshold, the panel preference remains enabled but the panel is an **on-demand modal overlay**, not a forced split or a new bottom section. At 80x24 it covers columns 48–80 of the 19-row body; underlying conversation retains its full 80-column layout. The ON captures show this overlay actively open. Esc dismisses it without interrupting work and leaves the preference enabled; Alt+D or `/layout panel` turns the preference off. While dismissed it consumes no rows or columns. Attention remains in the status row. Only an explicit panel action reopens it; voice or background events never reopen it.

When a docked panel becomes narrow during resize, dismiss it into this on-demand state and keep focus in the composer; never cover the operator's draft automatically. Growing back to the threshold restores the dock if its preference is still on. The example draws static open/docked states and checks boundary dimensions; resize events and focus restoration are not implemented or demonstrated.

At fewer than 16 rows, neither option reserves panel space; use the existing full-screen tasks/agents views. A future bar at fewer than 60 columns becomes a single `Menu` entry; below 16 rows it is transient only. These smaller-screen policies are proposed, outside the requested capture sizes. Option B instead moves its inspector below the transcript at widths below 112 or heights below 28, provided at least 16 rows remain in the terminal.

## Q5: one voice location, independent of optional layout

The **existing full-width bottom status row below the composer** is the sole location for voice phase, read-aloud preference and target. The baseline draw order in `crates/talaria-cli/src/run.rs` places status last; the prototype preserves that order. It remains outside the panel and menu overlay and is present with both additions off. Keep the existing priority: connection/attention before voice, then lower-priority model/context/cost details. Drop lower-priority details before losing the voice phase and target. Voice state is text, not color alone.

Examples in the captures are `REC 0:07 +read -> Hermes:design`, `transcribing… +read -> Hermes:design`, and `SPEAK +read -> Hermes:design`. Supplementary screens show `Herdr:review` and `ACP:review`, where ACP means Agent Client Protocol. The backend prefix is always explicit. Session names are short display aliases, not addresses or raw session identifiers. A shortened name must retain a disambiguating suffix; details reveal the full safe display name. `+read` means read-aloud enabled; `SPEAK` means audio actually playing. Neither substitutes for the other. Idle read-aloud can show `voice +read -> target`; no voice capability means no voice segment.

Recording pins `(backend, session, responder)` at capture start as required by #428. Show that pinned target through transcription, even if focus changes. A focus change must downgrade delivery to a draft and mark the target `draft`; it must never relabel the in-flight recording as belonging to the new focus. Permission speech stages a choice and requires Enter confirmation. An unresolved/disconnected target is explicit and blocks direct delivery. Speaking identifies the actual playback/announcement target; it clears immediately when client playback ends or is interrupted. Alt+R barge-in stops client playback before capture. Gateway fallback keeps its documented “already speaking will finish” limitation and must not falsely claim cancellation. These rules depend on #425–#428; this study implements none of them.

The current `voice_view` recording/transcription projections and `StatusSection` render the fixtures. `SPEAK` and backend targets are explicit prototype-only projections because the baseline has no Speaking phase or unified voice target in `VoiceView`. Partial transcription can continue using the existing transient `heard:` line when there is text; it is content feedback, never a new persistent voice surface. Do not add a voice badge to either top bar or right panel later.

## Q6: defaults and terminal architecture decision

**Both preferences default to off**, independently. The OFF captures retain the inline checklist and the native conversation arrangement, with voice in the same footer. Users may explicitly enable either addition alone; supplementary captures show both mixed combinations. Preferences belong to the client view, not to a gateway session or a voice backend.

Architecture decision record 0009 commits finished output once to native scrollback, with a full-width live footer. A fixed screen-top menu and a persistent side panel beside historical text cannot be layered over that stream by adding a `LiveSection`. The recommended interpretation is an **opt-in alternate-screen workspace** whenever either persistent addition is enabled; a transcript projection in that view is temporary, with native scrollback preserved on return. Turning both off restores the inline terminal. Existing full-screen overlays return to the workspace if enabled, otherwise inline mode. Focused menus/panels consume keys before the composer. All terminal mode/escape-sequence work remains in the terminal module.

This explicitly revisits record 0009's persistent-layout boundary. The operator must accept that tradeoff alongside the screens before a build card is filed. If preserving native scrollback interaction while either addition is on is mandatory, reject this recommendation and study terminal-owned splits separately. Do not silently convert the default transcript to a retained full-screen widget tree. The TestBackend frames prove cell placement only; they do not prove terminal switching, historical transcript projection, copy/search behavior, cancellation latency, routing, input handling, or performance.

## Evidence and reproduction

Inputs: issues #473, #424, #427 and #428; `docs/keybindings.md`; `docs/agents/patterns.md`; architecture decision records; checklist, subagents, status, voice, composer, live and terminal modules. Hermes contract pin: v0.22.4, revision `4d04303818b61bf3aa09278af48356315515965b`; the vendored contract includes session reasoning-effort and checklist state. No Hermes code or contract is changed.

With the required shared Cargo target directory exported in the shell, run `cargo run -p talaria-tui --example layout_study` to reproduce the text below. The example uses ratatui `Terminal<TestBackend>` at each real dimension, reuses the production composer, checklist and status renderers, and supplies only the proposed menu/panel arrangement and synthetic state. Both alternatives use the same fixture. The OFF frames represent the visible native conversation with historical text supplied as fixture content, not an emulator recording of the inline terminal.

There are 24 required frames: two options × two sizes × both additions ON/OFF × three voice phases. Eight supplementary frames cover independent toggles, focused menu overlays and the alternate Agents tab. Trailing blank cells are omitted exactly as in the patterns guide; right-pad to the stated width to recover every cell. All 24 or 40 rows are retained, including blank rows. The generated captures are appended without editing their cell text.

## Local verification and limits

The two `layout_study` example tests passed. They check the required matrix's dimensions, voice/target visibility, composer visibility, and panel geometry at boundary sizes. The example ran successfully and generated all 32 captures. Final `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` both passed on the frozen source. Study files remain local and uncommitted on the named branch.

The broader workspace check is **not green**. `cargo test --workspace` completed groups reporting 3,893 passed and 148 ignored, then failed compiling a Hermes client documentation example with error `E0463`, unable to find `tokio_tungstenite`. A subsequent isolated `cargo test -p hermes-client --doc` passed its one test. That retry does not change the original command's failed outcome, and the original error's cause is unestablished.

`cargo llvm-cov --workspace --ignore-filename-regex /generated/ --fail-under-lines 80` exited before producing a workspace line-coverage percentage. The benchmark test `sampler_reports_cpu_and_rss_of_a_child` observed 56.6% processor utilization against its sampling threshold above 80%. The 56.6% value describes processor utilization; no line-coverage percentage was produced. No benchmark or product source was changed to bypass either failure.

These local checks establish offline rendering evidence only. Live terminal switching, physical key delivery through multiplexers, resize/focus behavior, audio cancellation, and cross-backend voice routing were not tested or implemented by this study. The operator's screen decision remains pending.
