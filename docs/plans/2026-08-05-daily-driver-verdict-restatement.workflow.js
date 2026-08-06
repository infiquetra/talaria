// ===========================================================================
// talaria-daily-driver-verdict-restatement -- emitted Claude Code workflow harness.
// AUTO-EMITTED from a structured execution-spec by execution_spec.py.
// CONTROL FLOW ONLY -- every agent reads the plan as its authoritative spec.
// Per-unit {model, effort} tiers (R2(b)); R3 pilot/fan-out same-tier +
// R10 enumerated-target reconciliation enforced at emit time.
// ===========================================================================

export const meta = {
  name: "talaria-daily-driver-verdict-restatement",
  description: "Close DRIFT-04, the last open finding of the R1-R40 conformance audit: the v0.1 daily-driver verdict grades R2 and R3 unmet on reasons that stopped being true on 2026-08-04. Re-grade those rows against the live corpus, re-read the rows the finding never mentions, restate the verdict on the corrected table, and close the finding out.",
}
const settlement = {"casualty_threshold_percent":0,"dispatch_id":"workflow:1a1b26341260191337ad0c2d","driver":{"invocation_id":null,"units":[{"return_keys":[{"deliverable":"return:measurement_method_stated","result_key":"measurement_method_stated"},{"deliverable":"return:corpus_digest_and_count","result_key":"corpus_digest_and_count"},{"deliverable":"return:r2_row_text_and_evidence_cited","result_key":"r2_row_text_and_evidence_cited"},{"deliverable":"return:r3_streaming_evidence_cited","result_key":"r3_streaming_evidence_cited"},{"deliverable":"return:r3_replay_comparison_evidence_cited","result_key":"r3_replay_comparison_evidence_cited"},{"deliverable":"return:numbers_reproduced_from_clean_run","result_key":"numbers_reproduced_from_clean_run"}],"settlement_unit_id":"U1","workflow_unit_id":"U1"},{"return_keys":[{"deliverable":"return:row_6_present_state_and_method_counts","result_key":"row_6_present_state_and_method_counts"},{"deliverable":"return:row_13_present_state","result_key":"row_13_present_state"},{"deliverable":"return:row_19_present_state_and_f7_record_search","result_key":"row_19_present_state_and_f7_record_search"},{"deliverable":"return:five_items_present_state","result_key":"five_items_present_state"},{"deliverable":"return:items_reported_as_not_moved","result_key":"items_reported_as_not_moved"}],"settlement_unit_id":"U2","workflow_unit_id":"U2"},{"return_keys":[{"deliverable":"return:verdict_text_and_whether_it_moved","result_key":"verdict_text_and_whether_it_moved"},{"deliverable":"return:blocking_reasons_now_cited","result_key":"blocking_reasons_now_cited"},{"deliverable":"return:what_would_change_list_reordered","result_key":"what_would_change_list_reordered"},{"deliverable":"return:docstring_correction_with_prior_wording_recorded","result_key":"docstring_correction_with_prior_wording_recorded"},{"deliverable":"return:check_command_output","result_key":"check_command_output"}],"settlement_unit_id":"U3","workflow_unit_id":"U3"},{"return_keys":[{"deliverable":"return:register_entry_moved_and_counts_updated","result_key":"register_entry_moved_and_counts_updated"},{"deliverable":"return:queued_entry_removed","result_key":"queued_entry_removed"},{"deliverable":"return:readme_crossreference_updated","result_key":"readme_crossreference_updated"},{"deliverable":"return:register_self_correction_text","result_key":"register_self_correction_text"},{"deliverable":"return:journal_entries_filed","result_key":"journal_entries_filed"},{"deliverable":"return:drift_04_grep_sweep_results","result_key":"drift_04_grep_sweep_results"}],"settlement_unit_id":"U4","workflow_unit_id":"U4"}]},"max_attempts":3,"schema":"dispatch_settlement.v1","site":"workflow","units":[{"deliverables":["structured-result","return:measurement_method_stated","return:corpus_digest_and_count","return:r2_row_text_and_evidence_cited","return:r3_streaming_evidence_cited","return:r3_replay_comparison_evidence_cited","return:numbers_reproduced_from_clean_run"],"idempotency_key":"workflow:1a1b26341260191337ad0c2d:U1","unit_id":"U1"},{"deliverables":["structured-result","return:row_6_present_state_and_method_counts","return:row_13_present_state","return:row_19_present_state_and_f7_record_search","return:five_items_present_state","return:items_reported_as_not_moved"],"idempotency_key":"workflow:1a1b26341260191337ad0c2d:U2","unit_id":"U2"},{"deliverables":["structured-result","return:verdict_text_and_whether_it_moved","return:blocking_reasons_now_cited","return:what_would_change_list_reordered","return:docstring_correction_with_prior_wording_recorded","return:check_command_output"],"idempotency_key":"workflow:1a1b26341260191337ad0c2d:U3","unit_id":"U3"},{"deliverables":["structured-result","return:register_entry_moved_and_counts_updated","return:queued_entry_removed","return:readme_crossreference_updated","return:register_self_correction_text","return:journal_entries_filed","return:drift_04_grep_sweep_results"],"idempotency_key":"workflow:1a1b26341260191337ad0c2d:U4","unit_id":"U4"}]}

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

