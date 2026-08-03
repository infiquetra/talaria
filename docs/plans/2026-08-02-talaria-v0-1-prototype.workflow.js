// ===========================================================================
// talaria-v0-1-prototype -- emitted Claude Code workflow harness.
// AUTO-EMITTED from a structured execution-spec by execution_spec.py.
// CONTROL FLOW ONLY -- every agent reads the plan as its authoritative spec.
// Per-unit {model, effort} tiers (R2(b)); R3 pilot/fan-out same-tier +
// R10 enumerated-target reconciliation enforced at emit time.
// ===========================================================================

export const meta = {
  name: "talaria-v0-1-prototype",
  description: "Execute the Talaria v0.1 prototype plan: replay-first milestone (recorder, domain core, Textual gate, status runner) then live-gateway milestone (transport, bridges, slash, daily-driver closure).",
}
const settlement = {"casualty_threshold_percent":0,"dispatch_id":"workflow:f492fd7afadcc4bae9cfa833","driver":{"invocation_id":null,"units":[{"return_keys":[{"deliverable":"return:scaffold_commit_summary","result_key":"scaffold_commit_summary"},{"deliverable":"return:check_command_output","result_key":"check_command_output"},{"deliverable":"return:boundary_check_evidence","result_key":"boundary_check_evidence"},{"deliverable":"return:config_precedence_test_results","result_key":"config_precedence_test_results"}],"settlement_unit_id":"U1","workflow_unit_id":"U1"},{"return_keys":[{"deliverable":"return:equivalence_harness_result","result_key":"equivalence_harness_result"},{"deliverable":"return:ported_test_count","result_key":"ported_test_count"},{"deliverable":"return:outbound_redaction_evidence","result_key":"outbound_redaction_evidence"}],"settlement_unit_id":"U2","workflow_unit_id":"U2"},{"return_keys":[{"deliverable":"return:catalogue_doc_path","result_key":"catalogue_doc_path"},{"deliverable":"return:rule_to_test_map","result_key":"rule_to_test_map"},{"deliverable":"return:determinism_fixture_results","result_key":"determinism_fixture_results"},{"deliverable":"return:queued_p1_closure","result_key":"queued_p1_closure"}],"settlement_unit_id":"U3","workflow_unit_id":"U3"},{"return_keys":[{"deliverable":"return:assessment_doc_path","result_key":"assessment_doc_path"},{"deliverable":"return:per_criterion_verdicts","result_key":"per_criterion_verdicts"},{"deliverable":"return:queued_closure_diff","result_key":"queued_closure_diff"}],"settlement_unit_id":"U4","workflow_unit_id":"U4"},{"return_keys":[{"deliverable":"return:failure_matrix_test_results","result_key":"failure_matrix_test_results"},{"deliverable":"return:contract_doc_path","result_key":"contract_doc_path"},{"deliverable":"return:env_sanitization_evidence","result_key":"env_sanitization_evidence"}],"settlement_unit_id":"U6","workflow_unit_id":"U6"},{"return_keys":[{"deliverable":"return:gate_results_doc","result_key":"gate_results_doc"},{"deliverable":"return:gate_verdict","result_key":"gate_verdict"},{"deliverable":"return:pilot_suite_status","result_key":"pilot_suite_status"},{"deliverable":"return:adr_draft_path","result_key":"adr_draft_path"}],"settlement_unit_id":"U5","workflow_unit_id":"U5"},{"return_keys":[{"deliverable":"return:ae8_ae16_suite_results","result_key":"ae8_ae16_suite_results"},{"deliverable":"return:live_smoke_attach_evidence","result_key":"live_smoke_attach_evidence"},{"deliverable":"return:credential_hygiene_sweep_result","result_key":"credential_hygiene_sweep_result"}],"settlement_unit_id":"U7","workflow_unit_id":"U7"},{"return_keys":[{"deliverable":"return:bridge_round_trip_results","result_key":"bridge_round_trip_results"},{"deliverable":"return:ae3_live_corpus_sweep_result","result_key":"ae3_live_corpus_sweep_result"},{"deliverable":"return:expiry_late_answer_evidence","result_key":"expiry_late_answer_evidence"}],"settlement_unit_id":"U8","workflow_unit_id":"U8"},{"return_keys":[{"deliverable":"return:ae9_ae13_results","result_key":"ae9_ae13_results"},{"deliverable":"return:catalogue_inventory_summary","result_key":"catalogue_inventory_summary"},{"deliverable":"return:live_dispatch_evidence","result_key":"live_dispatch_evidence"}],"settlement_unit_id":"U9","workflow_unit_id":"U9"},{"return_keys":[{"deliverable":"return:verdict_doc_path","result_key":"verdict_doc_path"},{"deliverable":"return:daily_driver_verdict","result_key":"daily_driver_verdict"},{"deliverable":"return:ae10_ci_evidence","result_key":"ae10_ci_evidence"}],"settlement_unit_id":"U10","workflow_unit_id":"U10"}]},"max_attempts":3,"schema":"dispatch_settlement.v1","site":"workflow","units":[{"deliverables":["structured-result","return:scaffold_commit_summary","return:check_command_output","return:boundary_check_evidence","return:config_precedence_test_results"],"idempotency_key":"workflow:f492fd7afadcc4bae9cfa833:U1","unit_id":"U1"},{"deliverables":["structured-result","return:equivalence_harness_result","return:ported_test_count","return:outbound_redaction_evidence"],"idempotency_key":"workflow:f492fd7afadcc4bae9cfa833:U2","unit_id":"U2"},{"deliverables":["structured-result","return:catalogue_doc_path","return:rule_to_test_map","return:determinism_fixture_results","return:queued_p1_closure"],"idempotency_key":"workflow:f492fd7afadcc4bae9cfa833:U3","unit_id":"U3"},{"deliverables":["structured-result","return:assessment_doc_path","return:per_criterion_verdicts","return:queued_closure_diff"],"idempotency_key":"workflow:f492fd7afadcc4bae9cfa833:U4","unit_id":"U4"},{"deliverables":["structured-result","return:failure_matrix_test_results","return:contract_doc_path","return:env_sanitization_evidence"],"idempotency_key":"workflow:f492fd7afadcc4bae9cfa833:U6","unit_id":"U6"},{"deliverables":["structured-result","return:gate_results_doc","return:gate_verdict","return:pilot_suite_status","return:adr_draft_path"],"idempotency_key":"workflow:f492fd7afadcc4bae9cfa833:U5","unit_id":"U5"},{"deliverables":["structured-result","return:ae8_ae16_suite_results","return:live_smoke_attach_evidence","return:credential_hygiene_sweep_result"],"idempotency_key":"workflow:f492fd7afadcc4bae9cfa833:U7","unit_id":"U7"},{"deliverables":["structured-result","return:bridge_round_trip_results","return:ae3_live_corpus_sweep_result","return:expiry_late_answer_evidence"],"idempotency_key":"workflow:f492fd7afadcc4bae9cfa833:U8","unit_id":"U8"},{"deliverables":["structured-result","return:ae9_ae13_results","return:catalogue_inventory_summary","return:live_dispatch_evidence"],"idempotency_key":"workflow:f492fd7afadcc4bae9cfa833:U9","unit_id":"U9"},{"deliverables":["structured-result","return:verdict_doc_path","return:daily_driver_verdict","return:ae10_ci_evidence"],"idempotency_key":"workflow:f492fd7afadcc4bae9cfa833:U10","unit_id":"U10"}]}

