// ===========================================================================
// talaria-v0-2-block-markdown -- emitted Claude Code workflow harness.
// AUTO-EMITTED from a structured execution-spec by execution_spec.py.
// CONTROL FLOW ONLY -- every agent reads the plan as its authoritative spec.
// Per-unit {model, effort} tiers (R2(b)); R3 pilot/fan-out same-tier +
// R10 enumerated-target reconciliation enforced at emit time.
// ===========================================================================

export const meta = {
  name: "talaria-v0-2-block-markdown",
  description: "v0.2 block-level markdown + transcript differentiation: ADR-first bounded-rendering claim, domain terminal-path commits, safe parser boundary, hybrid pane, kind channels, restated gate re-run green; every unit cross-reviewed by the Codex engine before its dependents run.",
}
const settlement = {"casualty_threshold_percent":0,"dispatch_id":"workflow:2337e36be7609803486dca63","driver":{"invocation_id":null,"units":[{"return_keys":[{"deliverable":"return:adr_records_three_ceilings_with_measurement_points","result_key":"adr_records_three_ceilings_with_measurement_points"},{"deliverable":"return:adr_amends_literal_text_rule_explicitly","result_key":"adr_amends_literal_text_rule_explicitly"},{"deliverable":"return:fallback_trigger_recorded","result_key":"fallback_trigger_recorded"},{"deliverable":"return:decisions_md_mirrored","result_key":"decisions_md_mirrored"}],"settlement_unit_id":"U1","workflow_unit_id":"U1"},{"return_keys":[{"deliverable":"return:pane_reply_captured_verbatim","result_key":"pane_reply_captured_verbatim"},{"deliverable":"return:surviving_findings_with_file_line","result_key":"surviving_findings_with_file_line"},{"deliverable":"return:verdict_pass_or_findings","result_key":"verdict_pass_or_findings"}],"settlement_unit_id":"CR1","workflow_unit_id":"CR1"},{"return_keys":[{"deliverable":"return:terminal_paths_commit_partial_buffers","result_key":"terminal_paths_commit_partial_buffers"},{"deliverable":"return:transient_resume_never_duplicates","result_key":"transient_resume_never_duplicates"},{"deliverable":"return:entry_view_carries_raw_undecorated_bodies","result_key":"entry_view_carries_raw_undecorated_bodies"},{"deliverable":"return:line_buffer_and_v01_pin_unchanged","result_key":"line_buffer_and_v01_pin_unchanged"}],"settlement_unit_id":"U2","workflow_unit_id":"U2"},{"return_keys":[{"deliverable":"return:pane_reply_captured_verbatim","result_key":"pane_reply_captured_verbatim"},{"deliverable":"return:surviving_findings_with_file_line","result_key":"surviving_findings_with_file_line"},{"deliverable":"return:verdict_pass_or_findings","result_key":"verdict_pass_or_findings"}],"settlement_unit_id":"CR2","workflow_unit_id":"CR2"},{"return_keys":[{"deliverable":"return:html_literalized_visible_never_dropped","result_key":"html_literalized_visible_never_dropped"},{"deliverable":"return:links_and_bare_urls_inert_text","result_key":"links_and_bare_urls_inert_text"},{"deliverable":"return:rich_markup_and_ansi_never_interpreted","result_key":"rich_markup_and_ansi_never_interpreted"},{"deliverable":"return:entry_isolation_holds_across_unclosed_fence","result_key":"entry_isolation_holds_across_unclosed_fence"},{"deliverable":"return:textual_pin_test_fails_on_drift","result_key":"textual_pin_test_fails_on_drift"}],"settlement_unit_id":"U3","workflow_unit_id":"U3"},{"return_keys":[{"deliverable":"return:pane_reply_captured_verbatim","result_key":"pane_reply_captured_verbatim"},{"deliverable":"return:surviving_findings_with_file_line","result_key":"surviving_findings_with_file_line"},{"deliverable":"return:verdict_pass_or_findings","result_key":"verdict_pass_or_findings"}],"settlement_unit_id":"CR3","workflow_unit_id":"CR3"},{"return_keys":[{"deliverable":"return:fence_streams_progressively_at_boundaries","result_key":"fence_streams_progressively_at_boundaries"},{"deliverable":"return:interim_replacement_renders_exactly_once","result_key":"interim_replacement_renders_exactly_once"},{"deliverable":"return:commit_hands_tail_to_entry_without_rebuild","result_key":"commit_hands_tail_to_entry_without_rebuild"},{"deliverable":"return:mounted_renderables_under_cap_at_every_instant","result_key":"mounted_renderables_under_cap_at_every_instant"},{"deliverable":"return:condensed_line_arithmetic_still_sums","result_key":"condensed_line_arithmetic_still_sums"},{"deliverable":"return:reader_anchor_and_follow_bottom_survive_blocks","result_key":"reader_anchor_and_follow_bottom_survive_blocks"},{"deliverable":"return:table_cells_keyboard_reachable_at_80_columns","result_key":"table_cells_keyboard_reachable_at_80_columns"}],"settlement_unit_id":"U4","workflow_unit_id":"U4"},{"return_keys":[{"deliverable":"return:pane_reply_captured_verbatim","result_key":"pane_reply_captured_verbatim"},{"deliverable":"return:surviving_findings_with_file_line","result_key":"surviving_findings_with_file_line"},{"deliverable":"return:verdict_pass_or_findings","result_key":"verdict_pass_or_findings"}],"settlement_unit_id":"CR4","workflow_unit_id":"CR4"},{"return_keys":[{"deliverable":"return:adjacent_groups_distinguishable_by_computed_style","result_key":"adjacent_groups_distinguishable_by_computed_style"},{"deliverable":"return:reasoning_fence_keeps_kind_channel","result_key":"reasoning_fence_keeps_kind_channel"},{"deliverable":"return:styling_changes_no_widget_height","result_key":"styling_changes_no_widget_height"},{"deliverable":"return:twelve_kind_mapping_is_total","result_key":"twelve_kind_mapping_is_total"}],"settlement_unit_id":"U5","workflow_unit_id":"U5"},{"return_keys":[{"deliverable":"return:pane_reply_captured_verbatim","result_key":"pane_reply_captured_verbatim"},{"deliverable":"return:surviving_findings_with_file_line","result_key":"surviving_findings_with_file_line"},{"deliverable":"return:verdict_pass_or_findings","result_key":"verdict_pass_or_findings"}],"settlement_unit_id":"CR5","workflow_unit_id":"CR5"},{"return_keys":[{"deliverable":"return:region_ownership_proof_replaces_line_window_claim","result_key":"region_ownership_proof_replaces_line_window_claim"},{"deliverable":"return:progressiveness_asserted_at_timed_checkpoints","result_key":"progressiveness_asserted_at_timed_checkpoints"},{"deliverable":"return:adversarial_workloads_hold_ceilings_with_high_water","result_key":"adversarial_workloads_hold_ceilings_with_high_water"},{"deliverable":"return:replay_determinism_on_normalized_block_structure","result_key":"replay_determinism_on_normalized_block_structure"},{"deliverable":"return:early_termination_renders_all_received_content","result_key":"early_termination_renders_all_received_content"},{"deliverable":"return:gate_green_over_existing_and_feature_corpora","result_key":"gate_green_over_existing_and_feature_corpora"}],"settlement_unit_id":"U6","workflow_unit_id":"U6"},{"return_keys":[{"deliverable":"return:pane_reply_captured_verbatim","result_key":"pane_reply_captured_verbatim"},{"deliverable":"return:surviving_findings_with_file_line","result_key":"surviving_findings_with_file_line"},{"deliverable":"return:verdict_pass_or_findings","result_key":"verdict_pass_or_findings"}],"settlement_unit_id":"CR6","workflow_unit_id":"CR6"}]},"max_attempts":3,"schema":"dispatch_settlement.v1","site":"workflow","units":[{"deliverables":["structured-result","return:adr_records_three_ceilings_with_measurement_points","return:adr_amends_literal_text_rule_explicitly","return:fallback_trigger_recorded","return:decisions_md_mirrored"],"idempotency_key":"workflow:2337e36be7609803486dca63:U1","unit_id":"U1"},{"deliverables":["structured-result","return:pane_reply_captured_verbatim","return:surviving_findings_with_file_line","return:verdict_pass_or_findings"],"idempotency_key":"workflow:2337e36be7609803486dca63:CR1","unit_id":"CR1"},{"deliverables":["structured-result","return:terminal_paths_commit_partial_buffers","return:transient_resume_never_duplicates","return:entry_view_carries_raw_undecorated_bodies","return:line_buffer_and_v01_pin_unchanged"],"idempotency_key":"workflow:2337e36be7609803486dca63:U2","unit_id":"U2"},{"deliverables":["structured-result","return:pane_reply_captured_verbatim","return:surviving_findings_with_file_line","return:verdict_pass_or_findings"],"idempotency_key":"workflow:2337e36be7609803486dca63:CR2","unit_id":"CR2"},{"deliverables":["structured-result","return:html_literalized_visible_never_dropped","return:links_and_bare_urls_inert_text","return:rich_markup_and_ansi_never_interpreted","return:entry_isolation_holds_across_unclosed_fence","return:textual_pin_test_fails_on_drift"],"idempotency_key":"workflow:2337e36be7609803486dca63:U3","unit_id":"U3"},{"deliverables":["structured-result","return:pane_reply_captured_verbatim","return:surviving_findings_with_file_line","return:verdict_pass_or_findings"],"idempotency_key":"workflow:2337e36be7609803486dca63:CR3","unit_id":"CR3"},{"deliverables":["structured-result","return:fence_streams_progressively_at_boundaries","return:interim_replacement_renders_exactly_once","return:commit_hands_tail_to_entry_without_rebuild","return:mounted_renderables_under_cap_at_every_instant","return:condensed_line_arithmetic_still_sums","return:reader_anchor_and_follow_bottom_survive_blocks","return:table_cells_keyboard_reachable_at_80_columns"],"idempotency_key":"workflow:2337e36be7609803486dca63:U4","unit_id":"U4"},{"deliverables":["structured-result","return:pane_reply_captured_verbatim","return:surviving_findings_with_file_line","return:verdict_pass_or_findings"],"idempotency_key":"workflow:2337e36be7609803486dca63:CR4","unit_id":"CR4"},{"deliverables":["structured-result","return:adjacent_groups_distinguishable_by_computed_style","return:reasoning_fence_keeps_kind_channel","return:styling_changes_no_widget_height","return:twelve_kind_mapping_is_total"],"idempotency_key":"workflow:2337e36be7609803486dca63:U5","unit_id":"U5"},{"deliverables":["structured-result","return:pane_reply_captured_verbatim","return:surviving_findings_with_file_line","return:verdict_pass_or_findings"],"idempotency_key":"workflow:2337e36be7609803486dca63:CR5","unit_id":"CR5"},{"deliverables":["structured-result","return:region_ownership_proof_replaces_line_window_claim","return:progressiveness_asserted_at_timed_checkpoints","return:adversarial_workloads_hold_ceilings_with_high_water","return:replay_determinism_on_normalized_block_structure","return:early_termination_renders_all_received_content","return:gate_green_over_existing_and_feature_corpora"],"idempotency_key":"workflow:2337e36be7609803486dca63:U6","unit_id":"U6"},{"deliverables":["structured-result","return:pane_reply_captured_verbatim","return:surviving_findings_with_file_line","return:verdict_pass_or_findings"],"idempotency_key":"workflow:2337e36be7609803486dca63:CR6","unit_id":"CR6"}]}

