// In-memory protocol peer for the MCP application service. Shapes follow the pinned
// contract and the MCP unit specification's request/result examples.

#[derive(Default)]
struct McpSide {
    servers: std::collections::BTreeMap<String, Value>,
    flows: HashMap<String, String>,
    polls: VecDeque<Value>,
    errors: VecDeque<(String, Value)>,
    reload_count: usize,
    plugins: HashMap<String, String>,
    malformed: HashSet<String>,
    header_auth: HashSet<String>,
}

const MCP_FLOW: &str = "flow-example";
const MCP_AUTH_URL: &str = "https://example.invalid/authorize?redirect_uri=http%3A%2F%2F127.0.0.1%3A43123%2Fcallback&state=example";

impl MockGateway {
    pub fn drop_after(&self, method: &str) {
        lock(&self.shared).drop_after.insert(method.to_owned());
    }

    pub fn mcp_server(&self, server: Value) {
        let name = server["name"].as_str().expect("server name").to_owned();
        let mut state = lock(&self.shared);
        if let Some(plugin) = server["plugin"].as_str() {
            state.mcp.plugins.insert(name.clone(), plugin.to_owned());
        }
        state.mcp.servers.insert(name, server);
    }

    pub fn mcp_plugin(&self, name: &str, plugin: &str) {
        lock(&self.shared)
            .mcp
            .plugins
            .insert(name.to_owned(), plugin.to_owned());
    }

    pub fn mcp_malformed(&self, name: &str) {
        lock(&self.shared).mcp.malformed.insert(name.to_owned());
    }

    pub fn mcp_poll(&self, result: Value) {
        lock(&self.shared).mcp.polls.push_back(result);
    }

    pub fn mcp_error(&self, method: &str, error: Value) {
        lock(&self.shared)
            .mcp
            .errors
            .push_back((method.to_owned(), error));
    }

    pub fn mcp_reload_count(&self) -> usize {
        lock(&self.shared).mcp.reload_count
    }
}

fn mcp_summary(name: &str, config: &Value) -> Value {
    json!({
        "name": name,
        "transport": if config["url"].is_string() { "http" } else if config["command"].is_string() { "stdio" } else { "unknown" },
        "url": config.get("url"), "command": config.get("command"),
        "args": config.get("args").cloned().unwrap_or(json!([])),
        "env": [], "auth": config.get("auth"),
        "oauth_tokens_present": if config["auth"] == "oauth" { json!(false) } else { Value::Null },
        "enabled": true, "tools": null, "source": "config", "plugin": null,
    })
}

fn mcp_failure(code: i64, message: String) -> Value {
    json!({"code": code, "message": message})
}

fn mcp_writable(side: &McpSide, name: &str) -> Result<(), Value> {
    if let Some(plugin) = side.plugins.get(name) {
        return Err(mcp_failure(
            4090,
            format!("server '{name}' is provided by plugin '{plugin}' and cannot be modified"),
        ));
    }
    Ok(())
}

fn mcp_existing<'a>(side: &'a McpSide, name: &str) -> Result<&'a Value, Value> {
    if side.malformed.contains(name) {
        return Err(mcp_failure(4001, "malformed server config".to_owned()));
    }
    side.servers
        .get(name)
        .ok_or_else(|| mcp_failure(4064, format!("server '{name}' not found")))
}