const REPO = "infiquetra/talaria"

const __pulledCords = []

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
    throw new Error(
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
        throw new Error(
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
      throw new Error(
        `missing-output: Unit ${unitId} produced fewer items than expected. ` +
        `Expected ${targetCount}, produced ${producedCount}. Shortfall: ${shortfall}.`
      );
    }
  }

  if (opts.returns && opts.returns.length > 0) {
    const parsed = parseResult(result);
    if (parsed === null || parsed === undefined || typeof parsed !== 'object' || Array.isArray(parsed)) {
      throw new Error(
        `missing-output: Unit ${unitId} result is not a structured dictionary. ` +
        `Missing required keys: ${opts.returns.join(', ')}.`
      );
    }
    const missing = opts.returns.filter(
      k => !(k in parsed) || parsed[k] === null || parsed[k] === undefined
    );
    if (missing.length > 0) {
      throw new Error(
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

function __verifierPrompt(basePrompt, unitResult) {
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
  evidence to judge, return a refuted entry explaining the visibility gap; do not emit prose-only
  "nothing to verify" output.

UNIT RESULT INPUT (authoritative structured evidence):
${rendered}`;
}

// ---- U1: Python scaffold and quality gates ----
// ---- U4: Fallback presentation-layer assessment (PC8) ----
const [U1, U4] = await parallel([
  () =>
    __retry(() => agent(
      "U1: Create the Python scaffold and quality gates (pyproject, uv.lock, talaria package, the talaria/cli.py entry point with KTD7 flag parsing, talaria/config.py implementing KTD15's ~/.talaria precedence chain, boundary check, macOS-arm64-required CI matrix). Read docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md (unit U1) as your authoritative spec. Apply the plan's 'Unattended execution contract' section.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys scaffold_commit_summary, check_command_output, boundary_check_evidence, config_precedence_test_results -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
      { label: "Python scaffold and quality gates", model: "sonnet", effort: "medium", schema: {"additionalProperties": true, "properties": {"boundary_check_evidence": {}, "check_command_output": {}, "config_precedence_test_results": {}, "scaffold_commit_summary": {}}, "required": ["scaffold_commit_summary", "check_command_output", "boundary_check_evidence", "config_precedence_test_results"], "type": "object"} },
    ), { unitId: "U1", maxAttempts: 3 }),
  () =>
    __retry(() => agent(
      "U4: Assess prompt_toolkit against the gate criteria to plausibility depth and close the QUEUED P0. Read docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md (unit U4) as your authoritative spec. Apply the plan's 'Unattended execution contract' section.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys assessment_doc_path, per_criterion_verdicts, queued_closure_diff -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
      { label: "Fallback presentation-layer assessment (PC8)", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"assessment_doc_path": {}, "per_criterion_verdicts": {}, "queued_closure_diff": {}}, "required": ["assessment_doc_path", "per_criterion_verdicts", "queued_closure_diff"], "type": "object"} },
    ), { unitId: "U4", maxAttempts: 3 }),
])
__gate(U1, { unitId: "U1", expectsOutput: true, returns: ["scaffold_commit_summary", "check_command_output", "boundary_check_evidence", "config_precedence_test_results"], cordProposal: "sonnet/medium -> sonnet/high (+1 effort rung)" })
__gate(U4, { unitId: "U4", expectsOutput: true, returns: ["assessment_doc_path", "per_criterion_verdicts", "queued_closure_diff"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })

// ---- U2: Python recorder and redaction boundary ----
// depends_on: U1 (barrier)
const U2 = await agent(
  "U2: Re-encode the recorder and redaction boundary in Python with TS contract equivalence. Read docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md (unit U2) as your authoritative spec. Apply the plan's 'Unattended execution contract' section.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys equivalence_harness_result, ported_test_count, outbound_redaction_evidence -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Python recorder and redaction boundary", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"equivalence_harness_result": {}, "outbound_redaction_evidence": {}, "ported_test_count": {}}, "required": ["equivalence_harness_result", "ported_test_count", "outbound_redaction_evidence"], "type": "object"} },
)
__gate(U2, { unitId: "U2", expectsOutput: true, returns: ["equivalence_harness_result", "ported_test_count", "outbound_redaction_evidence"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })

// ---- U3: Domain core: normalize, state, projection, compat baseline ----
// depends_on: U2 (barrier)
const U3 = await agent(
  "U3: Complete the reconciliation catalogue at the pin, then implement the domain core (models, decode, normalize, state, projection, compat data). Read docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md (unit U3) as your authoritative spec. Apply the plan's 'Unattended execution contract' section.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys catalogue_doc_path, rule_to_test_map, determinism_fixture_results, queued_p1_closure -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Domain core: normalize, state, projection, compat baseline", model: "opus", effort: "high", schema: {"additionalProperties": true, "properties": {"catalogue_doc_path": {}, "determinism_fixture_results": {}, "queued_p1_closure": {}, "rule_to_test_map": {}}, "required": ["catalogue_doc_path", "rule_to_test_map", "determinism_fixture_results", "queued_p1_closure"], "type": "object"} },
)
__gate(U3, { unitId: "U3", expectsOutput: true, returns: ["catalogue_doc_path", "rule_to_test_map", "determinism_fixture_results", "queued_p1_closure"], cordProposal: "opus/high -> opus/xhigh (+1 effort rung)" })

// ---- U6: Status-line runner (frozen v1 contract) ----
// depends_on: U2, U3 (barrier)
const U6 = await agent(
  "U6: Implement the status-line runner against the frozen KTD5 contract with the full failure matrix. Read docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md (unit U6) as your authoritative spec. Apply the plan's 'Unattended execution contract' section.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys failure_matrix_test_results, contract_doc_path, env_sanitization_evidence -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Status-line runner (frozen v1 contract)", model: "sonnet", effort: "medium", schema: {"additionalProperties": true, "properties": {"contract_doc_path": {}, "env_sanitization_evidence": {}, "failure_matrix_test_results": {}}, "required": ["failure_matrix_test_results", "contract_doc_path", "env_sanitization_evidence"], "type": "object"} },
)
__gate(U6, { unitId: "U6", expectsOutput: true, returns: ["failure_matrix_test_results", "contract_doc_path", "env_sanitization_evidence"], cordProposal: "sonnet/medium -> sonnet/high (+1 effort rung)" })

// ---- U5: Replay-driven Textual shell - the framework validation gate ----
// depends_on: U2, U3, U4, U6 (barrier)
const U5 = await agent(
  "U5: Build the replay-driven Textual shell and run the framework validation gate against the pinned corpus and thresholds. Read docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md (unit U5) as your authoritative spec. Apply the plan's 'Unattended execution contract' section.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys gate_results_doc, gate_verdict, pilot_suite_status, adr_draft_path -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Replay-driven Textual shell - the framework validation gate", model: "opus", effort: "high", schema: {"additionalProperties": true, "properties": {"adr_draft_path": {}, "gate_results_doc": {}, "gate_verdict": {}, "pilot_suite_status": {}}, "required": ["gate_results_doc", "gate_verdict", "pilot_suite_status", "adr_draft_path"], "type": "object"} },
)
__gate(U5, { unitId: "U5", expectsOutput: true, returns: ["gate_results_doc", "gate_verdict", "pilot_suite_status", "adr_draft_path"], cordProposal: "opus/high -> opus/xhigh (+1 effort rung)" })

// ---- U7: Live transport: attach, RPC, reconnect, live recording ----
// depends_on: U2, U3, U5 (barrier)
const U7 = await agent(
  "U7: Implement the live transport (loopback ?token= query-parameter attach via a per-dial CredentialProvider, RPC correlation with connection-epoch qualified ids, reconnect reconciliation, LiveSource, outbound recording). Read docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md (unit U7) as your authoritative spec. Apply the plan's 'Unattended execution contract' section.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys ae8_ae16_suite_results, live_smoke_attach_evidence, credential_hygiene_sweep_result -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Live transport: attach, RPC, reconnect, live recording", model: "opus", effort: "high", schema: {"additionalProperties": true, "properties": {"ae8_ae16_suite_results": {}, "credential_hygiene_sweep_result": {}, "live_smoke_attach_evidence": {}}, "required": ["ae8_ae16_suite_results", "live_smoke_attach_evidence", "credential_hygiene_sweep_result"], "type": "object"} },
)
__gate(U7, { unitId: "U7", expectsOutput: true, returns: ["ae8_ae16_suite_results", "live_smoke_attach_evidence", "credential_hygiene_sweep_result"], cordProposal: "opus/high -> opus/xhigh (+1 effort rung)" })

// ---- U8: Blocking prompts: approval path and four bridges ----
// depends_on: U7 (barrier)
const U8 = await agent(
  "U8: Implement the approval path and four blocking bridges in place, with terminal-read served from the projection. Read docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md (unit U8) as your authoritative spec. Apply the plan's 'Unattended execution contract' section.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys bridge_round_trip_results, ae3_live_corpus_sweep_result, expiry_late_answer_evidence -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Blocking prompts: approval path and four bridges", model: "opus", effort: "high", schema: {"additionalProperties": true, "properties": {"ae3_live_corpus_sweep_result": {}, "bridge_round_trip_results": {}, "expiry_late_answer_evidence": {}}, "required": ["bridge_round_trip_results", "ae3_live_corpus_sweep_result", "expiry_late_answer_evidence"], "type": "object"} },
)
__gate(U8, { unitId: "U8", expectsOutput: true, returns: ["bridge_round_trip_results", "ae3_live_corpus_sweep_result", "expiry_late_answer_evidence"], cordProposal: "opus/high -> opus/xhigh (+1 effort rung)" })

// ---- U9: Slash commands and paste collapse ----
// depends_on: U8 (barrier)
const U9 = await agent(
  "U9: Implement gateway slash dispatch (six shapes, local-entry degradation, Talaria-local set) and paste collapse. Read docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md (unit U9) as your authoritative spec. Apply the plan's 'Unattended execution contract' section.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys ae9_ae13_results, catalogue_inventory_summary, live_dispatch_evidence -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Slash commands and paste collapse", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"ae9_ae13_results": {}, "catalogue_inventory_summary": {}, "live_dispatch_evidence": {}}, "required": ["ae9_ae13_results", "catalogue_inventory_summary", "live_dispatch_evidence"], "type": "object"} },
)
__gate(U9, { unitId: "U9", expectsOutput: true, returns: ["ae9_ae13_results", "catalogue_inventory_summary", "live_dispatch_evidence"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })

// ---- U10: Daily-driver closure: compat verdict, install, teardown ----
// depends_on: U9 (barrier)
const U10 = await agent(
  "U10: Run the compatibility-baseline verification, clean-install job, teardown proofs, and write the daily-driver verdict. Read docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md (unit U10) as your authoritative spec. Apply the plan's 'Unattended execution contract' section.\n\nRETURN CONTRACT (all tiers): your FINAL message MUST be ONLY a single JSON object with the keys verdict_doc_path, daily_driver_verdict, ae10_ci_evidence -- no prose, no markdown code fences, no YAML, and nothing before or after the JSON. The workflow gate parses this final message as JSON and FAILS the unit if it is not a JSON object carrying these keys. Emit it as your last action even if the work is only partial (fill each key with what you have plus a short note).",
  { label: "Daily-driver closure: compat verdict, install, teardown", model: "sonnet", effort: "high", schema: {"additionalProperties": true, "properties": {"ae10_ci_evidence": {}, "daily_driver_verdict": {}, "verdict_doc_path": {}}, "required": ["verdict_doc_path", "daily_driver_verdict", "ae10_ci_evidence"], "type": "object"} },
)
__gate(U10, { unitId: "U10", expectsOutput: true, returns: ["verdict_doc_path", "daily_driver_verdict", "ae10_ci_evidence"], cordProposal: "sonnet/high -> sonnet/xhigh (+1 effort rung)" })

if (__pulledCords.length > 0) {
  throw new Error(`pull-cord (#364): ${__pulledCords.length} unit(s) self-reported out of depth -- ` +
    __pulledCords.map((c) => `${c.unit}: ${c.reason}` + (c.proposal ? ` (propose ${c.proposal})` : ' (no legal climb: top of ladder or session ceiling -- HALT)')).join('; ') +
    '. ONE batched escalation ask -- confirm climbs via /tier patch and re-emit.')
}