const REPO = "infiquetra/talaria"

const __pulledCords = []

const __advisories = []
const __advisoryRounds = new Map()
const __ADVISORY_ITEM_CAP = 50
const __ADVISORY_ITEM_CHARS = 180
function __renderAdvisory(a) {
  var s
  try {
    if (a === null || a === undefined) s = "(empty advisory entry)"
    else if (typeof a === "string") s = a
    else s = String(a.claim || a.id || JSON.stringify(a))
  } catch (e) {
    s = "(unrenderable advisory entry)"
  }
  // Two passes over two distinct hazards. First: C0, DEL, C1 (NEL included) and the
  // line/paragraph separators -- these forge a second log line. Second: the bidi marks,
  // embeddings, overrides and isolates plus the BOM -- these leave the byte sequence intact
  // but reorder what a human READS in a terminal or log viewer (the Trojan-Source pattern).
  // Advisory text is model-authored by a verifier that read a diff it did not write, so both
  // classes are reachable from repo content.
  var t = s
    .replace(/[\u0000-\u001f\u007f-\u009f\u2028\u2029]+/g, " ")
    .replace(/[\u200e\u200f\u202a-\u202e\u2066-\u2069\ufeff]+/g, " ")
    .slice(0, __ADVISORY_ITEM_CHARS)
  // .slice() cuts on UTF-16 code units, so an astral character (any emoji) straddling the cap
  // leaves a lone high surrogate behind. This string is now STORED and returned, not just
  // logged, so ill-formed UTF-16 would cross the harness return boundary -- a consumer that
  // re-encodes it substitutes U+FFFD or raises. Drop the orphan half.
  var tail = t.charCodeAt(t.length - 1)
  if (tail >= 0xd800 && tail <= 0xdbff) t = t.slice(0, -1)
  return t
}
function __halt(message) {
  var e = new Error(message)
  e.advisory_corrections = __advisories
  return e
}
function __logAdvisory(unitId, reported, refuted) {
  // Counted on EVERY call, before the empty-items early return below -- a panel round that
  // produced no advisories is still a round. Deriving the ordinal from stored entries instead
  // would silently renumber: an iterate_to_consensus unit whose first round was clean would
  // label its second round "round 1", and a driver told to read the last entry would act on
  // advice about a discarded intermediate result. A Map, not an object, so a unit id like
  // `constructor` cannot collide with a prototype member.
  var round = (__advisoryRounds.get(unitId) || 0) + 1
  __advisoryRounds.set(unitId, round)
  var items = []
  try {
    for (var i = 0; i < reported.length; i++) {
      var adv = Array.isArray(reported[i].advisory_corrections) ? reported[i].advisory_corrections : []
      for (var j = 0; j < adv.length; j++) items.push(__renderAdvisory(adv[j]))
    }
  } catch (e) {
    // Through __renderAdvisory, not around it: this marker embeds a model-reachable
    // message and is the one path that would otherwise reach log() unscrubbed.
    items.push(__renderAdvisory("(advisory harvest failed: " + String(e && e.message) + ")"))
  }
  if (items.length === 0) return items
  var dropped = 0
  if (items.length > __ADVISORY_ITEM_CAP) {
    dropped = items.length - __ADVISORY_ITEM_CAP
    items = items.slice(0, __ADVISORY_ITEM_CAP)
  }
  __advisories.push({ unit: unitId, round: round, corrections: items, dropped: dropped })
  try {
    log(`verify panel over ${unitId} (round ${round}): deliverable ${refuted ? "REFUTED" : "UPHELD"} with ${items.length + dropped} advisory correction(s) (narrative/rationale only, non-gating): ` +
        items.join(" | ") + (dropped > 0 ? ` [+${dropped} suppressed]` : ""))
  } catch (e) {
    /* the non-gating accumulator must never be able to halt a run */
  }
  return items
}

function __gate(result, opts) {
  const unitId = opts.unitId || "unknown";

  function isEmptyOrAbsent(val) {
    if (val === null || val === undefined) return true;
    if (typeof val === 'string') return val.trim() === '';
    if (Array.isArray(val)) return val.length === 0;
    if (val instanceof Map || val instanceof Set) return val.size === 0;
    if (typeof val === 'object') return Object.keys(val).length === 0;
    return false;
  }

  function parseResult(val) {
    if (typeof val === 'string') {
      let s = val.trim();
      if (s.startsWith('```')) {
        const lines = s.split('\n');
        if (lines.length >= 2) {
          if (lines[0].startsWith('```')) {
            lines.shift();
          }
          if (lines.length && lines[lines.length - 1].trim() === '```') {
            lines.pop();
          }
          s = lines.join('\n').trim();
        }
      }
      if (s.startsWith('{') || s.startsWith('[')) {
        try {
          return JSON.parse(s);
        } catch (e) {
          // fall through to embedded-JSON extraction
        }
      }
      // Extract an embedded JSON value when the agent prepends conversational prose
      // before the object (sonnet/opus routinely add a "looks good, tests pass" preamble
      // ahead of the return object). Try object first, then array.
      const pairs = [['{', '}'], ['[', ']']];
      for (let i = 0; i < pairs.length; i++) {
        const start = s.indexOf(pairs[i][0]);
        const end = s.lastIndexOf(pairs[i][1]);
        if (start !== -1 && end > start) {
          try {
            return JSON.parse(s.slice(start, end + 1));
          } catch (e) {
            // try the next delimiter pair
          }
        }
      }
    }
    return val;
  }

  // #364 R7: pull_cord -- the worker-initiated out-of-depth disposition, a valid alternative
  // to the return contract (distinct from success and from the missing/malformed throws).
  // Cords batch into __pulledCords for ONE coordinator escalation entry (R8); the unit is
  // never marked complete because the batched check fails the run before it returns.
  const cordProbe = parseResult(result);
  if (cordProbe && typeof cordProbe === 'object' && !Array.isArray(cordProbe)
      && typeof cordProbe.pull_cord === 'string' && cordProbe.pull_cord.trim() !== '') {
    __pulledCords.push({ unit: unitId, reason: cordProbe.pull_cord.trim(),
                         proposal: opts.cordProposal || null });
    return result;
  }

  if (opts.expectsOutput && isEmptyOrAbsent(result)) {
    throw __halt(
      `missing-output: Unit ${unitId} expected structured output but received none or empty.`
    );
  }

  if (typeof result === 'string') {
    let s = result.trim();
    if (s.startsWith('```')) {
      const lines = s.split('\n');
      if (lines.length >= 2) {
        if (lines[0].startsWith('```')) {
          lines.shift();
        }
        if (lines.length && lines[lines.length - 1].trim() === '```') {
          lines.pop();
        }
        s = lines.join('\n').trim();
      }
    }
    if (s.startsWith('{') || s.startsWith('[')) {
      try {
        JSON.parse(s);
      } catch (e) {
        throw __halt(
          `malformed-output: Unit ${unitId} output is a structurally truncated JSON: ${e.message}`
        );
      }
    }
  }

  let targetCount = null;
  if (opts.targets !== undefined && opts.targets !== null) {
    if (typeof opts.targets === 'number') {
      targetCount = opts.targets;
    } else if (Array.isArray(opts.targets)) {
      targetCount = opts.targets.length;
    }
  }

  if (targetCount !== null) {
    const parsed = parseResult(result);
    let producedCount = 0;
    if (parsed !== null && parsed !== undefined) {
      if (Array.isArray(parsed)) {
        producedCount = parsed.length;
      } else if (parsed instanceof Map || parsed instanceof Set) {
        producedCount = parsed.size;
      } else if (typeof parsed === 'object') {
        producedCount = Object.keys(parsed).length;
      } else {
        producedCount = isEmptyOrAbsent(parsed) ? 0 : 1;
      }
    }
    if (producedCount < targetCount) {
      const shortfall = targetCount - producedCount;
      throw __halt(
        `missing-output: Unit ${unitId} produced fewer items than expected. ` +
        `Expected ${targetCount}, produced ${producedCount}. Shortfall: ${shortfall}.`
      );
    }
  }

  if (opts.returns && opts.returns.length > 0) {
    const parsed = parseResult(result);
    if (parsed === null || parsed === undefined || typeof parsed !== 'object' || Array.isArray(parsed)) {
      throw __halt(
        `missing-output: Unit ${unitId} result is not a structured dictionary. ` +
        `Missing required keys: ${opts.returns.join(', ')}.`
      );
    }
    const missing = opts.returns.filter(
      k => !(k in parsed) || parsed[k] === null || parsed[k] === undefined
    );
    if (missing.length > 0) {
      throw __halt(
        `missing-output: Unit ${unitId} output is missing required keys: ${missing.join(', ')}.`
      );
    }
  }

  return result;
}

