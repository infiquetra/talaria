// ===========================================================================
// talaria-model-picker-and-v0-1-closure -- emitted Claude Code workflow harness.
// AUTO-EMITTED from a structured execution-spec by execution_spec.py.
// CONTROL FLOW ONLY -- every agent reads the plan as its authoritative spec.
// Per-unit {model, effort} tiers (R2(b)); R3 pilot/fan-out same-tier +
// R10 enumerated-target reconciliation enforced at emit time.
// ===========================================================================

export const meta = {
  name: "talaria-model-picker-and-v0-1-closure",
  description: "Add a model picker in three steps (session model, profile, that profile's default model) built on the Hermes HTTP API Talaria's existing credential already reaches, and close the v0.1 daily-driver gate with the observed live evidence that building it produces. Settles the one blocking row no test run can settle.",
}
const settlement = {"casualty_threshold_percent":0,"dispatch_id":"workflow:55efc6dc542eab2cfc66b556","driver":{"invocation_id":null,"units":[{"return_keys":[{"deliverable":"return:credential_form_decided_and_source_cited","result_key":"credential_form_decided_and_source_cited"},{"deliverable":"return:origin_derived_from_gateway_url_not_configured_separately","result_key":"origin_derived_from_gateway_url_not_configured_separately"},{"deliverable":"return:credential_absent_from_logs_reprs_and_exceptions","result_key":"credential_absent_from_logs_reprs_and_exceptions"},{"deliverable":"return:http_401_404_refused_and_non_json_each_named","result_key":"http_401_404_refused_and_non_json_each_named"},{"deliverable":"return:oversized_body_refused_at_cap","result_key":"oversized_body_refused_at_cap"},{"deliverable":"return:ktd2_confirmed_or_revised_in_journal","result_key":"ktd2_confirmed_or_revised_in_journal"}],"settlement_unit_id":"U1","workflow_unit_id":"U1"},{"return_keys":[{"deliverable":"return:environment_no_longer_consulted_for_a_credential","result_key":"environment_no_longer_consulted_for_a_credential"},{"deliverable":"return:refusal_messages_name_only_surviving_routes","result_key":"refusal_messages_name_only_surviving_routes"},{"deliverable":"return:rotation_guarantee_re_expressed_against_the_file","result_key":"rotation_guarantee_re_expressed_against_the_file"},{"deliverable":"return:decision_written_into_readme","result_key":"decision_written_into_readme"},{"deliverable":"return:r1_wording_not_widened","result_key":"r1_wording_not_widened"},{"deliverable":"return:environment_half_failure_assertion_still_asserts_failure","result_key":"environment_half_failure_assertion_still_asserts_failure"},{"deliverable":"return:row_13_grading_limit_recorded_for_u7","result_key":"row_13_grading_limit_recorded_for_u7"}],"settlement_unit_id":"U3","workflow_unit_id":"U3"},{"return_keys":[{"deliverable":"return:providers_and_models_listed_with_current_marked","result_key":"providers_and_models_listed_with_current_marked"},{"deliverable":"return:unauthenticated_provider_visibly_distinguished","result_key":"unauthenticated_provider_visibly_distinguished"},{"deliverable":"return:selection_emits_exact_model_command_string","result_key":"selection_emits_exact_model_command_string"},{"deliverable":"return:composer_keeps_focus","result_key":"composer_keeps_focus"},{"deliverable":"return:unfetched_failed_and_warning_states_each_distinguishable","result_key":"unfetched_failed_and_warning_states_each_distinguishable"},{"deliverable":"return:stale_epoch_selection_refused","result_key":"stale_epoch_selection_refused"}],"settlement_unit_id":"U2","workflow_unit_id":"U2"},{"return_keys":[{"deliverable":"return:profiles_listed_with_dialability_marked","result_key":"profiles_listed_with_dialability_marked"},{"deliverable":"return:non_running_profile_refused_before_dial","result_key":"non_running_profile_refused_before_dial"},{"deliverable":"return:credential_refusal_surfaces_as_unavailable_with_reason","result_key":"credential_refusal_surfaces_as_unavailable_with_reason"},{"deliverable":"return:failed_switch_leaves_a_named_state","result_key":"failed_switch_leaves_a_named_state"},{"deliverable":"return:profiles_active_post_never_called","result_key":"profiles_active_post_never_called"},{"deliverable":"return:fixtures_use_synthetic_profile_names","result_key":"fixtures_use_synthetic_profile_names"}],"settlement_unit_id":"U4","workflow_unit_id":"U4"},{"return_keys":[{"deliverable":"return:confirmation_requires_two_distinct_acts","result_key":"confirmation_requires_two_distinct_acts"},{"deliverable":"return:first_call_never_carries_confirm_expensive_model","result_key":"first_call_never_carries_confirm_expensive_model"},{"deliverable":"return:new_sessions_only_stated_on_screen","result_key":"new_sessions_only_stated_on_screen"},{"deliverable":"return:http_400_named_rather_than_raised","result_key":"http_400_named_rather_than_raised"}],"settlement_unit_id":"U5","workflow_unit_id":"U5"},{"return_keys":[{"deliverable":"return:checklist_prepared_for_operator","result_key":"checklist_prepared_for_operator"},{"deliverable":"return:resume_and_session_paths_recorded","result_key":"resume_and_session_paths_recorded"},{"deliverable":"return:compatibility_check_output_captured","result_key":"compatibility_check_output_captured"},{"deliverable":"return:authentication_failure_branch_observed","result_key":"authentication_failure_branch_observed"},{"deliverable":"return:absent_capability_branch_observed","result_key":"absent_capability_branch_observed"},{"deliverable":"return:f1_and_f7_observed_by_a_person","result_key":"f1_and_f7_observed_by_a_person"},{"deliverable":"return:corpora_cited_by_digest_and_count_only","result_key":"corpora_cited_by_digest_and_count_only"}],"settlement_unit_id":"U6","workflow_unit_id":"U6"},{"return_keys":[{"deliverable":"return:row_6_graded_by_enumerated_methods_from_recordings","result_key":"row_6_graded_by_enumerated_methods_from_recordings"},{"deliverable":"return:rows_that_did_not_clear_stated_as_still_blocking","result_key":"rows_that_did_not_clear_stated_as_still_blocking"},{"deliverable":"return:evidence_table_and_gate_block_restated_together","result_key":"evidence_table_and_gate_block_restated_together"},{"deliverable":"return:backlinks_name_only_cleared_conditions","result_key":"backlinks_name_only_cleared_conditions"},{"deliverable":"return:gating_document_tests_pass","result_key":"gating_document_tests_pass"}],"settlement_unit_id":"U7","workflow_unit_id":"U7"}]},"max_attempts":3,"schema":"dispatch_settlement.v1","site":"workflow","units":[{"deliverables":["structured-result","return:credential_form_decided_and_source_cited","return:origin_derived_from_gateway_url_not_configured_separately","return:credential_absent_from_logs_reprs_and_exceptions","return:http_401_404_refused_and_non_json_each_named","return:oversized_body_refused_at_cap","return:ktd2_confirmed_or_revised_in_journal"],"idempotency_key":"workflow:55efc6dc542eab2cfc66b556:U1","unit_id":"U1"},{"deliverables":["structured-result","return:environment_no_longer_consulted_for_a_credential","return:refusal_messages_name_only_surviving_routes","return:rotation_guarantee_re_expressed_against_the_file","return:decision_written_into_readme","return:r1_wording_not_widened","return:environment_half_failure_assertion_still_asserts_failure","return:row_13_grading_limit_recorded_for_u7"],"idempotency_key":"workflow:55efc6dc542eab2cfc66b556:U3","unit_id":"U3"},{"deliverables":["structured-result","return:providers_and_models_listed_with_current_marked","return:unauthenticated_provider_visibly_distinguished","return:selection_emits_exact_model_command_string","return:composer_keeps_focus","return:unfetched_failed_and_warning_states_each_distinguishable","return:stale_epoch_selection_refused"],"idempotency_key":"workflow:55efc6dc542eab2cfc66b556:U2","unit_id":"U2"},{"deliverables":["structured-result","return:profiles_listed_with_dialability_marked","return:non_running_profile_refused_before_dial","return:credential_refusal_surfaces_as_unavailable_with_reason","return:failed_switch_leaves_a_named_state","return:profiles_active_post_never_called","return:fixtures_use_synthetic_profile_names"],"idempotency_key":"workflow:55efc6dc542eab2cfc66b556:U4","unit_id":"U4"},{"deliverables":["structured-result","return:confirmation_requires_two_distinct_acts","return:first_call_never_carries_confirm_expensive_model","return:new_sessions_only_stated_on_screen","return:http_400_named_rather_than_raised"],"idempotency_key":"workflow:55efc6dc542eab2cfc66b556:U5","unit_id":"U5"},{"deliverables":["structured-result","return:checklist_prepared_for_operator","return:resume_and_session_paths_recorded","return:compatibility_check_output_captured","return:authentication_failure_branch_observed","return:absent_capability_branch_observed","return:f1_and_f7_observed_by_a_person","return:corpora_cited_by_digest_and_count_only"],"idempotency_key":"workflow:55efc6dc542eab2cfc66b556:U6","unit_id":"U6"},{"deliverables":["structured-result","return:row_6_graded_by_enumerated_methods_from_recordings","return:rows_that_did_not_clear_stated_as_still_blocking","return:evidence_table_and_gate_block_restated_together","return:backlinks_name_only_cleared_conditions","return:gating_document_tests_pass"],"idempotency_key":"workflow:55efc6dc542eab2cfc66b556:U7","unit_id":"U7"}]}

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

