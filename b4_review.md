BLOCKED

### 1. Correctness / KTD3 violation (State contamination across session switches)
- **Location:** `talaria/domain/state.py` (KTD3 proposed change and `focus_session`)
- **Evidence:** KTD3 promises "an unknown type is announced once per session". However, `unknown_event_types` and the new `unknown_event_repeats` counter are not cleared in `focus_session` (lines 475-494). When switching sessions, `land_session` clears the transcript, and the gateway resends the full history of the new session. If `unknown_event_types` is preserved globally, an unknown event in the new session's history will be silently suppressed because the type is already latched from a previous session. Furthermore, because unknown events are part of the session history replayed by the gateway, the `unknown_event_repeats` counter will double every time the operator switches back to a session, rendering the diagnostic useless.
- **Minimum correction:** Add `unknown_event_types=frozenset()` and `unknown_event_repeats=0` to the `replace` call in `focus_session` so the latch and counter genuinely reset on a session switch.

### 2. Correctness (Unknown events bypass cross-session guard)
- **Location:** `talaria/domain/state.py:1387-1388` (`apply_frame`)
- **Evidence:** `apply_frame` routes `UnknownEventFrame` to `_apply_unknown_event` *before* delegating to `_apply_event`. The cross-session guard (`applies_to_focused_session`) lives entirely inside `_apply_event` (lines 1434-1437). Therefore, an unknown event arriving from a background session will bypass the guard, pollute the foreground session's transcript, and falsely increment the foreground session's latch and repeat counter.
- **Minimum correction:** Enforce the cross-session check for `UnknownEventFrame` (using its `session_id`) before routing it to `_apply_unknown_event`, treating foreign session unknown events as `cross_session_events_ignored`.

### 3. Factual Accuracy / Risk Assessment
- **Location:** Risk section and AE4
- **Evidence:** The plan predicts that the `interface_shows_everything` check in the replay gate will stay green. However, this check no longer exists. As explicitly documented in `tests/replay/test_gate.py:352-365`, `interface_shows_everything` was replaced by the two-part ownership proof (`content_is_complete` and `block_documents_are_owned`).
- **Minimum correction:** Update the Risk section and AE4 to name the actual current gate checks (`content_is_complete` and `block_documents_are_owned`). The risk prediction itself survives the attempt to refute it: because `content_is_complete` derives its expectation from the domain state's transcript view, omitting a row from the domain state means the projection expects one fewer row, keeping the check green.

**Categories checked and found clean:**
- Decision completeness
- Scope