function __is429(x) {
  if (x === null || x === undefined) return false;
  if (typeof x === 'number') return x === 429;
  if (typeof x === 'string') return /(^|[^0-9])429([^0-9]|$)/.test(x) || /rate[\s_-]?limit/i.test(x);
  var status = x.status || x.statusCode || x.status_code || x.code;
  if (status === 429 || status === '429') return true;
  if (x.rateLimited === true || x.rate_limited === true) return true;
  var msg = x.message || x.error || '';
  return typeof msg === 'string' && (/(^|[^0-9])429([^0-9]|$)/.test(msg) || /rate[\s_-]?limit/i.test(msg));
}

function __retryAfterMs(signal) {
  if (signal === null || typeof signal !== 'object') return null;
  if (typeof signal.retryAfterMs === 'number') return signal.retryAfterMs;
  if (typeof signal.retryAfter === 'number') return signal.retryAfter * 1000;
  if (typeof signal.retry_after === 'number') return signal.retry_after * 1000;
  return null;
}

function __retryBackoffMs(attempt, baseMs, maxMs, retryAfterMs) {
  if (typeof retryAfterMs === 'number' && retryAfterMs > 0) {
    return Math.min(retryAfterMs, maxMs);
  }
  return Math.min(baseMs * Math.pow(2, attempt - 1), maxMs);
}

async function __retry(thunk, opts) {
  var o = opts || {};
  var maxAttempts = o.maxAttempts || 3;
  var baseMs = o.baseMs || 1000;
  var maxMs = o.maxMs || 60000;
  var sleep = o.sleep || function (ms) {
    return new Promise(function (r) {
      if (typeof setTimeout === 'function') { setTimeout(r, ms); } else { r(); }
    });
  };
  var attempt = 0;
  while (true) {
    attempt++;
    var result;
    var threw = false;
    var caught = null;
    try {
      result = await thunk();
    } catch (err) {
      threw = true;
      caught = err;
    }
    var signal = threw ? caught : result;
    if (__is429(signal) && attempt < maxAttempts) {
      await sleep(__retryBackoffMs(attempt, baseMs, maxMs, __retryAfterMs(signal)));
      continue;
    }
    if (threw) throw caught;
    return result;
  }
}

function __verifierPrompt(basePrompt, unitResult, passRule) {
  var gatingBar = (passRule === "unanimous")
    ? "EVERY reporting verifier"
    : "a majority of the panel";
  var rendered;
  try {
    rendered = JSON.stringify(unitResult, null, 2);
  } catch (err) {
    rendered = String(unitResult);
  }
  var repoLine = (typeof REPO === "string")
    ? `PRIMARY REPO PATH: ${REPO}`
    : "PRIMARY REPO PATH: not declared by this workflow";
  return `${basePrompt}

VERIFIER VISIBILITY PROTOCOL (#519):
${repoLine}
- You run in a disposable verifier worktree. Before judging file content, capture the primary
  checkout SHA with: git -C <primary repo path> rev-parse HEAD
- Materialize that exact SHA in your verifier worktree with: git checkout <sha> -- .
- If the unit result names uncommitted files or diffs, inspect the primary checkout read-only
  with git -C <primary repo path> status --short and git -C <primary repo path> diff / diff --
  <path>. For named untracked output files, read the primary checkout path directly; never mutate
  the primary checkout.
- Return examined_sha as the SHA you actually materialized or inspected. If you cannot see enough
  evidence to judge, return a refuted_deliverable entry explaining the visibility gap; do not emit
  prose-only "nothing to verify" output.

VERDICT CONTRACT — two separate buckets. Read this before you write anything.

The unit result you are given contains BOTH a deliverable and a narrative. Sort every disagreement
you find into exactly one of these. Getting the bucket right matters more than finding a lot.

\`refuted_deliverable\` — GATING. A finding belongs here only if the unit's actual WORK is wrong:
- A changed file is wrong, incomplete, or breaks something.
- Required behavior is missing, or behavior the unit was told to preserve was destroyed.
- A test is missing, wrong, asserts nothing, or does not test what it claims.
- A claim in \`checks_run\` is FALSE — the command does not actually pass, or was not actually run,
  or its reported result does not reproduce. Re-run the commands and check.
- The unit says \`status: "done"\` but the work is not done.
- You could not see enough to judge (visibility gap).
A non-empty \`refuted_deliverable\` from ${gatingBar} KILLS the unit and HALTS the whole
workflow. Put a finding here only if you would defend stopping the run over it.

\`advisory_corrections\` — NON-GATING. A finding belongs here if the WORK is right but the unit's
own account of it is wrong or misleading:
- Its explanation of WHY something happened is factually incorrect.
- It misattributes a change to the wrong function, file, or line.
- It mischaracterizes a mechanism, or states a rationale that does not hold.
- Its advice to a downstream unit rests on a wrong premise.
These are recorded and handed to the driver. They do NOT stop the run. Report them fully and
precisely — a wrong premise passed downstream causes real damage later, so this bucket is
genuinely valuable, not a consolation prize.

The test: if the unit's code, tests, and check results are all sound, then NOTHING goes in
\`refuted_deliverable\`, no matter how wrong its prose is. Prose errors are advisory. Full stop.

Both keys are REQUIRED and must be arrays. Use \`[]\` for an empty bucket — never omit either one.

UNIT RESULT INPUT (structured evidence — the \`notes\` field is the unit's NARRATIVE, judge it
under \`advisory_corrections\`; the changed files, tests, and \`checks_run\` are the DELIVERABLE):
${rendered}`;
}