// ---- U1: The admin HTTP client, and the credential form settled against Hermes source ----
// ---- U3: Remove the environment credential source and re-grade row 13 ----
// concurrency chunk 1/2 (max_concurrent=1)
const [U1] = await parallel([
  () =>
    __retry(() => agent(
      "U1: Build talaria/transport/admin.py and its pure decode, after first reading Hermes source to establish which credential form the HTTP surface supports and citing it. Read docs/plans/2026-08-06-model-picker-and-v0-1-closure-plan.md (unit U1, and KTD1/KTD2) as your authoritative spec. CONCURRENCY CONSTRAINT: U3 runs in the same wave and is deleting HERMES_DASHBOARD_SESSION_TOKEN as a credential source \u2014 no fixture, test or docstring you write may supply a credential through that variable. Use the credential file route or an injected provider.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys credential_form_decided_and_source_cited, origin_derived_from_gateway_url_not_configured_separately, credential_absent_from_logs_reprs_and_exceptions, http_401_404_refused_and_non_json_each_named, oversized_body_refused_at_cap, ktd2_confirmed_or_revised_in_journal -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
      { label: "The admin HTTP client, and the credential form settled against Hermes source", model: "opus", effort: "high", schema: {"additionalProperties": true, "properties": {"credential_absent_from_logs_reprs_and_exceptions": {}, "credential_form_decided_and_source_cited": {}, "http_401_404_refused_and_non_json_each_named": {}, "ktd2_confirmed_or_revised_in_journal": {}, "origin_derived_from_gateway_url_not_configured_separately": {}, "oversized_body_refused_at_cap": {}}, "required": ["credential_form_decided_and_source_cited", "origin_derived_from_gateway_url_not_configured_separately", "credential_absent_from_logs_reprs_and_exceptions", "http_401_404_refused_and_non_json_each_named", "oversized_body_refused_at_cap", "ktd2_confirmed_or_revised_in_journal"], "type": "object"} },
    ), { unitId: "U1", maxAttempts: 3 }),
])
__gate(U1, { unitId: "U1", expectsOutput: true, returns: ["credential_form_decided_and_source_cited", "origin_derived_from_gateway_url_not_configured_separately", "credential_absent_from_logs_reprs_and_exceptions", "http_401_404_refused_and_non_json_each_named", "oversized_body_refused_at_cap", "ktd2_confirmed_or_revised_in_journal"], cordProposal: "opus/high -> opus/xhigh (+1 effort rung)" })