// ---- U1: Re-grade the R2 and R3 rows against the live corpus, with citations ----
const U1 = await agent(
  "U1: Re-grade the daily-driver verdict's R2 and R3 rows (evidence-table rows 17 and 18) against the live recordings, updating them in place with citations to the specific frames that settle each. Read docs/plans/2026-08-05-daily-driver-verdict-restatement-plan.md (unit U1) as your authoritative spec.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys measurement_method_stated, corpus_digest_and_count, r2_row_text_and_evidence_cited, r3_streaming_evidence_cited, r3_replay_comparison_evidence_cited, numbers_reproduced_from_clean_run -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Re-grade the R2 and R3 rows against the live corpus, with citations", model: "opus", effort: "high", schema: {"additionalProperties": true, "properties": {"corpus_digest_and_count": {}, "measurement_method_stated": {}, "numbers_reproduced_from_clean_run": {}, "r2_row_text_and_evidence_cited": {}, "r3_replay_comparison_evidence_cited": {}, "r3_streaming_evidence_cited": {}}, "required": ["measurement_method_stated", "corpus_digest_and_count", "r2_row_text_and_evidence_cited", "r3_streaming_evidence_cited", "r3_replay_comparison_evidence_cited", "numbers_reproduced_from_clean_run"], "type": "object"} },
)
__gate(U1, { unitId: "U1", expectsOutput: true, returns: ["measurement_method_stated", "corpus_digest_and_count", "r2_row_text_and_evidence_cited", "r3_streaming_evidence_cited", "r3_replay_comparison_evidence_cited", "numbers_reproduced_from_clean_run"], cordProposal: "opus/high -> opus/xhigh (+1 effort rung)" })

// ---- U2: Re-read rows 6, 13 and 19 and the five what-would-change items, reporting what has NOT moved ----
// depends_on: U1 (barrier)
const U2 = await agent(
  "U2: Re-read evidence-table rows 6, 13 and 19 and the five 'What would change this verdict' items against current evidence, and state the present position of each -- including, explicitly, the ones that have not moved. Do not restate the verdict. Read docs/plans/2026-08-05-daily-driver-verdict-restatement-plan.md (unit U2) as your authoritative spec.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys row_6_present_state_and_method_counts, row_13_present_state, row_19_present_state_and_f7_record_search, five_items_present_state, items_reported_as_not_moved -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Re-read rows 6, 13 and 19 and the five what-would-change items, reporting what has NOT moved", model: "opus", effort: "high", schema: {"additionalProperties": true, "properties": {"five_items_present_state": {}, "items_reported_as_not_moved": {}, "row_13_present_state": {}, "row_19_present_state_and_f7_record_search": {}, "row_6_present_state_and_method_counts": {}}, "required": ["row_6_present_state_and_method_counts", "row_13_present_state", "row_19_present_state_and_f7_record_search", "five_items_present_state", "items_reported_as_not_moved"], "type": "object"} },
)
__gate(U2, { unitId: "U2", expectsOutput: true, returns: ["row_6_present_state_and_method_counts", "row_13_present_state", "row_19_present_state_and_f7_record_search", "five_items_present_state", "items_reported_as_not_moved"], cordProposal: "opus/high -> opus/xhigh (+1 effort rung)" })

