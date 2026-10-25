// Mock answers for the model catalogue, the effort key and fenced session changes. Issue 230.

use hermes_contract::schemas::ModelOptionsResult;

const EFFORT_WORDS: [&str; 9] = [
    "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra", "reset",
];

fn push_method_error(
    id: &Value,
    method: &str,
    reason: &str,
    state: &State,
    outgoing: &mut Vec<(Option<Duration>, String, Meta)>,
) {
    let text = if let Some(field) = reason.strip_prefix("field:") {
        refusal_with_data(
            id,
            "invalid_params",
            json!({"reason": "invalid_params", "field": field}),
        )
    } else if let Some(name) = reason.strip_prefix("fence:") {
        refusal_with_data(
            id,
            name,
            json!({
                "reason": name,
                "revision": state.sess.mutate_revision,
                "execution_generation": state.sess.mutate_generation,
            }),
        )
    } else if let Some(rest) = reason.strip_prefix("rpc:") {
        let (code, message) = rest.split_once(':').unwrap_or(("4001", rest));
        let code: i64 = code.parse().unwrap_or(REFUSAL_CODE);
        refusal_with_data(id, message, json!({"reason": message})).replace(
            &format!("\"code\":{REFUSAL_CODE}"),
            &format!("\"code\":{code}"),
        )
    } else {
        refusal_text(id, reason, None)
    };
    outgoing.push((None, text, reply_meta(id, method)));
}

fn refusal_with_data(id: &Value, reason: &str, data: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": REFUSAL_CODE, "message": reason, "data": data},
    })
    .to_string()
}

fn flag_is_bool(params: &Value, key: &str) -> bool {
    match params.get(key) {
        None => true,
        Some(value) => value.is_boolean(),
    }
}

fn handle_model_options(state: &State, params: &Value) -> Result<Value, String> {
    for key in ["explicit_only", "include_unconfigured", "refresh"] {
        if !flag_is_bool(params, key) {
            return Err("invalid_params".to_owned());
        }
    }
    if let Some(reason) = &state.sess.options_fail {
        if reason == "unknown_method" {
            return Err("rpc:-32601:unknown_method".to_owned());
        }
        return Err(reason.clone());
    }
    let chosen = if params
        .get("session_id")
        .filter(|value| !value.is_null())
        .is_some()
    {
        &state.sess.options_with_session
    } else {
        &state.sess.options_profile
    };
    serde_json::from_value::<ModelOptionsResult>(chosen.clone())
        .unwrap_or_else(|error| panic!("scripted model.options is not a catalogue: {error}"));
    Ok(chosen.clone())
}

fn handle_config_get(state: &mut State, params: &Value) -> Result<Value, String> {
    // Hermes: gateway/session_config.py L65-L113. A queued raw result answers verbatim
    // first. (#237)
    if let Some(raw) = state
        .sess
        .config_raw
        .get_mut("config.get")
        .and_then(VecDeque::pop_front)
    {
        return Ok(raw);
    }
    match params["key"].as_str().unwrap_or_default() {
        "reasoning" => Ok(json!({"value": state.sess.reasoning_value, "display": "show"})),
        // Hermes: gateway/session_config.py L65-L113. The profile read answers the
        // configured mode, or `permission_denied` while the switch is on. (#237)
        "approvals.mode" => {
            if state.sess.mode_read_denied {
                return Err("permission_denied".to_owned());
            }
            Ok(json!({"value": state.sess.approvals_mode}))
        }
        // Hermes: gateway/session_busy_controls.py L145-L153. The bypass read answers the
        // stored bypass. (#237)
        "yolo" => {
            let sid = params["session_id"].as_str().unwrap_or_default();
            let on = state.sess.session_bypass.get(sid).copied().unwrap_or(false);
            Ok(json!({
                "key": "yolo",
                "value": if on { "1" } else { "0" },
                "scope": "session",
            }))
        }
        _ => Err("invalid_params".to_owned()),
    }
}

