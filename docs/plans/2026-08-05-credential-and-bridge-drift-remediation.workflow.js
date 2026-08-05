// ===========================================================================
// talaria-drift-remediation -- emitted Claude Code workflow harness.
// AUTO-EMITTED from a structured execution-spec by execution_spec.py.
// CONTROL FLOW ONLY -- every agent reads the plan as its authoritative spec.
// Per-unit {model, effort} tiers (R2(b)); R3 pilot/fan-out same-tier +
// R10 enumerated-target reconciliation enforced at emit time.
// ===========================================================================

export const meta = {
  name: "talaria-drift-remediation",
  description: "Close the two v0.1 conformance findings against R9: talaria record can only authenticate by putting a live token on the command line (DRIFT-03), and the recorder redaction test covers three of the four blocking bridges (DRIFT-01).",
}
const settlement = {"casualty_threshold_percent":0,"dispatch_id":"workflow:c52cefc0c0e62e8b574e9267","driver":{"invocation_id":null,"units":[{"return_keys":[{"deliverable":"return:new_test_name","result_key":"new_test_name"},{"deliverable":"return:red_demonstration_output","result_key":"red_demonstration_output"},{"deliverable":"return:redaction_reason_asserted","result_key":"redaction_reason_asserted"},{"deliverable":"return:check_command_output","result_key":"check_command_output"}],"settlement_unit_id":"U1","workflow_unit_id":"U1"},{"return_keys":[{"deliverable":"return:refusal_message_text","result_key":"refusal_message_text"},{"deliverable":"return:credential_absent_from_message_evidence","result_key":"credential_absent_from_message_evidence"},{"deliverable":"return:dial_url_test_results","result_key":"dial_url_test_results"},{"deliverable":"return:readme_diff_summary","result_key":"readme_diff_summary"},{"deliverable":"return:check_command_output","result_key":"check_command_output"}],"settlement_unit_id":"U2","workflow_unit_id":"U2"},{"return_keys":[{"deliverable":"return:entry_points_probed","result_key":"entry_points_probed"},{"deliverable":"return:guard_test_name","result_key":"guard_test_name"},{"deliverable":"return:guard_red_demonstration_output","result_key":"guard_red_demonstration_output"},{"deliverable":"return:sweep_results_per_entry_point","result_key":"sweep_results_per_entry_point"},{"deliverable":"return:document_corrections_summary","result_key":"document_corrections_summary"},{"deliverable":"return:check_command_output","result_key":"check_command_output"}],"settlement_unit_id":"U3","workflow_unit_id":"U3"}]},"max_attempts":3,"schema":"dispatch_settlement.v1","site":"workflow","units":[{"deliverables":["structured-result","return:new_test_name","return:red_demonstration_output","return:redaction_reason_asserted","return:check_command_output"],"idempotency_key":"workflow:c52cefc0c0e62e8b574e9267:U1","unit_id":"U1"},{"deliverables":["structured-result","return:refusal_message_text","return:credential_absent_from_message_evidence","return:dial_url_test_results","return:readme_diff_summary","return:check_command_output"],"idempotency_key":"workflow:c52cefc0c0e62e8b574e9267:U2","unit_id":"U2"},{"deliverables":["structured-result","return:entry_points_probed","return:guard_test_name","return:guard_red_demonstration_output","return:sweep_results_per_entry_point","return:document_corrections_summary","return:check_command_output"],"idempotency_key":"workflow:c52cefc0c0e62e8b574e9267:U3","unit_id":"U3"}]}

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