// ---- U3: Restate the verdict on the corrected table, and correct the docstring that cites it ----
// depends_on: U2 (barrier)
const U3 = await agent(
  "U3: Restate the verdict on the table U1 and U2 produced, applying the document's own rule that any gap blocks a ready verdict, and correct the now-false sentence in open_session's docstring at talaria/ui/app.py. Read docs/plans/2026-08-05-daily-driver-verdict-restatement-plan.md (unit U3) as your authoritative spec.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys verdict_text_and_whether_it_moved, blocking_reasons_now_cited, what_would_change_list_reordered, docstring_correction_with_prior_wording_recorded, check_command_output -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Restate the verdict on the corrected table, and correct the docstring that cites it", model: "opus", effort: "high", schema: {"additionalProperties": true, "properties": {"blocking_reasons_now_cited": {}, "check_command_output": {}, "docstring_correction_with_prior_wording_recorded": {}, "verdict_text_and_whether_it_moved": {}, "what_would_change_list_reordered": {}}, "required": ["verdict_text_and_whether_it_moved", "blocking_reasons_now_cited", "what_would_change_list_reordered", "docstring_correction_with_prior_wording_recorded", "check_command_output"], "type": "object"} },
)
__gate(U3, { unitId: "U3", expectsOutput: true, returns: ["verdict_text_and_whether_it_moved", "blocking_reasons_now_cited", "what_would_change_list_reordered", "docstring_correction_with_prior_wording_recorded", "check_command_output"], cordProposal: "opus/high -> opus/xhigh (+1 effort rung)" })

// ---- U4: Close DRIFT-04 out across the register, the worklist and the index ----
// depends_on: U3 (barrier)
const U4 = await agent(
  "U4: Close DRIFT-04 out -- move its register entry to Resolved findings with the closing commit, update the register's counts and status header, remove its P1 entry from QUEUED.md, fix the README cross-reference, correct the register's own wrong 'exactly two things' claim in place, and file the journal entries. Read docs/plans/2026-08-05-daily-driver-verdict-restatement-plan.md (unit U4) as your authoritative spec.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys register_entry_moved_and_counts_updated, queued_entry_removed, readme_crossreference_updated, register_self_correction_text, journal_entries_filed, drift_04_grep_sweep_results -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Close DRIFT-04 out across the register, the worklist and the index", model: "sonnet", effort: "medium", schema: {"additionalProperties": true, "properties": {"drift_04_grep_sweep_results": {}, "journal_entries_filed": {}, "queued_entry_removed": {}, "readme_crossreference_updated": {}, "register_entry_moved_and_counts_updated": {}, "register_self_correction_text": {}}, "required": ["register_entry_moved_and_counts_updated", "queued_entry_removed", "readme_crossreference_updated", "register_self_correction_text", "journal_entries_filed", "drift_04_grep_sweep_results"], "type": "object"} },
)
__gate(U4, { unitId: "U4", expectsOutput: true, returns: ["register_entry_moved_and_counts_updated", "queued_entry_removed", "readme_crossreference_updated", "register_self_correction_text", "journal_entries_filed", "drift_04_grep_sweep_results"], cordProposal: "sonnet/medium -> sonnet/high (+1 effort rung)" })

if (__pulledCords.length > 0) {
  throw __halt(`pull-cord (#364): ${__pulledCords.length} unit(s) self-reported out of depth -- ` +
    __pulledCords.map((c) => `${c.unit}: ${c.reason}` + (c.proposal ? ` (propose ${c.proposal})` : ' (no legal climb: top of ladder or session ceiling -- HALT)')).join('; ') +
    '. ONE batched escalation ask -- confirm climbs via /tier patch and re-emit.')
}

return {
  units: { "U1": U1, "U2": U2, "U3": U3, "U4": U4 },
  advisory_corrections: __advisories,
}