// ---- U1: ADR-0006: the block-aware bounded-rendering claim ----
// ---- U2: Domain: terminal-path commits + entry-scoped projection ----
// ---- U3: The block factory and its forgery proof ----
const [U1, U2, U3] = await parallel([
  () =>
    __retry(() => agent(
      "U1: Write ADR-0006 recording the block-aware bounded-rendering claim BEFORE any implementation: the three ceilings (mounted top-level renderables <= 600 read from Textual's own tree; per-boundary reconcile work proportional to tail + newly committed entries; p99 append/apply latency under one 50ms boundary on the adversarial workloads) with their exact measurement points, the amendment to ADR-0005's literal_text-only rule, and the gate-triggered styled-line-run fallback with what evidence invokes it. Mirror the decision to docs/engineering-journal/DECISIONS.md. Read docs/plans/2026-08-09-talaria-v0-2-block-markdown-plan.md (unit U1, KTD1/KTD8, R13) as your authoritative spec.\n\n## Presentation contract (Infiquetra house style)\n\nYour output is read by another agent, or relayed by a main thread to one operator who is supervising\nseveral workstreams at once. Write for that reader, not for someone who watched you work.\n\n**A stated return contract always wins.** If your instructions specify a return shape \u2014 a JSON object,\na named schema, a structured-output tool call, a required final message \u2014 obey it exactly and ignore\nanything below that would conflict with it. These rules govern the prose you write; they never reshape\na required return value.\n\n**Lead with the answer.** The first sentence says what you found or what is now true. A recap of your\nassignment, a list of the files you opened, and a narration of your process are not findings and do not\nopen a report.\n\n**Report state, not activity.** \"The migration runs clean on Postgres 16\" is state. \"I ran the\nmigration and then checked the logs\" is activity. State is what your caller can act on.\n\n**Situate before you detail.** One sentence naming the repository, host, or system in play, before any\nnumber, path, or identifier. Whoever reads you was not in your context.\n\n**Name the thing; never gesture at it.** A commit hash, issue number, pull-request number, branch, test\nname, or `path:line` reference appears in apposition to a noun saying what it is \u2014 \"pull request 656\",\n\"the emitter at `execution_spec.py:3244`\" \u2014 never as a sentence's subject or object on its own. The\nsame goes for unanchored roles: say the repository, the host, the path, not \"the receiver\" or \"the\ndownstream job\".\n\n**Quote only what is load-bearing.** Reproduce exact error strings, diff hunks, and command output\nwhose precise characters matter. Do not paste a whole file, a whole log, or a whole payload and leave\nthe reading to your caller \u2014 digesting it is the work you were spawned to do.\n\n**No unrequested visual.** No diagram, table, banner, or drawn box unless your caller asked for one, or\nyou are comparing three or more items that share attributes, which is a Markdown table. Use Mermaid\nonly in text destined for a file, a pull-request body, or a rendered artifact \u2014 never in a payload\nbound for a terminal. Box-drawing characters are for file-tree connectors and genuine pictures only,\nnever for callouts, banners, or emphasis.\n\n**No operator ceremony.** The operator-facing closing block and the main thread's style tell belong to\nthe main thread alone. Do not write either one. End when your content ends.\n\n**Say what you did not verify.** An unverified inference is labelled as one, in the same sentence.\n\"I did not check X\" is a finding; a confident guess that reads like a measurement is a defect that\npropagates, because your caller cannot tell the two apart from the outside.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys adr_records_three_ceilings_with_measurement_points, adr_amends_literal_text_rule_explicitly, fallback_trigger_recorded, decisions_md_mirrored -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
      { label: "ADR-0006: the block-aware bounded-rendering claim", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"adr_amends_literal_text_rule_explicitly": {}, "adr_records_three_ceilings_with_measurement_points": {}, "decisions_md_mirrored": {}, "fallback_trigger_recorded": {}}, "required": ["adr_records_three_ceilings_with_measurement_points", "adr_amends_literal_text_rule_explicitly", "fallback_trigger_recorded", "decisions_md_mirrored"], "type": "object"} },
    ), { unitId: "U1", maxAttempts: 3 }),
  () =>
    __retry(() => agent(
      "U2: In the domain only (ADR-0002: no framework import), commit partial streaming and reasoning buffers as entries on turn-terminal paths (_on_error at talaria/domain/state.py:1181, confirmed cancellation, terminal disconnect) while a transient resume never duplicates; grow TranscriptView with per-entry records (kind, raw body, committed flag, line span) while the flattened decorated line buffer stays byte-identical for terminal_read and content_is_complete - tests/domain/test_projection.py::test_every_transcript_entry_survives_into_the_line_buffer must pass UNMODIFIED. Read docs/plans/2026-08-09-talaria-v0-2-block-markdown-plan.md (unit U2, KTD6/KTD7, R6/R18) as your authoritative spec.\n\n## Presentation contract (Infiquetra house style)\n\nYour output is read by another agent, or relayed by a main thread to one operator who is supervising\nseveral workstreams at once. Write for that reader, not for someone who watched you work.\n\n**A stated return contract always wins.** If your instructions specify a return shape \u2014 a JSON object,\na named schema, a structured-output tool call, a required final message \u2014 obey it exactly and ignore\nanything below that would conflict with it. These rules govern the prose you write; they never reshape\na required return value.\n\n**Lead with the answer.** The first sentence says what you found or what is now true. A recap of your\nassignment, a list of the files you opened, and a narration of your process are not findings and do not\nopen a report.\n\n**Report state, not activity.** \"The migration runs clean on Postgres 16\" is state. \"I ran the\nmigration and then checked the logs\" is activity. State is what your caller can act on.\n\n**Situate before you detail.** One sentence naming the repository, host, or system in play, before any\nnumber, path, or identifier. Whoever reads you was not in your context.\n\n**Name the thing; never gesture at it.** A commit hash, issue number, pull-request number, branch, test\nname, or `path:line` reference appears in apposition to a noun saying what it is \u2014 \"pull request 656\",\n\"the emitter at `execution_spec.py:3244`\" \u2014 never as a sentence's subject or object on its own. The\nsame goes for unanchored roles: say the repository, the host, the path, not \"the receiver\" or \"the\ndownstream job\".\n\n**Quote only what is load-bearing.** Reproduce exact error strings, diff hunks, and command output\nwhose precise characters matter. Do not paste a whole file, a whole log, or a whole payload and leave\nthe reading to your caller \u2014 digesting it is the work you were spawned to do.\n\n**No unrequested visual.** No diagram, table, banner, or drawn box unless your caller asked for one, or\nyou are comparing three or more items that share attributes, which is a Markdown table. Use Mermaid\nonly in text destined for a file, a pull-request body, or a rendered artifact \u2014 never in a payload\nbound for a terminal. Box-drawing characters are for file-tree connectors and genuine pictures only,\nnever for callouts, banners, or emphasis.\n\n**No operator ceremony.** The operator-facing closing block and the main thread's style tell belong to\nthe main thread alone. Do not write either one. End when your content ends.\n\n**Say what you did not verify.** An unverified inference is labelled as one, in the same sentence.\n\"I did not check X\" is a finding; a confident guess that reads like a measurement is a defect that\npropagates, because your caller cannot tell the two apart from the outside.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys terminal_paths_commit_partial_buffers, transient_resume_never_duplicates, entry_view_carries_raw_undecorated_bodies, line_buffer_and_v01_pin_unchanged -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
      { label: "Domain: terminal-path commits + entry-scoped projection", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"entry_view_carries_raw_undecorated_bodies": {}, "line_buffer_and_v01_pin_unchanged": {}, "terminal_paths_commit_partial_buffers": {}, "transient_resume_never_duplicates": {}}, "required": ["terminal_paths_commit_partial_buffers", "transient_resume_never_duplicates", "entry_view_carries_raw_undecorated_bodies", "line_buffer_and_v01_pin_unchanged"], "type": "object"} },
    ), { unitId: "U2", maxAttempts: 3 }),
  () =>
    __retry(() => agent(
      "U3: Create talaria/ui/blocks.py - the single constructor for safely configured Markdown entry documents: parser_factory returning MarkdownIt('gfm-like') with html=False and linkify=False, open_links=False, defang before parse at parity with talaria/ui/literal.py. Prove every forbidden channel with a test: HTML literalized visible (never dropped), Rich console markup unparsed, ANSI defanged, links styled but inert with nothing opened or fetched, bare URLs stay text, entry isolation across an unclosed fence, and a Textual 8.2.8 pin test on append/update semantics that fails loudly on upgrade drift. Read docs/plans/2026-08-09-talaria-v0-2-block-markdown-plan.md (unit U3, KTD3/KTD4, R9/R10/R15) as your authoritative spec.\n\n## Presentation contract (Infiquetra house style)\n\nYour output is read by another agent, or relayed by a main thread to one operator who is supervising\nseveral workstreams at once. Write for that reader, not for someone who watched you work.\n\n**A stated return contract always wins.** If your instructions specify a return shape \u2014 a JSON object,\na named schema, a structured-output tool call, a required final message \u2014 obey it exactly and ignore\nanything below that would conflict with it. These rules govern the prose you write; they never reshape\na required return value.\n\n**Lead with the answer.** The first sentence says what you found or what is now true. A recap of your\nassignment, a list of the files you opened, and a narration of your process are not findings and do not\nopen a report.\n\n**Report state, not activity.** \"The migration runs clean on Postgres 16\" is state. \"I ran the\nmigration and then checked the logs\" is activity. State is what your caller can act on.\n\n**Situate before you detail.** One sentence naming the repository, host, or system in play, before any\nnumber, path, or identifier. Whoever reads you was not in your context.\n\n**Name the thing; never gesture at it.** A commit hash, issue number, pull-request number, branch, test\nname, or `path:line` reference appears in apposition to a noun saying what it is \u2014 \"pull request 656\",\n\"the emitter at `execution_spec.py:3244`\" \u2014 never as a sentence's subject or object on its own. The\nsame goes for unanchored roles: say the repository, the host, the path, not \"the receiver\" or \"the\ndownstream job\".\n\n**Quote only what is load-bearing.** Reproduce exact error strings, diff hunks, and command output\nwhose precise characters matter. Do not paste a whole file, a whole log, or a whole payload and leave\nthe reading to your caller \u2014 digesting it is the work you were spawned to do.\n\n**No unrequested visual.** No diagram, table, banner, or drawn box unless your caller asked for one, or\nyou are comparing three or more items that share attributes, which is a Markdown table. Use Mermaid\nonly in text destined for a file, a pull-request body, or a rendered artifact \u2014 never in a payload\nbound for a terminal. Box-drawing characters are for file-tree connectors and genuine pictures only,\nnever for callouts, banners, or emphasis.\n\n**No operator ceremony.** The operator-facing closing block and the main thread's style tell belong to\nthe main thread alone. Do not write either one. End when your content ends.\n\n**Say what you did not verify.** An unverified inference is labelled as one, in the same sentence.\n\"I did not check X\" is a finding; a confident guess that reads like a measurement is a defect that\npropagates, because your caller cannot tell the two apart from the outside.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys html_literalized_visible_never_dropped, links_and_bare_urls_inert_text, rich_markup_and_ansi_never_interpreted, entry_isolation_holds_across_unclosed_fence, textual_pin_test_fails_on_drift -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
      { label: "The block factory and its forgery proof", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"entry_isolation_holds_across_unclosed_fence": {}, "html_literalized_visible_never_dropped": {}, "links_and_bare_urls_inert_text": {}, "rich_markup_and_ansi_never_interpreted": {}, "textual_pin_test_fails_on_drift": {}}, "required": ["html_literalized_visible_never_dropped", "links_and_bare_urls_inert_text", "rich_markup_and_ansi_never_interpreted", "entry_isolation_holds_across_unclosed_fence", "textual_pin_test_fails_on_drift"], "type": "object"} },
    ), { unitId: "U3", maxAttempts: 3 }),
])
__gate(U1, { unitId: "U1", expectsOutput: true, returns: ["adr_records_three_ceilings_with_measurement_points", "adr_amends_literal_text_rule_explicitly", "fallback_trigger_recorded", "decisions_md_mirrored"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })
__gate(U2, { unitId: "U2", expectsOutput: true, returns: ["terminal_paths_commit_partial_buffers", "transient_resume_never_duplicates", "entry_view_carries_raw_undecorated_bodies", "line_buffer_and_v01_pin_unchanged"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })
__gate(U3, { unitId: "U3", expectsOutput: true, returns: ["html_literalized_visible_never_dropped", "links_and_bare_urls_inert_text", "rich_markup_and_ansi_never_interpreted", "entry_isolation_holds_across_unclosed_fence", "textual_pin_test_fails_on_drift"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })

// ---- CR1: Codex review of U1's ADR ----
// depends_on: U1 (barrier)
// ---- CR2: Codex review of U2's diff ----
// depends_on: U2 (barrier)
// ---- CR3: Codex review of U3's diff ----
// depends_on: U3 (barrier)
const [CR1, CR2, CR3] = await parallel([
  () =>
    __retry(() => agent(
      "CR1: Drive the operator's herdr-managed Codex reviewer pane to review U1's diff (ADR-0006 and its DECISIONS.md mirror), judge its findings, and return the ones that survive with file:line. Mechanics: resolve the reviewer pane via `herdr agent list` / `herdr tab list` using the machine-local identity the driver injects from the saga state; send the review request with `herdr pane send-text` (then `send-keys` Enter), wait for the agent to settle with `herdr pane wait-output` or agent_status polling, and read the reply with `herdr pane read --source recent-unwrapped` (a `visible` read presents stale content as a confident answer - do not use it). Capture the pane's reply verbatim as the review evidence (an uncaptured review is treated as not having run). The unit's spec is docs/plans/2026-08-09-talaria-v0-2-block-markdown-plan.md unit U1.\n\n## Presentation contract (Infiquetra house style)\n\nYour output is read by another agent, or relayed by a main thread to one operator who is supervising\nseveral workstreams at once. Write for that reader, not for someone who watched you work.\n\n**A stated return contract always wins.** If your instructions specify a return shape \u2014 a JSON object,\na named schema, a structured-output tool call, a required final message \u2014 obey it exactly and ignore\nanything below that would conflict with it. These rules govern the prose you write; they never reshape\na required return value.\n\n**Lead with the answer.** The first sentence says what you found or what is now true. A recap of your\nassignment, a list of the files you opened, and a narration of your process are not findings and do not\nopen a report.\n\n**Report state, not activity.** \"The migration runs clean on Postgres 16\" is state. \"I ran the\nmigration and then checked the logs\" is activity. State is what your caller can act on.\n\n**Situate before you detail.** One sentence naming the repository, host, or system in play, before any\nnumber, path, or identifier. Whoever reads you was not in your context.\n\n**Name the thing; never gesture at it.** A commit hash, issue number, pull-request number, branch, test\nname, or `path:line` reference appears in apposition to a noun saying what it is \u2014 \"pull request 656\",\n\"the emitter at `execution_spec.py:3244`\" \u2014 never as a sentence's subject or object on its own. The\nsame goes for unanchored roles: say the repository, the host, the path, not \"the receiver\" or \"the\ndownstream job\".\n\n**Quote only what is load-bearing.** Reproduce exact error strings, diff hunks, and command output\nwhose precise characters matter. Do not paste a whole file, a whole log, or a whole payload and leave\nthe reading to your caller \u2014 digesting it is the work you were spawned to do.\n\n**No unrequested visual.** No diagram, table, banner, or drawn box unless your caller asked for one, or\nyou are comparing three or more items that share attributes, which is a Markdown table. Use Mermaid\nonly in text destined for a file, a pull-request body, or a rendered artifact \u2014 never in a payload\nbound for a terminal. Box-drawing characters are for file-tree connectors and genuine pictures only,\nnever for callouts, banners, or emphasis.\n\n**No operator ceremony.** The operator-facing closing block and the main thread's style tell belong to\nthe main thread alone. Do not write either one. End when your content ends.\n\n**Say what you did not verify.** An unverified inference is labelled as one, in the same sentence.\n\"I did not check X\" is a finding; a confident guess that reads like a measurement is a defect that\npropagates, because your caller cannot tell the two apart from the outside.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys pane_reply_captured_verbatim, surviving_findings_with_file_line, verdict_pass_or_findings -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
      { label: "Codex review of U1's ADR", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"pane_reply_captured_verbatim": {}, "surviving_findings_with_file_line": {}, "verdict_pass_or_findings": {}}, "required": ["pane_reply_captured_verbatim", "surviving_findings_with_file_line", "verdict_pass_or_findings"], "type": "object"} },
    ), { unitId: "CR1", maxAttempts: 3 }),
  () =>
    __retry(() => agent(
      "CR2: Drive the operator's herdr-managed Codex reviewer pane to review U2's diff, judge its findings, and return the ones that survive with file:line. Mechanics: resolve the reviewer pane via `herdr agent list` / `herdr tab list` using the machine-local identity the driver injects from the saga state; send the review request with `herdr pane send-text` (then `send-keys` Enter), wait for the agent to settle with `herdr pane wait-output` or agent_status polling, and read the reply with `herdr pane read --source recent-unwrapped` (a `visible` read presents stale content as a confident answer - do not use it). Capture the pane's reply verbatim as the review evidence (an uncaptured review is treated as not having run). The unit's spec is docs/plans/2026-08-09-talaria-v0-2-block-markdown-plan.md unit U2.\n\n## Presentation contract (Infiquetra house style)\n\nYour output is read by another agent, or relayed by a main thread to one operator who is supervising\nseveral workstreams at once. Write for that reader, not for someone who watched you work.\n\n**A stated return contract always wins.** If your instructions specify a return shape \u2014 a JSON object,\na named schema, a structured-output tool call, a required final message \u2014 obey it exactly and ignore\nanything below that would conflict with it. These rules govern the prose you write; they never reshape\na required return value.\n\n**Lead with the answer.** The first sentence says what you found or what is now true. A recap of your\nassignment, a list of the files you opened, and a narration of your process are not findings and do not\nopen a report.\n\n**Report state, not activity.** \"The migration runs clean on Postgres 16\" is state. \"I ran the\nmigration and then checked the logs\" is activity. State is what your caller can act on.\n\n**Situate before you detail.** One sentence naming the repository, host, or system in play, before any\nnumber, path, or identifier. Whoever reads you was not in your context.\n\n**Name the thing; never gesture at it.** A commit hash, issue number, pull-request number, branch, test\nname, or `path:line` reference appears in apposition to a noun saying what it is \u2014 \"pull request 656\",\n\"the emitter at `execution_spec.py:3244`\" \u2014 never as a sentence's subject or object on its own. The\nsame goes for unanchored roles: say the repository, the host, the path, not \"the receiver\" or \"the\ndownstream job\".\n\n**Quote only what is load-bearing.** Reproduce exact error strings, diff hunks, and command output\nwhose precise characters matter. Do not paste a whole file, a whole log, or a whole payload and leave\nthe reading to your caller \u2014 digesting it is the work you were spawned to do.\n\n**No unrequested visual.** No diagram, table, banner, or drawn box unless your caller asked for one, or\nyou are comparing three or more items that share attributes, which is a Markdown table. Use Mermaid\nonly in text destined for a file, a pull-request body, or a rendered artifact \u2014 never in a payload\nbound for a terminal. Box-drawing characters are for file-tree connectors and genuine pictures only,\nnever for callouts, banners, or emphasis.\n\n**No operator ceremony.** The operator-facing closing block and the main thread's style tell belong to\nthe main thread alone. Do not write either one. End when your content ends.\n\n**Say what you did not verify.** An unverified inference is labelled as one, in the same sentence.\n\"I did not check X\" is a finding; a confident guess that reads like a measurement is a defect that\npropagates, because your caller cannot tell the two apart from the outside.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys pane_reply_captured_verbatim, surviving_findings_with_file_line, verdict_pass_or_findings -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
      { label: "Codex review of U2's diff", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"pane_reply_captured_verbatim": {}, "surviving_findings_with_file_line": {}, "verdict_pass_or_findings": {}}, "required": ["pane_reply_captured_verbatim", "surviving_findings_with_file_line", "verdict_pass_or_findings"], "type": "object"} },
    ), { unitId: "CR2", maxAttempts: 3 }),
  () =>
    __retry(() => agent(
      "CR3: Drive the operator's herdr-managed Codex reviewer pane to review U3's diff, judge its findings, and return the ones that survive with file:line. Mechanics: resolve the reviewer pane via `herdr agent list` / `herdr tab list` using the machine-local identity the driver injects from the saga state; send the review request with `herdr pane send-text` (then `send-keys` Enter), wait for the agent to settle with `herdr pane wait-output` or agent_status polling, and read the reply with `herdr pane read --source recent-unwrapped` (a `visible` read presents stale content as a confident answer - do not use it). Capture the pane's reply verbatim as the review evidence (an uncaptured review is treated as not having run). The unit's spec is docs/plans/2026-08-09-talaria-v0-2-block-markdown-plan.md unit U3.\n\n## Presentation contract (Infiquetra house style)\n\nYour output is read by another agent, or relayed by a main thread to one operator who is supervising\nseveral workstreams at once. Write for that reader, not for someone who watched you work.\n\n**A stated return contract always wins.** If your instructions specify a return shape \u2014 a JSON object,\na named schema, a structured-output tool call, a required final message \u2014 obey it exactly and ignore\nanything below that would conflict with it. These rules govern the prose you write; they never reshape\na required return value.\n\n**Lead with the answer.** The first sentence says what you found or what is now true. A recap of your\nassignment, a list of the files you opened, and a narration of your process are not findings and do not\nopen a report.\n\n**Report state, not activity.** \"The migration runs clean on Postgres 16\" is state. \"I ran the\nmigration and then checked the logs\" is activity. State is what your caller can act on.\n\n**Situate before you detail.** One sentence naming the repository, host, or system in play, before any\nnumber, path, or identifier. Whoever reads you was not in your context.\n\n**Name the thing; never gesture at it.** A commit hash, issue number, pull-request number, branch, test\nname, or `path:line` reference appears in apposition to a noun saying what it is \u2014 \"pull request 656\",\n\"the emitter at `execution_spec.py:3244`\" \u2014 never as a sentence's subject or object on its own. The\nsame goes for unanchored roles: say the repository, the host, the path, not \"the receiver\" or \"the\ndownstream job\".\n\n**Quote only what is load-bearing.** Reproduce exact error strings, diff hunks, and command output\nwhose precise characters matter. Do not paste a whole file, a whole log, or a whole payload and leave\nthe reading to your caller \u2014 digesting it is the work you were spawned to do.\n\n**No unrequested visual.** No diagram, table, banner, or drawn box unless your caller asked for one, or\nyou are comparing three or more items that share attributes, which is a Markdown table. Use Mermaid\nonly in text destined for a file, a pull-request body, or a rendered artifact \u2014 never in a payload\nbound for a terminal. Box-drawing characters are for file-tree connectors and genuine pictures only,\nnever for callouts, banners, or emphasis.\n\n**No operator ceremony.** The operator-facing closing block and the main thread's style tell belong to\nthe main thread alone. Do not write either one. End when your content ends.\n\n**Say what you did not verify.** An unverified inference is labelled as one, in the same sentence.\n\"I did not check X\" is a finding; a confident guess that reads like a measurement is a defect that\npropagates, because your caller cannot tell the two apart from the outside.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys pane_reply_captured_verbatim, surviving_findings_with_file_line, verdict_pass_or_findings -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
      { label: "Codex review of U3's diff", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"pane_reply_captured_verbatim": {}, "surviving_findings_with_file_line": {}, "verdict_pass_or_findings": {}}, "required": ["pane_reply_captured_verbatim", "surviving_findings_with_file_line", "verdict_pass_or_findings"], "type": "object"} },
    ), { unitId: "CR3", maxAttempts: 3 }),
])
__gate(CR1, { unitId: "CR1", expectsOutput: true, returns: ["pane_reply_captured_verbatim", "surviving_findings_with_file_line", "verdict_pass_or_findings"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })
__gate(CR2, { unitId: "CR2", expectsOutput: true, returns: ["pane_reply_captured_verbatim", "surviving_findings_with_file_line", "verdict_pass_or_findings"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })
__gate(CR3, { unitId: "CR3", expectsOutput: true, returns: ["pane_reply_captured_verbatim", "surviving_findings_with_file_line", "verdict_pass_or_findings"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })

// ---- U4: The hybrid transcript pane ----
// depends_on: CR1, CR2, CR3 (barrier)
const U4 = await agent(
  "U4: Rebuild TranscriptPane as the hybrid: committed assistant/reasoning entries mount one Markdown document each (built by talaria/ui/blocks.py), every other kind keeps one TranscriptLine per line, the in-flight stream renders in a single live Markdown tail widget driven by public Markdown.append at the 50ms boundary with Markdown.update for interim replacement (replace wins, exactly once); commit hands the tail source to the entry document without a pane rebuild; restate cap, condensation (banner keeps line arithmetic), and reader anchor over mixed units per ADR-0006's ceilings; tables keyboard-reachable at 80 columns. Read docs/plans/2026-08-09-talaria-v0-2-block-markdown-plan.md (unit U4, KTD1/KTD2/KTD3, R1/R2/R5/R16/R17) as your authoritative spec.\n\n## Presentation contract (Infiquetra house style)\n\nYour output is read by another agent, or relayed by a main thread to one operator who is supervising\nseveral workstreams at once. Write for that reader, not for someone who watched you work.\n\n**A stated return contract always wins.** If your instructions specify a return shape \u2014 a JSON object,\na named schema, a structured-output tool call, a required final message \u2014 obey it exactly and ignore\nanything below that would conflict with it. These rules govern the prose you write; they never reshape\na required return value.\n\n**Lead with the answer.** The first sentence says what you found or what is now true. A recap of your\nassignment, a list of the files you opened, and a narration of your process are not findings and do not\nopen a report.\n\n**Report state, not activity.** \"The migration runs clean on Postgres 16\" is state. \"I ran the\nmigration and then checked the logs\" is activity. State is what your caller can act on.\n\n**Situate before you detail.** One sentence naming the repository, host, or system in play, before any\nnumber, path, or identifier. Whoever reads you was not in your context.\n\n**Name the thing; never gesture at it.** A commit hash, issue number, pull-request number, branch, test\nname, or `path:line` reference appears in apposition to a noun saying what it is \u2014 \"pull request 656\",\n\"the emitter at `execution_spec.py:3244`\" \u2014 never as a sentence's subject or object on its own. The\nsame goes for unanchored roles: say the repository, the host, the path, not \"the receiver\" or \"the\ndownstream job\".\n\n**Quote only what is load-bearing.** Reproduce exact error strings, diff hunks, and command output\nwhose precise characters matter. Do not paste a whole file, a whole log, or a whole payload and leave\nthe reading to your caller \u2014 digesting it is the work you were spawned to do.\n\n**No unrequested visual.** No diagram, table, banner, or drawn box unless your caller asked for one, or\nyou are comparing three or more items that share attributes, which is a Markdown table. Use Mermaid\nonly in text destined for a file, a pull-request body, or a rendered artifact \u2014 never in a payload\nbound for a terminal. Box-drawing characters are for file-tree connectors and genuine pictures only,\nnever for callouts, banners, or emphasis.\n\n**No operator ceremony.** The operator-facing closing block and the main thread's style tell belong to\nthe main thread alone. Do not write either one. End when your content ends.\n\n**Say what you did not verify.** An unverified inference is labelled as one, in the same sentence.\n\"I did not check X\" is a finding; a confident guess that reads like a measurement is a defect that\npropagates, because your caller cannot tell the two apart from the outside.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys fence_streams_progressively_at_boundaries, interim_replacement_renders_exactly_once, commit_hands_tail_to_entry_without_rebuild, mounted_renderables_under_cap_at_every_instant, condensed_line_arithmetic_still_sums, reader_anchor_and_follow_bottom_survive_blocks, table_cells_keyboard_reachable_at_80_columns -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "The hybrid transcript pane", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"commit_hands_tail_to_entry_without_rebuild": {}, "condensed_line_arithmetic_still_sums": {}, "fence_streams_progressively_at_boundaries": {}, "interim_replacement_renders_exactly_once": {}, "mounted_renderables_under_cap_at_every_instant": {}, "reader_anchor_and_follow_bottom_survive_blocks": {}, "table_cells_keyboard_reachable_at_80_columns": {}}, "required": ["fence_streams_progressively_at_boundaries", "interim_replacement_renders_exactly_once", "commit_hands_tail_to_entry_without_rebuild", "mounted_renderables_under_cap_at_every_instant", "condensed_line_arithmetic_still_sums", "reader_anchor_and_follow_bottom_survive_blocks", "table_cells_keyboard_reachable_at_80_columns"], "type": "object"} },
)
__gate(U4, { unitId: "U4", expectsOutput: true, returns: ["fence_streams_progressively_at_boundaries", "interim_replacement_renders_exactly_once", "commit_hands_tail_to_entry_without_rebuild", "mounted_renderables_under_cap_at_every_instant", "condensed_line_arithmetic_still_sums", "reader_anchor_and_follow_bottom_survive_blocks", "table_cells_keyboard_reachable_at_80_columns"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })

// ---- CR4: Codex review of U4's diff ----
// depends_on: U4 (barrier)
const CR4 = await agent(
  "CR4: Drive the operator's herdr-managed Codex reviewer pane to review U4's diff, judge its findings, and return the ones that survive with file:line. Mechanics: resolve the reviewer pane via `herdr agent list` / `herdr tab list` using the machine-local identity the driver injects from the saga state; send the review request with `herdr pane send-text` (then `send-keys` Enter), wait for the agent to settle with `herdr pane wait-output` or agent_status polling, and read the reply with `herdr pane read --source recent-unwrapped` (a `visible` read presents stale content as a confident answer - do not use it). Capture the pane's reply verbatim as the review evidence (an uncaptured review is treated as not having run). The unit's spec is docs/plans/2026-08-09-talaria-v0-2-block-markdown-plan.md unit U4.\n\n## Presentation contract (Infiquetra house style)\n\nYour output is read by another agent, or relayed by a main thread to one operator who is supervising\nseveral workstreams at once. Write for that reader, not for someone who watched you work.\n\n**A stated return contract always wins.** If your instructions specify a return shape \u2014 a JSON object,\na named schema, a structured-output tool call, a required final message \u2014 obey it exactly and ignore\nanything below that would conflict with it. These rules govern the prose you write; they never reshape\na required return value.\n\n**Lead with the answer.** The first sentence says what you found or what is now true. A recap of your\nassignment, a list of the files you opened, and a narration of your process are not findings and do not\nopen a report.\n\n**Report state, not activity.** \"The migration runs clean on Postgres 16\" is state. \"I ran the\nmigration and then checked the logs\" is activity. State is what your caller can act on.\n\n**Situate before you detail.** One sentence naming the repository, host, or system in play, before any\nnumber, path, or identifier. Whoever reads you was not in your context.\n\n**Name the thing; never gesture at it.** A commit hash, issue number, pull-request number, branch, test\nname, or `path:line` reference appears in apposition to a noun saying what it is \u2014 \"pull request 656\",\n\"the emitter at `execution_spec.py:3244`\" \u2014 never as a sentence's subject or object on its own. The\nsame goes for unanchored roles: say the repository, the host, the path, not \"the receiver\" or \"the\ndownstream job\".\n\n**Quote only what is load-bearing.** Reproduce exact error strings, diff hunks, and command output\nwhose precise characters matter. Do not paste a whole file, a whole log, or a whole payload and leave\nthe reading to your caller \u2014 digesting it is the work you were spawned to do.\n\n**No unrequested visual.** No diagram, table, banner, or drawn box unless your caller asked for one, or\nyou are comparing three or more items that share attributes, which is a Markdown table. Use Mermaid\nonly in text destined for a file, a pull-request body, or a rendered artifact \u2014 never in a payload\nbound for a terminal. Box-drawing characters are for file-tree connectors and genuine pictures only,\nnever for callouts, banners, or emphasis.\n\n**No operator ceremony.** The operator-facing closing block and the main thread's style tell belong to\nthe main thread alone. Do not write either one. End when your content ends.\n\n**Say what you did not verify.** An unverified inference is labelled as one, in the same sentence.\n\"I did not check X\" is a finding; a confident guess that reads like a measurement is a defect that\npropagates, because your caller cannot tell the two apart from the outside.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys pane_reply_captured_verbatim, surviving_findings_with_file_line, verdict_pass_or_findings -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Codex review of U4's diff", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"pane_reply_captured_verbatim": {}, "surviving_findings_with_file_line": {}, "verdict_pass_or_findings": {}}, "required": ["pane_reply_captured_verbatim", "surviving_findings_with_file_line", "verdict_pass_or_findings"], "type": "object"} },
)
__gate(CR4, { unitId: "CR4", expectsOutput: true, returns: ["pane_reply_captured_verbatim", "surviving_findings_with_file_line", "verdict_pass_or_findings"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })

// ---- U5: Kind differentiation channels ----
// depends_on: CR4 (barrier)
const U5 = await agent(
  "U5: Map the twelve TranscriptKind members to the five kind groups with per-widget CSS classes and theme variables; carry the channel by background tint and gutter marker so a fence's syntax colours cannot erase it; assert by computed styles with zero per-widget height delta against the same screen unstyled; the mapping test is total (a new kind fails it rather than rendering ungrouped). Read docs/plans/2026-08-09-talaria-v0-2-block-markdown-plan.md (unit U5, KTD5, R7/R8) as your authoritative spec.\n\n## Presentation contract (Infiquetra house style)\n\nYour output is read by another agent, or relayed by a main thread to one operator who is supervising\nseveral workstreams at once. Write for that reader, not for someone who watched you work.\n\n**A stated return contract always wins.** If your instructions specify a return shape \u2014 a JSON object,\na named schema, a structured-output tool call, a required final message \u2014 obey it exactly and ignore\nanything below that would conflict with it. These rules govern the prose you write; they never reshape\na required return value.\n\n**Lead with the answer.** The first sentence says what you found or what is now true. A recap of your\nassignment, a list of the files you opened, and a narration of your process are not findings and do not\nopen a report.\n\n**Report state, not activity.** \"The migration runs clean on Postgres 16\" is state. \"I ran the\nmigration and then checked the logs\" is activity. State is what your caller can act on.\n\n**Situate before you detail.** One sentence naming the repository, host, or system in play, before any\nnumber, path, or identifier. Whoever reads you was not in your context.\n\n**Name the thing; never gesture at it.** A commit hash, issue number, pull-request number, branch, test\nname, or `path:line` reference appears in apposition to a noun saying what it is \u2014 \"pull request 656\",\n\"the emitter at `execution_spec.py:3244`\" \u2014 never as a sentence's subject or object on its own. The\nsame goes for unanchored roles: say the repository, the host, the path, not \"the receiver\" or \"the\ndownstream job\".\n\n**Quote only what is load-bearing.** Reproduce exact error strings, diff hunks, and command output\nwhose precise characters matter. Do not paste a whole file, a whole log, or a whole payload and leave\nthe reading to your caller \u2014 digesting it is the work you were spawned to do.\n\n**No unrequested visual.** No diagram, table, banner, or drawn box unless your caller asked for one, or\nyou are comparing three or more items that share attributes, which is a Markdown table. Use Mermaid\nonly in text destined for a file, a pull-request body, or a rendered artifact \u2014 never in a payload\nbound for a terminal. Box-drawing characters are for file-tree connectors and genuine pictures only,\nnever for callouts, banners, or emphasis.\n\n**No operator ceremony.** The operator-facing closing block and the main thread's style tell belong to\nthe main thread alone. Do not write either one. End when your content ends.\n\n**Say what you did not verify.** An unverified inference is labelled as one, in the same sentence.\n\"I did not check X\" is a finding; a confident guess that reads like a measurement is a defect that\npropagates, because your caller cannot tell the two apart from the outside.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys adjacent_groups_distinguishable_by_computed_style, reasoning_fence_keeps_kind_channel, styling_changes_no_widget_height, twelve_kind_mapping_is_total -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Kind differentiation channels", model: "sonnet", effort: "medium", schema: {"additionalProperties": true, "properties": {"adjacent_groups_distinguishable_by_computed_style": {}, "reasoning_fence_keeps_kind_channel": {}, "styling_changes_no_widget_height": {}, "twelve_kind_mapping_is_total": {}}, "required": ["adjacent_groups_distinguishable_by_computed_style", "reasoning_fence_keeps_kind_channel", "styling_changes_no_widget_height", "twelve_kind_mapping_is_total"], "type": "object"} },
)
__gate(U5, { unitId: "U5", expectsOutput: true, returns: ["adjacent_groups_distinguishable_by_computed_style", "reasoning_fence_keeps_kind_channel", "styling_changes_no_widget_height", "twelve_kind_mapping_is_total"], cordProposal: "sonnet/medium -> sonnet/high (+1 effort rung)" })

// ---- CR5: Codex review of U5's diff ----
// depends_on: U5 (barrier)
const CR5 = await agent(
  "CR5: Drive the operator's herdr-managed Codex reviewer pane to review U5's diff, judge its findings, and return the ones that survive with file:line. Mechanics: resolve the reviewer pane via `herdr agent list` / `herdr tab list` using the machine-local identity the driver injects from the saga state; send the review request with `herdr pane send-text` (then `send-keys` Enter), wait for the agent to settle with `herdr pane wait-output` or agent_status polling, and read the reply with `herdr pane read --source recent-unwrapped` (a `visible` read presents stale content as a confident answer - do not use it). Capture the pane's reply verbatim as the review evidence (an uncaptured review is treated as not having run). The unit's spec is docs/plans/2026-08-09-talaria-v0-2-block-markdown-plan.md unit U5.\n\n## Presentation contract (Infiquetra house style)\n\nYour output is read by another agent, or relayed by a main thread to one operator who is supervising\nseveral workstreams at once. Write for that reader, not for someone who watched you work.\n\n**A stated return contract always wins.** If your instructions specify a return shape \u2014 a JSON object,\na named schema, a structured-output tool call, a required final message \u2014 obey it exactly and ignore\nanything below that would conflict with it. These rules govern the prose you write; they never reshape\na required return value.\n\n**Lead with the answer.** The first sentence says what you found or what is now true. A recap of your\nassignment, a list of the files you opened, and a narration of your process are not findings and do not\nopen a report.\n\n**Report state, not activity.** \"The migration runs clean on Postgres 16\" is state. \"I ran the\nmigration and then checked the logs\" is activity. State is what your caller can act on.\n\n**Situate before you detail.** One sentence naming the repository, host, or system in play, before any\nnumber, path, or identifier. Whoever reads you was not in your context.\n\n**Name the thing; never gesture at it.** A commit hash, issue number, pull-request number, branch, test\nname, or `path:line` reference appears in apposition to a noun saying what it is \u2014 \"pull request 656\",\n\"the emitter at `execution_spec.py:3244`\" \u2014 never as a sentence's subject or object on its own. The\nsame goes for unanchored roles: say the repository, the host, the path, not \"the receiver\" or \"the\ndownstream job\".\n\n**Quote only what is load-bearing.** Reproduce exact error strings, diff hunks, and command output\nwhose precise characters matter. Do not paste a whole file, a whole log, or a whole payload and leave\nthe reading to your caller \u2014 digesting it is the work you were spawned to do.\n\n**No unrequested visual.** No diagram, table, banner, or drawn box unless your caller asked for one, or\nyou are comparing three or more items that share attributes, which is a Markdown table. Use Mermaid\nonly in text destined for a file, a pull-request body, or a rendered artifact \u2014 never in a payload\nbound for a terminal. Box-drawing characters are for file-tree connectors and genuine pictures only,\nnever for callouts, banners, or emphasis.\n\n**No operator ceremony.** The operator-facing closing block and the main thread's style tell belong to\nthe main thread alone. Do not write either one. End when your content ends.\n\n**Say what you did not verify.** An unverified inference is labelled as one, in the same sentence.\n\"I did not check X\" is a finding; a confident guess that reads like a measurement is a defect that\npropagates, because your caller cannot tell the two apart from the outside.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys pane_reply_captured_verbatim, surviving_findings_with_file_line, verdict_pass_or_findings -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Codex review of U5's diff", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"pane_reply_captured_verbatim": {}, "surviving_findings_with_file_line": {}, "verdict_pass_or_findings": {}}, "required": ["pane_reply_captured_verbatim", "surviving_findings_with_file_line", "verdict_pass_or_findings"], "type": "object"} },
)
__gate(CR5, { unitId: "CR5", expectsOutput: true, returns: ["pane_reply_captured_verbatim", "surviving_findings_with_file_line", "verdict_pass_or_findings"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })

// ---- U6: The restated gate, the feature corpus, and the green re-run ----
// depends_on: CR5 (barrier)
const U6 = await agent(
  "U6: Replace interface_shows_everything's one-line-one-widget claim with the region-ownership proof (every projected source region of a committed block entry owned by a mounted visible block whose source_range covers it, construct-specific visual assertions; line-rendered kinds keep the window comparison); grow the deterministic feature corpus the plan's R14 scopes (every construct, early termination by cancel/error/disconnect, parser attacks, kind groups, 80-column resize, long-unclosed-fence and growing-table workloads with latency thresholds); assert progressiveness at timed checkpoints; compare replay determinism on normalized block structure under pinned width/theme/framework; re-run the gate green over existing AND feature corpora and record the measured verdict with high-water figures. Read docs/plans/2026-08-09-talaria-v0-2-block-markdown-plan.md (unit U6, KTD1/KTD8, R5/R11/R12/R13/R14/R17) as your authoritative spec.\n\n## Presentation contract (Infiquetra house style)\n\nYour output is read by another agent, or relayed by a main thread to one operator who is supervising\nseveral workstreams at once. Write for that reader, not for someone who watched you work.\n\n**A stated return contract always wins.** If your instructions specify a return shape \u2014 a JSON object,\na named schema, a structured-output tool call, a required final message \u2014 obey it exactly and ignore\nanything below that would conflict with it. These rules govern the prose you write; they never reshape\na required return value.\n\n**Lead with the answer.** The first sentence says what you found or what is now true. A recap of your\nassignment, a list of the files you opened, and a narration of your process are not findings and do not\nopen a report.\n\n**Report state, not activity.** \"The migration runs clean on Postgres 16\" is state. \"I ran the\nmigration and then checked the logs\" is activity. State is what your caller can act on.\n\n**Situate before you detail.** One sentence naming the repository, host, or system in play, before any\nnumber, path, or identifier. Whoever reads you was not in your context.\n\n**Name the thing; never gesture at it.** A commit hash, issue number, pull-request number, branch, test\nname, or `path:line` reference appears in apposition to a noun saying what it is \u2014 \"pull request 656\",\n\"the emitter at `execution_spec.py:3244`\" \u2014 never as a sentence's subject or object on its own. The\nsame goes for unanchored roles: say the repository, the host, the path, not \"the receiver\" or \"the\ndownstream job\".\n\n**Quote only what is load-bearing.** Reproduce exact error strings, diff hunks, and command output\nwhose precise characters matter. Do not paste a whole file, a whole log, or a whole payload and leave\nthe reading to your caller \u2014 digesting it is the work you were spawned to do.\n\n**No unrequested visual.** No diagram, table, banner, or drawn box unless your caller asked for one, or\nyou are comparing three or more items that share attributes, which is a Markdown table. Use Mermaid\nonly in text destined for a file, a pull-request body, or a rendered artifact \u2014 never in a payload\nbound for a terminal. Box-drawing characters are for file-tree connectors and genuine pictures only,\nnever for callouts, banners, or emphasis.\n\n**No operator ceremony.** The operator-facing closing block and the main thread's style tell belong to\nthe main thread alone. Do not write either one. End when your content ends.\n\n**Say what you did not verify.** An unverified inference is labelled as one, in the same sentence.\n\"I did not check X\" is a finding; a confident guess that reads like a measurement is a defect that\npropagates, because your caller cannot tell the two apart from the outside.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys region_ownership_proof_replaces_line_window_claim, progressiveness_asserted_at_timed_checkpoints, adversarial_workloads_hold_ceilings_with_high_water, replay_determinism_on_normalized_block_structure, early_termination_renders_all_received_content, gate_green_over_existing_and_feature_corpora -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "The restated gate, the feature corpus, and the green re-run", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"adversarial_workloads_hold_ceilings_with_high_water": {}, "early_termination_renders_all_received_content": {}, "gate_green_over_existing_and_feature_corpora": {}, "progressiveness_asserted_at_timed_checkpoints": {}, "region_ownership_proof_replaces_line_window_claim": {}, "replay_determinism_on_normalized_block_structure": {}}, "required": ["region_ownership_proof_replaces_line_window_claim", "progressiveness_asserted_at_timed_checkpoints", "adversarial_workloads_hold_ceilings_with_high_water", "replay_determinism_on_normalized_block_structure", "early_termination_renders_all_received_content", "gate_green_over_existing_and_feature_corpora"], "type": "object"} },
)
__gate(U6, { unitId: "U6", expectsOutput: true, returns: ["region_ownership_proof_replaces_line_window_claim", "progressiveness_asserted_at_timed_checkpoints", "adversarial_workloads_hold_ceilings_with_high_water", "replay_determinism_on_normalized_block_structure", "early_termination_renders_all_received_content", "gate_green_over_existing_and_feature_corpora"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })

// ---- CR6: Codex review of U6's diff ----
// depends_on: U6 (barrier)
const CR6 = await agent(
  "CR6: Drive the operator's herdr-managed Codex reviewer pane to review U6's diff (the restated gate and corpus), judge its findings, and return the ones that survive with file:line. Mechanics: resolve the reviewer pane via `herdr agent list` / `herdr tab list` using the machine-local identity the driver injects from the saga state; send the review request with `herdr pane send-text` (then `send-keys` Enter), wait for the agent to settle with `herdr pane wait-output` or agent_status polling, and read the reply with `herdr pane read --source recent-unwrapped` (a `visible` read presents stale content as a confident answer - do not use it). Capture the pane's reply verbatim as the review evidence (an uncaptured review is treated as not having run). The unit's spec is docs/plans/2026-08-09-talaria-v0-2-block-markdown-plan.md unit U6.\n\n## Presentation contract (Infiquetra house style)\n\nYour output is read by another agent, or relayed by a main thread to one operator who is supervising\nseveral workstreams at once. Write for that reader, not for someone who watched you work.\n\n**A stated return contract always wins.** If your instructions specify a return shape \u2014 a JSON object,\na named schema, a structured-output tool call, a required final message \u2014 obey it exactly and ignore\nanything below that would conflict with it. These rules govern the prose you write; they never reshape\na required return value.\n\n**Lead with the answer.** The first sentence says what you found or what is now true. A recap of your\nassignment, a list of the files you opened, and a narration of your process are not findings and do not\nopen a report.\n\n**Report state, not activity.** \"The migration runs clean on Postgres 16\" is state. \"I ran the\nmigration and then checked the logs\" is activity. State is what your caller can act on.\n\n**Situate before you detail.** One sentence naming the repository, host, or system in play, before any\nnumber, path, or identifier. Whoever reads you was not in your context.\n\n**Name the thing; never gesture at it.** A commit hash, issue number, pull-request number, branch, test\nname, or `path:line` reference appears in apposition to a noun saying what it is \u2014 \"pull request 656\",\n\"the emitter at `execution_spec.py:3244`\" \u2014 never as a sentence's subject or object on its own. The\nsame goes for unanchored roles: say the repository, the host, the path, not \"the receiver\" or \"the\ndownstream job\".\n\n**Quote only what is load-bearing.** Reproduce exact error strings, diff hunks, and command output\nwhose precise characters matter. Do not paste a whole file, a whole log, or a whole payload and leave\nthe reading to your caller \u2014 digesting it is the work you were spawned to do.\n\n**No unrequested visual.** No diagram, table, banner, or drawn box unless your caller asked for one, or\nyou are comparing three or more items that share attributes, which is a Markdown table. Use Mermaid\nonly in text destined for a file, a pull-request body, or a rendered artifact \u2014 never in a payload\nbound for a terminal. Box-drawing characters are for file-tree connectors and genuine pictures only,\nnever for callouts, banners, or emphasis.\n\n**No operator ceremony.** The operator-facing closing block and the main thread's style tell belong to\nthe main thread alone. Do not write either one. End when your content ends.\n\n**Say what you did not verify.** An unverified inference is labelled as one, in the same sentence.\n\"I did not check X\" is a finding; a confident guess that reads like a measurement is a defect that\npropagates, because your caller cannot tell the two apart from the outside.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys pane_reply_captured_verbatim, surviving_findings_with_file_line, verdict_pass_or_findings -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Codex review of U6's diff", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"pane_reply_captured_verbatim": {}, "surviving_findings_with_file_line": {}, "verdict_pass_or_findings": {}}, "required": ["pane_reply_captured_verbatim", "surviving_findings_with_file_line", "verdict_pass_or_findings"], "type": "object"} },
)
__gate(CR6, { unitId: "CR6", expectsOutput: true, returns: ["pane_reply_captured_verbatim", "surviving_findings_with_file_line", "verdict_pass_or_findings"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })

if (__pulledCords.length > 0) {
  throw __halt(`pull-cord (#364): ${__pulledCords.length} unit(s) self-reported out of depth -- ` +
    __pulledCords.map((c) => `${c.unit}: ${c.reason}` + (c.proposal ? ` (propose ${c.proposal})` : ' (no legal climb: top of ladder or session ceiling -- HALT)')).join('; ') +
    '. ONE batched escalation ask -- confirm climbs via /tier patch and re-emit.')
}

return {
  units: { "U1": U1, "CR1": CR1, "U2": U2, "CR2": CR2, "U3": U3, "CR3": CR3, "U4": U4, "CR4": CR4, "U5": U5, "CR5": CR5, "U6": U6, "CR6": CR6 },
  advisory_corrections: __advisories,
}