// These three refusal shapes follow hermes_cli/mcp_security.py at the pinned
// revision: known indicators, shell egress and shell persistence. Commands are
// inspected as text; the peer never executes a configured command.
fn mcp_suspicious(config: &Value) -> bool {
    let command = config["command"].as_str().unwrap_or("");
    let script = config["args"]
        .as_array()
        .map(|args| {
            args.iter()
                .map(|arg| arg.as_str().unwrap_or(""))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let env = config["env"]
        .as_object()
        .map(|env| {
            env.values()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let flat = format!("{command} {script} {env}");
    if [
        "AAAAC3NzaC1lZDI1NTE5AAAAICBoh1oDC4DnsO1m5mJ4yfEKrQebaFh",
        "hermes-0day",
        "60.165.167.",
        "118.182.244.156",
        "61.178.123.196",
    ]
    .iter()
    .any(|ioc| flat.contains(ioc))
    {
        return true;
    }
    let words = shell_words::split(command)
        .unwrap_or_else(|_| command.split_whitespace().map(str::to_owned).collect());
    let basename = words
        .first()
        .and_then(|word| std::path::Path::new(word).file_name())
        .and_then(|word| word.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ![
        "bash",
        "sh",
        "zsh",
        "dash",
        "fish",
        "cmd",
        "cmd.exe",
        "powershell",
        "powershell.exe",
        "pwsh",
        "pwsh.exe",
    ]
    .contains(&basename.as_str())
    {
        return false;
    }
    let script = script.to_ascii_lowercase();
    let net_word = |byte: u8| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte);
    let word = |literal: &str| {
        script.match_indices(literal).any(|(start, _)| {
            let end = start + literal.len();
            (start == 0 || !net_word(script.as_bytes()[start - 1]))
                && (end == script.len() || !net_word(script.as_bytes()[end]))
        })
    };
    [
        "curl",
        "wget",
        "nc",
        "ncat",
        "socat",
        "invoke-webrequest",
        "invoke-restmethod",
        "system.net.webclient",
    ]
    .iter()
    .any(|name| word(name))
        || script.contains("/dev/tcp/")
        || [
            "authorized_keys",
            ".ssh/",
            "/etc/ssh",
            "/etc/pam.d",
            "/etc/sudoers",
            "/etc/cron",
            "crontab",
            "/etc/rc.local",
            "/etc/systemd",
            ".bashrc",
            ".bash_profile",
            ".profile",
            ".zshrc",
        ]
        .iter()
        .any(|surface| script.contains(surface))
        || (script.contains("pam_") && script.contains(".so"))
}

fn mcp_result(side: &mut McpSide, method: &str, params: &Value) -> Result<Value, Value> {
    let allowed: &[&str] = match method {
        "mcp.catalog" | "mcp.servers.list" | "mcp.servers.status" => &["profile"],
        "mcp.servers.add" => &["profile", "name", "preset", "config", "bearer_token"],
        "mcp.servers.set_api_key" => &["profile", "name", "value", "env_var"],
        "mcp.servers.test" | "mcp.servers.remove" => &["profile", "name"],
        "mcp.servers.oauth.start" => &["profile", "name", "client_redirect_uri"],
        "mcp.servers.oauth.poll" | "mcp.servers.oauth.cancel" => &["profile", "name", "session_id"],
        "mcp.servers.oauth.callback" => &[
            "profile",
            "name",
            "session_id",
            "code",
            "state",
            "error",
            "iss",
        ],
        "reload.mcp" => &["session_id", "confirm", "rev", "profile"],
        _ => unreachable!("only MCP methods reach this handler"),
    };
    if !params
        .as_object()
        .is_some_and(|object| object.keys().all(|key| allowed.contains(&key.as_str())))
    {
        return Err(if method == "reload.mcp" {
            json!({"code": 4001, "message": "invalid_params", "data": {"reason": "invalid_params"}})
        } else {
            mcp_failure(4001, "invalid params".to_owned())
        });
    }
    if method == "reload.mcp" {
        if params["confirm"] != true {
            return Ok(
                json!({"status": "confirm_required", "message": "Reloading MCP refreshes every chat of this profile."}),
            );
        }
        side.reload_count += 1;
        return Ok(
            json!({"status": "reloaded", "message": "MCP servers reconciled", "session": "applied", "session_servers": side.servers.keys().collect::<Vec<_>>()}),
        );
    }
    match method {
        "mcp.catalog" => {
            return Ok(json!({"servers": [{
                "name": "deepwiki", "description": "Public repository questions", "connector_slug": null,
                "installed": side.servers.contains_key("deepwiki"), "enabled": side.servers.get("deepwiki").is_some_and(|row| row["enabled"] == true),
                "requires": [], "transport": "TransportSpec(type='http', command=None, args=[], url='https://example.invalid/mcp', version=None, env={})",
            }]}));
        }
        "mcp.servers.list" => {
            return Ok(json!({"servers": side.servers.values().collect::<Vec<_>>()}));
        }
        "mcp.servers.status" => {
            return Ok(json!({"servers": side.servers.values().map(|row| json!({
            "name": row["name"], "transport": row["transport"], "tools": 3, "connected": true,
            "disabled": row["enabled"] == false, "status": "connected", "source": row["source"], "plugin": row["plugin"],
        })).collect::<Vec<_>>(), "checked_at": 1760000000000_i64}));
        }
        _ => {}
    }
    let name = params["name"].as_str().unwrap_or("").trim();
    if name.is_empty() {
        return Err(mcp_failure(4063, "name required".to_owned()));
    }
    if method == "mcp.servers.set_api_key" && params["value"].as_str().unwrap_or("").is_empty() {
        return Err(mcp_failure(4063, "value required".to_owned()));
    }
    if matches!(
        method,
        "mcp.servers.oauth.poll" | "mcp.servers.oauth.cancel" | "mcp.servers.oauth.callback"
    ) && params["session_id"]
        .as_str()
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Err(mcp_failure(4063, "session_id required".to_owned()));
    }
    if matches!(
        method,
        "mcp.servers.add"
            | "mcp.servers.set_api_key"
            | "mcp.servers.remove"
            | "mcp.servers.oauth.start"
    ) {
        mcp_writable(side, name)?;
    }
    match method {
        "mcp.servers.add" => {
            if side.servers.contains_key(name) || side.malformed.contains(name) {
                return Err(mcp_failure(4090, format!("server '{name}' already exists")));
            }
            let mut config = params.get("config").filter(|value| value.is_object()).cloned().unwrap_or(json!({}));
            if !config["url"].is_string() && !config["command"].is_string() {
                if params["preset"] == "deepwiki" {
                    config["url"] = json!("https://example.invalid/mcp");
                } else if let Some(preset) = params["preset"].as_str() {
                    return Err(mcp_failure(4063, format!("Unknown MCP catalog entry or preset: {preset}")));
                } else {
                    return Err(mcp_failure(4063, "config must specify a 'url' (http) or 'command' (stdio), or a valid 'preset'".to_owned()));
                }
            }
            if mcp_suspicious(&config) {
                return Err(mcp_failure(4001, format!("server '{name}' rejected: suspicious command/args configuration")));
            }
            if config.get("headers").is_some_and(|headers| !headers.is_null()) || params["bearer_token"].as_str().is_some_and(|token| !token.is_empty()) {
                side.header_auth.insert(name.to_owned());
            }
            let server = mcp_summary(name, &config);
            side.servers.insert(name.to_owned(), server.clone());
            Ok(json!({"ok": true, "name": name, "server": server}))
        }
        "mcp.servers.set_api_key" => {
            let value = params["value"].as_str().unwrap_or("");
            if value.is_empty() { return Err(mcp_failure(4063, "value required".to_owned())); }
            let row = mcp_existing(side, name)?;
            let normalized = value.trim().strip_prefix("Bearer ").unwrap_or(value.trim()).trim();
            if row["url"].is_string() && (normalized.is_empty() || normalized.eq_ignore_ascii_case("bearer")) {
                return Err(mcp_failure(4063, "value is not a valid credential".to_owned()));
            }
            let default = format!("MCP_{}_API_KEY", name.to_ascii_uppercase().chars().map(|ch| if ch.is_ascii_alphanumeric() || ch == '_' { ch } else { '_' }).collect::<String>().trim_matches('_'));
            let env = params["env_var"].as_str().unwrap_or(&default).to_owned();
            let mut server = row.clone();
            if server["url"].is_string() { side.header_auth.insert(name.to_owned()); }
            else { server["env"] = json!([env]); }
            side.servers.insert(name.to_owned(), server.clone());
            Ok(json!({"ok": true, "name": name, "env_var": env, "server": server}))
        }
        "mcp.servers.test" => {
            let row = mcp_existing(side, name)?;
            if row["auth"] == "oauth" && row["oauth_tokens_present"] == false {
                return Ok(json!({"ok": false, "error": "OAuth authentication required — no token found.", "tools": [], "oauth_needed": true, "oauth_tokens_present": false}));
            }
            Ok(json!({"ok": true, "tools": [{"name": "example", "description": "Example tool"}], "prompts": 0, "resources": 0, "oauth_needed": false, "oauth_tokens_present": null}))
        }
        "mcp.servers.remove" => {
            let removed = side.servers.remove(name).is_some() | side.malformed.remove(name);
            if !removed {
                return Err(mcp_failure(4064, format!("server '{name}' not found")));
            }
            side.header_auth.remove(name);
            Ok(json!({"ok": true, "removed": true}))
        }
        "mcp.servers.oauth.start" => {
            let row = mcp_existing(side, name)?;
            if !row["url"].is_string() { return Err(mcp_failure(4001, "stdio servers authenticate via env keys, not OAuth".to_owned())); }
            if row["auth"] != "oauth" && side.header_auth.contains(name) { return Err(mcp_failure(4001, "this server uses header/API-key auth, not OAuth".to_owned())); }
            side.flows.insert(MCP_FLOW.to_owned(), name.to_owned());
            Ok(json!({"ok": true, "session_id": MCP_FLOW, "auth_url": MCP_AUTH_URL, "flow": "pkce"}))
        }
        "mcp.servers.oauth.poll" => Ok(side.polls.pop_front().unwrap_or_else(|| {
            if side.flows.get(params["session_id"].as_str().unwrap_or("")) != Some(&name.to_owned()) {
                json!({"ok": true, "status": "error", "error_message": "OAuth session not found or expired"})
            } else { json!({"ok": true, "status": "pending", "session_id": MCP_FLOW, "error_message": null, "auth_url": MCP_AUTH_URL, "tools": null}) }
        })),
        "mcp.servers.oauth.cancel" => {
            side.flows.remove(params["session_id"].as_str().unwrap_or(""));
            Ok(json!({"ok": true, "status": "error"}))
        }
        "mcp.servers.oauth.callback" => Ok(if side.flows.get(params["session_id"].as_str().unwrap_or("")) == Some(&name.to_owned()) {
            json!({"ok": true, "session_id": MCP_FLOW})
        } else { json!({"ok": false, "error_message": "OAuth callback state mismatch"}) }),
        _ => unreachable!(),
    }
}

pub fn check_mcp_peer_refusals() {
    let mut side = McpSide::default();
    side.plugins
        .insert("plugin-only".into(), "example-plugin".into());
    side.servers.insert(
        "existing".into(),
        mcp_summary("existing", &json!({"url":"https://example.invalid/mcp"})),
    );
    let error = |side: &mut McpSide, method: &str, params: Value| {
        mcp_result(side, method, &params).expect_err("peer must refuse")
    };
    for method in [
        "mcp.servers.add",
        "mcp.servers.set_api_key",
        "mcp.servers.remove",
        "mcp.servers.oauth.start",
    ] {
        let mut params = json!({"name":" ", "value":"key"});
        if method != "mcp.servers.set_api_key" {
            params.as_object_mut().unwrap().remove("value");
        }
        assert_eq!(
            error(&mut side, method, params),
            mcp_failure(4063, "name required".into())
        );
        let mut params = json!({"name":"plugin-only"});
        if method == "mcp.servers.set_api_key" {
            params["value"] = json!("key");
        }
        assert_eq!(
            error(&mut side, method, params)["code"],
            4090,
            "plugin ownership precedes missing server or config"
        );
    }
    assert_eq!(
        error(
            &mut side,
            "mcp.servers.set_api_key",
            json!({"name":"plugin-only", "value":""})
        ),
        mcp_failure(4063, "value required".into())
    );
    assert_eq!(
        error(&mut side, "mcp.servers.add", json!({"name":"existing"})),
        mcp_failure(4090, "server 'existing' already exists".into())
    );
    for method in [
        "mcp.servers.test",
        "mcp.servers.remove",
        "mcp.servers.oauth.start",
    ] {
        assert_eq!(
            error(&mut side, method, json!({"name":"gone"})),
            mcp_failure(4064, "server 'gone' not found".into())
        );
    }
    for bad in [
        json!({"command":"bash", "args":["-c", "curl https://example.invalid"]}),
        json!({"command":"/bin/sh", "args":["-c", "printf x >> ~/.ssh/authorized_keys"]}),
        json!({"command":"python3", "args":["hermes-0day"]}),
    ] {
        assert_eq!(
            error(
                &mut side,
                "mcp.servers.add",
                json!({"name":"blocked", "config":bad})
            ),
            mcp_failure(
                4001,
                "server 'blocked' rejected: suspicious command/args configuration".into()
            )
        );
        assert!(!side.servers.contains_key("blocked"));
    }
    for safe in [
        json!({"command":"python3", "args":["-c", "print('curl')"]}),
        json!({"command":"npx", "args":["-y", "server"]}),
    ] {
        assert!(!mcp_suspicious(&safe), "custom commands stay allowed");
    }
    assert_eq!(side.reload_count, 0);
    assert_eq!(
        error(&mut side, "mcp.servers.list", json!({"extra":true}))["code"],
        4001
    );
    assert_eq!(
        error(&mut side, "reload.mcp", json!({"always":false}))["data"]["reason"],
        "invalid_params"
    );
    mcp_result(&mut side, "reload.mcp", &json!({})).unwrap();
    assert_eq!(side.reload_count, 0);
    mcp_result(&mut side, "reload.mcp", &json!({"confirm":true})).unwrap();
    assert_eq!(side.reload_count, 1);
}

fn plan_mcp(
    state: &mut State,
    method: &str,
    id: &Value,
    params: &Value,
    outgoing: &mut Vec<(Option<Duration>, String, Meta)>,
) -> bool {
    if !matches!(
        method,
        "mcp.catalog"
            | "mcp.servers.list"
            | "mcp.servers.status"
            | "mcp.servers.add"
            | "mcp.servers.set_api_key"
            | "mcp.servers.test"
            | "mcp.servers.remove"
            | "mcp.servers.oauth.start"
            | "mcp.servers.oauth.poll"
            | "mcp.servers.oauth.cancel"
            | "mcp.servers.oauth.callback"
            | "reload.mcp"
    ) {
        return false;
    }
    if state.silent_all || state.silent_methods.contains(method) {
        return true;
    }
    if state
        .mcp
        .errors
        .front()
        .is_some_and(|(wanted, _)| wanted == method)
    {
        let (_, error) = state.mcp.errors.pop_front().unwrap();
        outgoing.push((
            None,
            json!({"jsonrpc":"2.0", "id":id, "error":error}).to_string(),
            reply_meta(id, method),
        ));
        return true;
    }
    let frame = match mcp_result(&mut state.mcp, method, params) {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
    };
    outgoing.push((None, frame.to_string(), reply_meta(id, method)));
    true
}