// concurrency chunk 2/2 (max_concurrent=1)
const [U3] = await parallel([
  () =>
    __retry(() => agent(
      "U3: Execute option (b) \u2014 remove HERMES_DASHBOARD_SESSION_TOKEN from the credential precedence chain, write the remaining supported routes into README.md, re-express every affected test (rotation guarantees against the file, not deleted), and record in DECISIONS.md exactly how far option (b) lets row 13 be re-graded \u2014 U7 owns the verdict document and grades against what you record; do not edit it yourself. Read docs/plans/2026-08-06-model-picker-and-v0-1-closure-plan.md (unit U3, KTD8) and docs/engineering-journal/QUEUED.md's entry 'R1's environment clause is unmet, and no change to Talaria can meet it' as your authoritative spec. Do not write that R1's environment clause is now technically met: an inherited variable stays visible in /proc/<pid>/environ and no code change touches that.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys environment_no_longer_consulted_for_a_credential, refusal_messages_name_only_surviving_routes, rotation_guarantee_re_expressed_against_the_file, decision_written_into_readme, r1_wording_not_widened, environment_half_failure_assertion_still_asserts_failure, row_13_grading_limit_recorded_for_u7 -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
      { label: "Remove the environment credential source and re-grade row 13", model: "opus", effort: "high", schema: {"additionalProperties": true, "properties": {"decision_written_into_readme": {}, "environment_half_failure_assertion_still_asserts_failure": {}, "environment_no_longer_consulted_for_a_credential": {}, "r1_wording_not_widened": {}, "refusal_messages_name_only_surviving_routes": {}, "rotation_guarantee_re_expressed_against_the_file": {}, "row_13_grading_limit_recorded_for_u7": {}}, "required": ["environment_no_longer_consulted_for_a_credential", "refusal_messages_name_only_surviving_routes", "rotation_guarantee_re_expressed_against_the_file", "decision_written_into_readme", "r1_wording_not_widened", "environment_half_failure_assertion_still_asserts_failure", "row_13_grading_limit_recorded_for_u7"], "type": "object"} },
    ), { unitId: "U3", maxAttempts: 3 }),
])
__gate(U3, { unitId: "U3", expectsOutput: true, returns: ["environment_no_longer_consulted_for_a_credential", "refusal_messages_name_only_surviving_routes", "rotation_guarantee_re_expressed_against_the_file", "decision_written_into_readme", "r1_wording_not_widened", "environment_half_failure_assertion_still_asserts_failure", "row_13_grading_limit_recorded_for_u7"], cordProposal: "opus/high -> opus/xhigh (+1 effort rung)" })

