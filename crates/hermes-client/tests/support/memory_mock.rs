#![allow(dead_code)]

use super::mock_rest::{MockRest, RecordedRequest};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

fn fixture(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/rest/memory")
        .join(name);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn readiness_error(state: &State, name: &str) -> Option<(u16, Value)> {
    let row = state.status["providers"]
        .as_array()?
        .iter()
        .find(|row| row["name"] == name);
    match row {
        None => Some((
            400,
            json!({"detail":format!("Unknown memory provider '{name}'.")}),
        )),
        Some(row) if row["status"] != "ready" => {
            let status = row["status"].as_str().unwrap_or("").replace('_', " ");
            Some((
                400,
                json!({"detail":format!("Memory provider '{name}' is not ready ({status}). Configure it in the dashboard first.")}),
            ))
        }
        _ => None,
    }
}

pub struct MemoryMock {
    pub rest: MockRest,
    state: Arc<Mutex<State>>,
}
struct State {
    status: Value,
    curator: Value,
    retaindb: Value,
    byterover: Value,
    declared: Value,
    memory_exists: bool,
    user_exists: bool,
    action: Value,
    scripted: Vec<Value>,
    next_pid: i64,
}
impl MemoryMock {
    pub async fn start() -> Self {
        let state = Arc::new(Mutex::new(State {
            status: fixture("status-builtin.json"),
            curator: fixture("curator.json"),
            retaindb: fixture("retaindb-plain.json"),
            byterover: fixture("byterover-plain.json"),
            declared: fixture("declared-sample.json"),
            memory_exists: true,
            user_exists: false,
            action: json!({"name":"curator-run","running":false,"exit_code":null,"pid":null,"lines":[]}),
            scripted: Vec::new(),
            next_pid: 4242,
        }));
        let mut rest = MockRest::start().await;
        for (method, path) in [
            ("GET", "/api/memory"),
            ("PUT", "/api/memory/provider"),
            ("POST", "/api/memory/reset*"),
            (
                "GET",
                "/api/memory/providers/retaindb/config?surface=declared",
            ),
            ("GET", "/api/memory/providers/retaindb/config"),
            (
                "PUT",
                "/api/memory/providers/retaindb/config?surface=declared",
            ),
            ("PUT", "/api/memory/providers/retaindb/config"),
            (
                "GET",
                "/api/memory/providers/byterover/config?surface=declared",
            ),
            ("GET", "/api/memory/providers/byterover/config"),
            (
                "PUT",
                "/api/memory/providers/byterover/config?surface=declared",
            ),
            ("PUT", "/api/memory/providers/byterover/config"),
            ("GET", "/api/curator"),
            ("PUT", "/api/curator/paused"),
            ("POST", "/api/curator/run*"),
            ("GET", "/api/actions/*"),
            ("GET", "/api/memory/providers/*"),
            ("PUT", "/api/memory/providers/*"),
        ] {
            let shared = state.clone();
            rest = rest.with_route(method, path, move |request| {
                respond(&mut shared.lock().unwrap(), request)
            });
        }
        Self { rest, state }
    }
    pub fn script_actions(&self, script: Vec<Value>) {
        self.state.lock().unwrap().scripted = script;
    }
    pub fn set_action(&self, action: Value) {
        self.state.lock().unwrap().action = action;
    }
    pub fn set_status(&self, status: Value) {
        self.state.lock().unwrap().status = status;
    }
    pub fn set_plain(&self, name: &str, config: Value) {
        let mut state = self.state.lock().unwrap();
        if name == "retaindb" {
            state.retaindb = config;
        } else if name == "byterover" {
            state.byterover = config;
        }
    }
    pub fn set_declared(&self, config: Value) {
        self.state.lock().unwrap().declared = config;
    }
    pub fn set_file(&self, name: &str, size: Option<u64>) {
        let mut state = self.state.lock().unwrap();
        match name {
            "memory" => state.memory_exists = size.is_some(),
            "user" => state.user_exists = size.is_some(),
            _ => return,
        }
        state.status["builtin_files"][name] = json!(size.unwrap_or(0));
    }
}
fn respond(s: &mut State, request: &RecordedRequest) -> (u16, Value) {
    let path = request.path.as_str();
    let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
    let selector = path.split_once('?').and_then(|(_, q)| {
        url::form_urlencoded::parse(q.as_bytes())
            .find(|(k, _)| k == "profile")
            .map(|(_, v)| v.into_owned())
    });
    if selector
        .as_deref()
        .is_some_and(|p| !p.is_empty() && p != "current" && p != "fixture")
    {
        return (403, json!({"detail":"profile_scope_mismatch"}));
    }
    match (request.method.as_str(), path) {
        ("GET", "/api/memory") => (200, s.status.clone()),
        ("PUT", "/api/memory/provider") => {
            let Some(provider) = body.get("provider").and_then(Value::as_str) else {
                return (422, json!({"detail":[]}));
            };
            let provider = provider.trim().to_lowercase();
            let provider =
                if ["default", "builtin", "built-in", "none"].contains(&provider.as_str()) {
                    String::new()
                } else {
                    provider
                };
            if !provider.is_empty()
                && let Some(error) = readiness_error(s, &provider)
            {
                return error;
            }
            s.status["active"] = json!(provider);
            (200, json!({"ok":true,"active":provider}))
        }
        ("POST", p) if p.starts_with("/api/memory/reset") => {
            let target = body
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_lowercase();
            let target = if target.is_empty() {
                "all"
            } else {
                target.as_str()
            };
            if !["memory", "user", "all"].contains(&target) {
                return (400, json!({"detail":"target must be all, memory, or user"}));
            }
            if selector.as_deref().is_none_or(|p| p.trim().is_empty()) {
                return (
                    400,
                    json!({"detail":"POST /api/memory/reset requires an explicit profile: this backend serves several profiles, so an unnamed target would act on the wrong profile's data."}),
                );
            }
            let mut deleted = Vec::new();
            for (name, filename) in [("memory", "MEMORY.md"), ("user", "USER.md")] {
                if (target == "all" || target == name)
                    && if name == "memory" {
                        s.memory_exists
                    } else {
                        s.user_exists
                    }
                {
                    deleted.push(filename);
                    s.status["builtin_files"][name] = json!(0);
                    if name == "memory" {
                        s.memory_exists = false;
                    } else {
                        s.user_exists = false;
                    }
                }
            }
            (200, json!({"ok":true,"deleted":deleted}))
        }
        ("GET", "/api/curator") => (200, s.curator.clone()),
        ("PUT", "/api/curator/paused") => {
            let Some(paused) = body.get("paused").and_then(Value::as_bool) else {
                return (422, json!({"detail":[]}));
            };
            s.curator["paused"] = json!(paused);
            (200, json!({"ok":true,"paused":paused}))
        }
        ("POST", p) if p.starts_with("/api/curator/run") => {
            if selector.as_deref().is_none_or(|p| p.trim().is_empty()) {
                return (
                    400,
                    json!({"detail":"POST /api/curator/run requires an explicit profile: this backend serves several profiles, so an unnamed target would act on the wrong profile's data."}),
                );
            }
            let pid = s.next_pid;
            s.next_pid += 1;
            s.action = json!({"name":"curator-run","running":true,"exit_code":null,"pid":pid,"lines":["=== curator-run started"]});
            (200, json!({"ok":true,"pid":pid,"name":"curator-run"}))
        }
        ("GET", p) if p.starts_with("/api/actions/") => {
            if !p.starts_with("/api/actions/curator-run/status") {
                return (404, json!({"detail":"Not Found"}));
            }
            if !s.scripted.is_empty() {
                s.action = s.scripted.remove(0);
            }
            let count = p
                .split_once('?')
                .and_then(|(_, q)| {
                    url::form_urlencoded::parse(q.as_bytes())
                        .find(|(k, _)| k == "lines")
                        .and_then(|(_, v)| v.parse::<usize>().ok())
                })
                .unwrap_or(200)
                .clamp(1, 2000);
            let mut action = s.action.clone();
            if let Some(lines) = action["lines"].as_array_mut() {
                let start = lines.len().saturating_sub(count);
                lines.drain(..start);
            }
            (200, action)
        }
        (method, p) if p.starts_with("/api/memory/providers/") => {
            let Some(name) = p
                .strip_prefix("/api/memory/providers/")
                .and_then(|p| p.split('/').next())
            else {
                return (404, json!({"detail":"Not Found"}));
            };
            if !["retaindb", "byterover", "sampleprov"].contains(&name) {
                return (404, json!({"detail":"Not Found"}));
            }
            let declared = p.contains("?surface=declared");
            if method == "GET" && declared {
                if name == "sampleprov" {
                    return (200, s.declared.clone());
                }
                return (
                    200,
                    json!({"name":name,"label":name,"docs_url":"","fields":[]}),
                );
            }
            if method == "GET" {
                if name == "sampleprov" {
                    return (
                        200,
                        json!({"name":name,"label":name,"fields":[],"setup":{}}),
                    );
                }
                return (
                    200,
                    if name == "retaindb" {
                        s.retaindb.clone()
                    } else {
                        s.byterover.clone()
                    },
                );
            }
            let Some(values) = body.get("values").and_then(Value::as_object) else {
                return (422, json!({"detail":[]}));
            };
            if declared {
                if name != "sampleprov" {
                    return (
                        404,
                        json!({"detail":format!("Unknown memory provider: {name}")}),
                    );
                }
                let mut fields = s.declared["fields"].as_array().unwrap().clone();
                for field in &mut fields {
                    let key = field["key"].as_str().unwrap().to_owned();
                    if let Some(value) = values.get(&key) {
                        let text = if value.is_null() {
                            String::new()
                        } else {
                            value
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| value.to_string())
                        };
                        let text = text.trim();
                        if field["kind"] == "secret" {
                            if !text.is_empty()
                                && field
                                    .get("env_key")
                                    .and_then(Value::as_str)
                                    .is_some_and(|v| !v.is_empty())
                            {
                                field["is_set"] = json!(true);
                            }
                        } else {
                            let kind = field["kind"].as_str().unwrap_or("text");
                            let default = field
                                .get("default")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned();
                            let selected = if text.is_empty() {
                                default.as_str()
                            } else {
                                text
                            };
                            if kind == "select"
                                && !field["options"].as_array().is_some_and(|opts| {
                                    opts.iter().any(|opt| opt["value"] == selected)
                                })
                            {
                                return (
                                    400,
                                    json!({"detail":format!("Invalid value for '{key}'")}),
                                );
                            }
                            if kind == "number" && !text.is_empty() && text.parse::<f64>().is_err()
                            {
                                return (
                                    400,
                                    json!({"detail":format!("Invalid number for '{key}'")}),
                                );
                            }
                            if kind == "json" && !text.is_empty() {
                                match serde_json::from_str::<Value>(text) {
                                    Err(_) => {
                                        return (
                                            400,
                                            json!({"detail":format!("Invalid JSON for '{key}'")}),
                                        );
                                    }
                                    Ok(v) if !v.is_object() && !v.is_array() => {
                                        return (
                                            400,
                                            json!({"detail":format!("'{key}' must be a JSON object or array")}),
                                        );
                                    }
                                    _ => {}
                                }
                            }
                            let output = if kind == "bool" {
                                if ["1", "true", "yes", "on"]
                                    .contains(&text.to_lowercase().as_str())
                                {
                                    "true"
                                } else {
                                    "false"
                                }
                            } else {
                                selected
                            };
                            field["value"] = json!(output);
                            field["is_set"] = json!(!output.is_empty());
                        }
                    }
                }
                s.declared["fields"] = json!(fields);
                s.status["active"] = json!(name);
                return (200, json!({"ok":true}));
            }
            if name == "sampleprov" {
                return (404, json!({"detail":"Not Found"}));
            }
            let config = if name == "retaindb" {
                &mut s.retaindb
            } else {
                &mut s.byterover
            };
            let mut current = config["fields"].as_array().unwrap().clone();
            for index in 0..current.len() {
                let field = &current[index];
                let visible = field
                    .get("when")
                    .and_then(Value::as_object)
                    .is_none_or(|when| {
                        when.iter().all(|(key, expected)| {
                            current.iter().find(|f| f["key"] == *key).is_some_and(|f| {
                                f["value"] == *expected
                                    || f["value"].as_str().is_some_and(|v| {
                                        v == expected.to_string().trim_matches('"')
                                    })
                            })
                        })
                    });
                if !visible {
                    continue;
                }
                let field = &mut current[index];
                let key = field["key"].as_str().unwrap().to_owned();
                if field["kind"] == "secret" {
                    if let Some(value) = values.get(&key)
                        && value.as_str().is_some_and(|v| !v.trim().is_empty())
                        && (key == "api_key"
                            || field
                                .get("env_key")
                                .and_then(Value::as_str)
                                .is_some_and(|v| !v.is_empty()))
                    {
                        field["is_set"] = json!(true);
                    }
                } else {
                    let value = values
                        .get(&key)
                        .cloned()
                        .unwrap_or_else(|| field["value"].clone());
                    let default = field.get("default").cloned().unwrap_or_else(|| {
                        match (name, key.as_str()) {
                            ("retaindb", "base_url") => json!("https://api.retaindb.com"),
                            ("byterover", "auto_extract") => json!("true"),
                            _ => json!(""),
                        }
                    });
                    let mut value = if value.as_str().is_some_and(|v| v.trim().is_empty()) {
                        default
                    } else if let Some(v) = value.as_str() {
                        json!(v.trim())
                    } else {
                        value
                    };
                    if field["kind"] == "select"
                        && !field["options"].as_array().is_some_and(|options| {
                            options.iter().any(|option| option["value"] == value)
                        })
                    {
                        return (400, json!({"detail":format!("Invalid value for '{key}'")}));
                    }
                    if field["kind"] == "boolean" {
                        let b = match &value {
                            Value::Bool(v) => Some(*v),
                            Value::Number(n) => n.as_f64().map(|n| n != 0.0),
                            Value::String(v) => match v.to_lowercase().as_str() {
                                "true" | "1" | "yes" | "on" => Some(true),
                                "false" | "0" | "no" | "off" => Some(false),
                                _ => None,
                            },
                            _ => None,
                        };
                        let Some(b) = b else {
                            return (
                                400,
                                json!({"detail":format!("Invalid boolean value: {value}")}),
                            );
                        };
                        value = json!(b);
                    }
                    if field["kind"] == "integer" || field["kind"] == "number" {
                        let number = value
                            .as_f64()
                            .or_else(|| value.as_str().and_then(|s| s.parse::<f64>().ok()));
                        if number.is_none_or(|n| {
                            n.is_nan()
                                || n.abs() > f64::MAX
                                || (field["kind"] == "integer" && n.fract() != 0.0)
                                || field["minimum"].as_f64().is_some_and(|min| n < min)
                                || field["maximum"].as_f64().is_some_and(|max| n > max)
                        }) {
                            return (
                                400,
                                json!({"detail":format!("Invalid numeric value for '{key}'")}),
                            );
                        }
                        value = json!(number.unwrap().to_string());
                    }
                    field["value"] = value;
                    field["is_set"] = json!(field["value"] != "");
                }
            }
            config["fields"] = json!(current);
            if name == "retaindb"
                && s.retaindb["fields"][0]["is_set"] == true
                && let Some(provider) = s.status["providers"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|p| p["name"] == "retaindb")
            {
                provider["configured"] = json!(true);
                provider["available"] = json!(true);
                provider["status"] = json!("ready");
            }
            if let Some(error) = readiness_error(s, name) {
                return error;
            }
            s.status["active"] = json!(name);
            (
                200,
                if declared {
                    json!({"ok":true})
                } else {
                    json!({"ok":true,"active":name})
                },
            )
        }
        _ => (404, json!({"detail":"Not Found"})),
    }
}