// ---- U1: Prove the fourth blocking bridge redacts, over a real socket ----
const U1 = await agent(
  "U1: Add the missing live-socket redaction test for the fourth blocking bridge, terminal.read.respond / text, and demonstrate it fails when the deny-set entry is removed. Read docs/plans/2026-08-05-credential-and-bridge-drift-remediation-plan.md (unit U1) as your authoritative spec.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys new_test_name, red_demonstration_output, redaction_reason_asserted, check_command_output -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Prove the fourth blocking bridge redacts, over a real socket", model: "sonnet", effort: "medium", schema: {"additionalProperties": true, "properties": {"check_command_output": {}, "new_test_name": {}, "red_demonstration_output": {}, "redaction_reason_asserted": {}}, "required": ["new_test_name", "red_demonstration_output", "redaction_reason_asserted", "check_command_output"], "type": "object"} },
)
__gate(U1, { unitId: "U1", expectsOutput: true, returns: ["new_test_name", "red_demonstration_output", "redaction_reason_asserted", "check_command_output"], cordProposal: "sonnet/medium -> sonnet/high (+1 effort rung)" })

// ---- U2: talaria record resolves its credential through the chain and refuses one on the command line ----
// depends_on: U1 (barrier)
const U2 = await agent(
  "U2: Make talaria record resolve its credential through the same CredentialProvider chain the launcher uses, and refuse a URL carrying a credential in either its query string or its userinfo, exiting 2 with a rotate warning that never echoes the value. Rewrite README.md's record example in the same commit. Read docs/plans/2026-08-05-credential-and-bridge-drift-remediation-plan.md (unit U2) as your authoritative spec.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys refusal_message_text, credential_absent_from_message_evidence, dial_url_test_results, readme_diff_summary, check_command_output -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "talaria record resolves its credential through the chain and refuses one on the command line", model: "opus", effort: "high", schema: {"additionalProperties": true, "properties": {"check_command_output": {}, "credential_absent_from_message_evidence": {}, "dial_url_test_results": {}, "readme_diff_summary": {}, "refusal_message_text": {}}, "required": ["refusal_message_text", "credential_absent_from_message_evidence", "dial_url_test_results", "readme_diff_summary", "check_command_output"], "type": "object"} },
)
__gate(U2, { unitId: "U2", expectsOutput: true, returns: ["refusal_message_text", "credential_absent_from_message_evidence", "dial_url_test_results", "readme_diff_summary", "check_command_output"], cordProposal: "opus/high -> opus/xhigh (+1 effort rung)" })

// ---- U3: Measure the process surface at every credential-holding entry point, and correct the overclaims ----
// depends_on: U2 (barrier)
const U3 = await agent(
  "U3: Parametrize the process-surface probe over every entry point that can hold a credential (launcher, record, refresh-credential), add the subcommand classification guard, and correct the argv overclaims in QUEUED.md and DECISIONS.md so they record what is now measured. Read docs/plans/2026-08-05-credential-and-bridge-drift-remediation-plan.md (unit U3) as your authoritative spec.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys entry_points_probed, guard_test_name, guard_red_demonstration_output, sweep_results_per_entry_point, document_corrections_summary, check_command_output -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Measure the process surface at every credential-holding entry point, and correct the overclaims", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"check_command_output": {}, "document_corrections_summary": {}, "entry_points_probed": {}, "guard_red_demonstration_output": {}, "guard_test_name": {}, "sweep_results_per_entry_point": {}}, "required": ["entry_points_probed", "guard_test_name", "guard_red_demonstration_output", "sweep_results_per_entry_point", "document_corrections_summary", "check_command_output"], "type": "object"} },
)
__gate(U3, { unitId: "U3", expectsOutput: true, returns: ["entry_points_probed", "guard_test_name", "guard_red_demonstration_output", "sweep_results_per_entry_point", "document_corrections_summary", "check_command_output"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })

if (__pulledCords.length > 0) {
  throw __halt(`pull-cord (#364): ${__pulledCords.length} unit(s) self-reported out of depth -- ` +
    __pulledCords.map((c) => `${c.unit}: ${c.reason}` + (c.proposal ? ` (propose ${c.proposal})` : ' (no legal climb: top of ladder or session ceiling -- HALT)')).join('; ') +
    '. ONE batched escalation ask -- confirm climbs via /tier patch and re-emit.')
}

return {
  units: { "U1": U1, "U2": U2, "U3": U3 },
  advisory_corrections: __advisories,
}