// ---- U2: The session model picker ----
// depends_on: U1 (barrier)
const U2 = await agent(
  "U2: Build the foldable model-picker region and the /models local command that opens it, dispatching selection through the existing slash path. Read docs/plans/2026-08-06-model-picker-and-v0-1-closure-plan.md (unit U2, KTD3/KTD4) as your authoritative spec \u2014 it names the six wiring points in talaria/ui/app.py and two hazards there (perform_local_command is synchronous while action_toggle_palette is async; PaletteRegion is opened by a key binding and no slash command). The command name is /models, plural: the gateway already owns singular /model.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys providers_and_models_listed_with_current_marked, unauthenticated_provider_visibly_distinguished, selection_emits_exact_model_command_string, composer_keeps_focus, unfetched_failed_and_warning_states_each_distinguishable, stale_epoch_selection_refused -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "The session model picker", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"composer_keeps_focus": {}, "providers_and_models_listed_with_current_marked": {}, "selection_emits_exact_model_command_string": {}, "stale_epoch_selection_refused": {}, "unauthenticated_provider_visibly_distinguished": {}, "unfetched_failed_and_warning_states_each_distinguishable": {}}, "required": ["providers_and_models_listed_with_current_marked", "unauthenticated_provider_visibly_distinguished", "selection_emits_exact_model_command_string", "composer_keeps_focus", "unfetched_failed_and_warning_states_each_distinguishable", "stale_epoch_selection_refused"], "type": "object"} },
)
__gate(U2, { unitId: "U2", expectsOutput: true, returns: ["providers_and_models_listed_with_current_marked", "unauthenticated_provider_visibly_distinguished", "selection_emits_exact_model_command_string", "composer_keeps_focus", "unfetched_failed_and_warning_states_each_distinguishable", "stale_epoch_selection_refused"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })

// ---- U4: The profile picker, and one credential per endpoint ----
// depends_on: U1, U2 (barrier)
const U4 = await agent(
  "U4: Add profile listing to the admin client and a profile mode to the picker behind the /profiles local command (plural: the gateway already owns singular /profile), where selection resolves a new endpoint, re-resolves the credential for it, and reconnects. Read docs/plans/2026-08-06-model-picker-and-v0-1-closure-plan.md (unit U4, KTD5/KTD6) as your authoritative spec. Use synthetic profile names in every fixture: this is a public repository.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys profiles_listed_with_dialability_marked, non_running_profile_refused_before_dial, credential_refusal_surfaces_as_unavailable_with_reason, failed_switch_leaves_a_named_state, profiles_active_post_never_called, fixtures_use_synthetic_profile_names -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "The profile picker, and one credential per endpoint", model: "opus", effort: "high", schema: {"additionalProperties": true, "properties": {"credential_refusal_surfaces_as_unavailable_with_reason": {}, "failed_switch_leaves_a_named_state": {}, "fixtures_use_synthetic_profile_names": {}, "non_running_profile_refused_before_dial": {}, "profiles_active_post_never_called": {}, "profiles_listed_with_dialability_marked": {}}, "required": ["profiles_listed_with_dialability_marked", "non_running_profile_refused_before_dial", "credential_refusal_surfaces_as_unavailable_with_reason", "failed_switch_leaves_a_named_state", "profiles_active_post_never_called", "fixtures_use_synthetic_profile_names"], "type": "object"} },
)
__gate(U4, { unitId: "U4", expectsOutput: true, returns: ["profiles_listed_with_dialability_marked", "non_running_profile_refused_before_dial", "credential_refusal_surfaces_as_unavailable_with_reason", "failed_switch_leaves_a_named_state", "profiles_active_post_never_called", "fixtures_use_synthetic_profile_names"], cordProposal: "opus/high -> opus/xhigh (+1 effort rung)" })

// ---- U5: The default-model picker, with the expensive-model confirmation surfaced ----
// depends_on: U4 (barrier)
const U5 = await agent(
  "U5: Add the profile-scoped default-model write and its two-act confirmation flow. Read docs/plans/2026-08-06-model-picker-and-v0-1-closure-plan.md (unit U5, KTD7) as your authoritative spec.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys confirmation_requires_two_distinct_acts, first_call_never_carries_confirm_expensive_model, new_sessions_only_stated_on_screen, http_400_named_rather_than_raised -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "The default-model picker, with the expensive-model confirmation surfaced", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"confirmation_requires_two_distinct_acts": {}, "first_call_never_carries_confirm_expensive_model": {}, "http_400_named_rather_than_raised": {}, "new_sessions_only_stated_on_screen": {}}, "required": ["confirmation_requires_two_distinct_acts", "first_call_never_carries_confirm_expensive_model", "new_sessions_only_stated_on_screen", "http_400_named_rather_than_raised"], "type": "object"} },
)
__gate(U5, { unitId: "U5", expectsOutput: true, returns: ["confirmation_requires_two_distinct_acts", "first_call_never_carries_confirm_expensive_model", "new_sessions_only_stated_on_screen", "http_400_named_rather_than_raised"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })

// ---- U6: Collate the scripted live acceptance run ----
// depends_on: U5, U3 (barrier)
const U6 = await agent(
  "U6: Prepare the row-19 checklist for the operator to execute, then collate the recordings they produce into digests and counts. The runs themselves are the operator's; F7 and the isolated-session observation cannot be automated. Read docs/plans/2026-08-06-model-picker-and-v0-1-closure-plan.md (unit U6, KTD9) as your authoritative spec.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys checklist_prepared_for_operator, resume_and_session_paths_recorded, compatibility_check_output_captured, authentication_failure_branch_observed, absent_capability_branch_observed, f1_and_f7_observed_by_a_person, corpora_cited_by_digest_and_count_only -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Collate the scripted live acceptance run", model: "sonnet", effort: "low", schema: {"additionalProperties": true, "properties": {"absent_capability_branch_observed": {}, "authentication_failure_branch_observed": {}, "checklist_prepared_for_operator": {}, "compatibility_check_output_captured": {}, "corpora_cited_by_digest_and_count_only": {}, "f1_and_f7_observed_by_a_person": {}, "resume_and_session_paths_recorded": {}}, "required": ["checklist_prepared_for_operator", "resume_and_session_paths_recorded", "compatibility_check_output_captured", "authentication_failure_branch_observed", "absent_capability_branch_observed", "f1_and_f7_observed_by_a_person", "corpora_cited_by_digest_and_count_only"], "type": "object"} },
)
__gate(U6, { unitId: "U6", expectsOutput: true, returns: ["checklist_prepared_for_operator", "resume_and_session_paths_recorded", "compatibility_check_output_captured", "authentication_failure_branch_observed", "absent_capability_branch_observed", "f1_and_f7_observed_by_a_person", "corpora_cited_by_digest_and_count_only"], cordProposal: "sonnet/low -> sonnet/medium (+1 effort rung)" })

// ---- U7: Re-grade the gate on what was actually measured ----
// depends_on: U6, U3 (barrier)
const U7 = await agent(
  "U7: Re-grade rows 6, 13 and 19 against what was measured, restate the evidence table and the gate block together, and leave the verdict NOT READY on any reason that did not clear. Read docs/plans/2026-08-06-model-picker-and-v0-1-closure-plan.md (unit U7, R10/R13) as your authoritative spec.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys row_6_graded_by_enumerated_methods_from_recordings, rows_that_did_not_clear_stated_as_still_blocking, evidence_table_and_gate_block_restated_together, backlinks_name_only_cleared_conditions, gating_document_tests_pass -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Re-grade the gate on what was actually measured", model: "opus", effort: "high", schema: {"additionalProperties": true, "properties": {"backlinks_name_only_cleared_conditions": {}, "evidence_table_and_gate_block_restated_together": {}, "gating_document_tests_pass": {}, "row_6_graded_by_enumerated_methods_from_recordings": {}, "rows_that_did_not_clear_stated_as_still_blocking": {}}, "required": ["row_6_graded_by_enumerated_methods_from_recordings", "rows_that_did_not_clear_stated_as_still_blocking", "evidence_table_and_gate_block_restated_together", "backlinks_name_only_cleared_conditions", "gating_document_tests_pass"], "type": "object"} },
)
__gate(U7, { unitId: "U7", expectsOutput: true, returns: ["row_6_graded_by_enumerated_methods_from_recordings", "rows_that_did_not_clear_stated_as_still_blocking", "evidence_table_and_gate_block_restated_together", "backlinks_name_only_cleared_conditions", "gating_document_tests_pass"], cordProposal: "opus/high -> opus/xhigh (+1 effort rung)" })

if (__pulledCords.length > 0) {
  throw __halt(`pull-cord (#364): ${__pulledCords.length} unit(s) self-reported out of depth -- ` +
    __pulledCords.map((c) => `${c.unit}: ${c.reason}` + (c.proposal ? ` (propose ${c.proposal})` : ' (no legal climb: top of ladder or session ceiling -- HALT)')).join('; ') +
    '. ONE batched escalation ask -- confirm climbs via /tier patch and re-emit.')
}

return {
  units: { "U1": U1, "U3": U3, "U2": U2, "U4": U4, "U5": U5, "U6": U6, "U7": U7 },
  advisory_corrections: __advisories,
}