fn handle_config_set(state: &mut State, params: &Value) -> Result<Value, String> {
    if let Some(raw) = state
        .sess
        .config_raw
        .get_mut("config.set")
        .and_then(VecDeque::pop_front)
    {
        return Ok(raw);
    }
    if params.get("confirm_expensive_model").is_some() {
        return Err("invalid_params".to_owned());
    }
    let key = params["key"].as_str().unwrap_or_default();
    if let Some(scope) = params.get("scope") {
        // Hermes: gateway/session_tui_config.py L312-L337 for the scope trap. Only a
        // global yolo write flips the profile mode; any other scoped write must name a
        // real scope. (#237)
        if key == "yolo" && scope.as_str() == Some("global") {
            state.sess.approvals_mode = if state.sess.approvals_mode == "off" {
                "manual"
            } else {
                "off"
            }
            .to_owned();
            state.sess.yolo_global_flip = true;
            return Ok(json!({"key": "yolo", "value": params["value"], "scope": "global"}));
        }
        match scope.as_str() {
            Some("session" | "global") => {}
            _ => return Err("invalid_params".to_owned()),
        }
    }
    match key {
        "reasoning" => {
            let value = params.get("value").cloned().unwrap_or(Value::Null);
            let word = value.as_str().unwrap_or("");
            if !EFFORT_WORDS.contains(&word) {
                return Err("invalid_params".to_owned());
            }
            if state.sess.config_set_mode == "refuse" {
                return Err("permission_denied".to_owned());
            }
            if state.sess.config_set_mode == "drop" {
                state.sess.suppress_reply = true;
            }
            let scope = params["scope"].as_str().unwrap_or("session");
            Ok(json!({"key": "reasoning", "value": value, "scope": scope}))
        }
        // Hermes: gateway/session_tui_config.py L300-L309 for the profile write;
        // gateway/session_busy_controls.py L128-L160 and tools/approval_yolo.py L31-L43
        // for the bypass write. (#237)
        "yolo" => {
            let sid = params["session_id"].as_str().unwrap_or_default().to_owned();
            let on = match params.get("value").and_then(Value::as_str) {
                None => {
                    let flipped = !state
                        .sess
                        .session_bypass
                        .get(&sid)
                        .copied()
                        .unwrap_or(false);
                    state.sess.yolo_toggle = true;
                    flipped
                }
                Some("1") => true,
                Some("0") => false,
                Some(_) => return Err("invalid_params".to_owned()),
            };
            state.sess.session_bypass.insert(sid, on);
            Ok(json!({
                "key": "yolo",
                "value": if on { "1" } else { "0" },
                "scope": "session",
            }))
        }
        "approvals.mode" => match params["value"].as_str() {
            Some(word @ ("manual" | "smart" | "off")) => {
                if state.sess.storage_unavailable {
                    return Err("storage_unavailable".to_owned());
                }
                state.sess.approvals_mode = word.to_owned();
                Ok(json!({"key": "approvals.mode", "value": word}))
            }
            _ => Err("invalid_params".to_owned()),
        },
        _ => Err("invalid_params".to_owned()),
    }
}

fn mutate_fingerprint(params: &Value) -> Value {
    json!({
        "operation": params.get("operation"),
        "payload": params.get("payload"),
        "expected_revision": params.get("expected_revision"),
        "expected_generation": params.get("expected_generation"),
    })
}

fn handle_session_mutate(state: &mut State, params: &Value) -> Result<Value, String> {
    let operation = params["operation"].as_str().unwrap_or("").to_owned();
    if matches!(operation.as_str(), "model" | "reasoning")
        && !params.get("expected_generation").is_some_and(Value::is_i64)
    {
        return Err("invalid_params".to_owned());
    }
    let request_id = params["request_id"].as_str().unwrap_or("").to_owned();
    let fingerprint = mutate_fingerprint(params);
    if let Some((stored_fingerprint, stored_result)) = state.sess.receipts.get(&request_id) {
        if stored_fingerprint == &fingerprint {
            return Ok(stored_result.clone());
        }
        return Err("admission_conflict".to_owned());
    }
    let expected_revision = params["expected_revision"].as_i64().unwrap_or(i64::MIN);
    let expected_generation = params["expected_generation"].as_i64();
    if expected_revision != state.sess.mutate_revision
        || expected_generation != Some(state.sess.mutate_generation)
    {
        let name = if expected_revision != state.sess.mutate_revision {
            "revision_conflict"
        } else {
            "stale_generation"
        };
        return Err(format!("fence:{name}"));
    }
    let payload = params.get("payload").cloned().unwrap_or(Value::Null);
    let mode = state.sess.mutate_mode.clone();
    if mode == "unresolvable" {
        return Err("model_resolution_failed".to_owned());
    }
    if mode == "busy" && operation == "reasoning" {
        return Err("session_busy".to_owned());
    }
    if mode == "busy" && operation == "model" {
        let result = json!({
            "session_id": params["session_id"],
            "operation": "model",
            "status": "deferred",
            "model": payload.get("model").cloned().unwrap_or(Value::Null),
            "revision": state.sess.mutate_revision,
            "execution_generation": state.sess.mutate_generation,
        });
        state
            .sess
            .receipts
            .insert(request_id, (fingerprint, result.clone()));
        return Ok(result);
    }
    let confirmed = payload.get("confirm").and_then(Value::as_str) == Some("tok-1");
    let needs_confirm = mode == "guarded-twice" || (mode == "guarded" && !confirmed);
    if needs_confirm {
        let result = json!({
            "session_id": params["session_id"],
            "operation": operation,
            "status": "confirmation_required",
            "confirm_required": true,
            "confirm": "tok-1",
            "target_model": payload.get("model").cloned().unwrap_or(Value::Null),
            "target_provider": payload.get("provider").cloned().unwrap_or(Value::Null),
            "confirm_message": "Confirm the switch.",
            "payload": payload,
        });
        state
            .sess
            .receipts
            .insert(request_id, (fingerprint, result.clone()));
        return Ok(result);
    }
    state.sess.mutate_revision += 1;
    state.sess.mutate_generation += 1;
    let mut result = json!({
        "session_id": params["session_id"],
        "revision": state.sess.mutate_revision,
        "execution_generation": state.sess.mutate_generation,
        "operation": operation,
    });
    if operation == "model" {
        if let Some(model) = payload.get("model") {
            result["model"] = model.clone();
        }
        if let Some(provider) = payload.get("provider") {
            result["provider"] = provider.clone();
        }
        if let Some(reasoning) = payload.get("reasoning") {
            result["effort"] = reasoning.clone();
        }
    }
    if operation == "reasoning"
        && let Some(effort) = payload.get("effort")
    {
        result["effort"] = effort.clone();
    }
    state
        .sess
        .receipts
        .insert(request_id, (fingerprint, result.clone()));
    if mode == "drop" {
        state.sess.suppress_reply = true;
    }
    Ok(result)
}
