//! Live Model Context Protocol checks against an already running, pinned Hermes gateway.
//! Run ignored tests serially. `TALARIA_LIVE_REQUIRE=1` makes every skip fail;
//! writes additionally require `TALARIA_PROBE_WRITES=1` and always target `fixture`.
//! Discovery never starts, updates or restarts Hermes. No test confirms a reload.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hermes_client::rest::{NativeHttpMinter, RestClient, RestOptions};
use hermes_client::{
    CallError, CallOptions, Client, DiscoverOptions, EnsurePolicy, Purpose, Scope,
};
use hermes_contract::schemas::{McpCatalogResult, McpServersListResult, McpServersStatusResult};
use serde_json::{Value, json};
use talaria_core::mcp::{TransportKind, transport_kind};

fn home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(hermes_client::default_home)
}

fn opted_in(variable: &str) -> bool {
    std::env::var(variable).ok().as_deref() == Some("1")
}

fn skip(reason: &str) {
    println!("LIVE_MCP_RESULT: SKIPPED ({reason})");
    assert!(
        !opted_in("TALARIA_LIVE_REQUIRE"),
        "live check required: {reason}"
    );
}

struct Live {
    client: Client,
    rest: RestClient,
}

// Errors are fixed public descriptions: discovery errors can contain private paths.
async fn connect_fixture() -> Result<Live, &'static str> {
    let home = home();
    let endpoint = hermes_client::discover_with(
        &home,
        &DiscoverOptions {
            ensure: EnsurePolicy::Never,
            control_timeout: Duration::from_secs(5),
        },
    )
    .await
    .map_err(|_| "running gateway unavailable")?;
    if hermes_contract::pinned::live_skip_reason(endpoint.code_sha()).is_some() {
        return Err("gateway revision does not match the pinned Hermes release");
    }
    let endpoint = Arc::new(endpoint);
    let profile = endpoint
        .resolve_profile("fixture")
        .map_err(|_| "fixture unavailable")?;
    let ticket = hermes_client::mint_ticket(
        &home,
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .map_err(|_| "fixture ticket unavailable")?;
    let rest = RestClient::new(
        endpoint.api_origin(),
        Arc::new(NativeHttpMinter::new(home, endpoint.clone(), profile)),
        RestOptions::default(),
    )
    .map_err(|_| "loopback web interface unavailable")?;
    let mut connection = hermes_client::connect(&endpoint, ticket)
        .await
        .map_err(|_| "fixture connection unavailable")?;
    hermes_client::handshake(&mut connection)
        .await
        .map_err(|_| "fixture handshake unavailable")?;
    let (client, _inbound) = Client::start(connection.into_parts());
    Ok(Live { client, rest })
}

async fn begin() -> Option<Live> {
    match connect_fixture().await {
        Ok(live) => Some(live),
        Err(reason) => {
            skip(reason);
            None
        }
    }
}

async fn call(client: &Client, method: &str, params: Value) -> Value {
    match client
        .call_raw(
            method,
            params,
            CallOptions {
                timeout: Some(Duration::from_secs(60)),
            },
        )
        .await
    {
        Ok(value) => value,
        Err(CallError::Rpc(error)) if error.code == -32601 => {
            panic!("stop: {method} is absent on the interactive fixture connection")
        }
        Err(_) => panic!("{method} did not answer successfully"),
    }
}

// Each path contains object field names and an array marker, never scalar values.
fn field_paths(value: &Value, path: &str, fields: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                let next = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                fields.insert(next.clone());
                field_paths(value, &next, fields);
            }
        }
        Value::Array(array) => {
            for value in array {
                field_paths(value, &format!("{path}[]"), fields);
            }
        }
        _ => {}
    }
}

fn record_fields(directory: &std::path::Path, method: &str, reply: &Value) {
    let mut fields = BTreeSet::new();
    field_paths(reply, "", &mut fields);
    let bytes = serde_json::to_vec_pretty(&fields).expect("field names encode");
    let filename = format!("{method}.fields.json");
    assert!(
        std::fs::write(directory.join(&filename), bytes).is_ok(),
        "field names persist"
    );
    println!("LIVE_MCP_FIELDS: {filename}");
}

async fn list(client: &Client) -> McpServersListResult {
    serde_json::from_value(call(client, "mcp.servers.list", json!({})).await)
        .unwrap_or_else(|_| panic!("server list matches the pinned contract"))
}

fn names(list: &McpServersListResult) -> Vec<String> {
    let mut names: Vec<_> = list
        .servers
        .iter()
        .map(|server| server.name.clone())
        .collect();
    names.sort();
    names
}

#[tokio::test]
#[ignore = "requires the operator's running pinned gateway and fixture profile"]
async fn live_reads_on_fixture() {
    let Some(live) = begin().await else { return };
    // Keep the temporary directory for attachment; output names contain no host paths.
    let directory = tempfile::Builder::new()
        .prefix("talaria-mcp-field-names-")
        .tempdir()
        .unwrap_or_else(|_| panic!("field evidence directory available"))
        .keep();
    let catalogue = call(&live.client, "mcp.catalog", json!({})).await;
    let servers = call(&live.client, "mcp.servers.list", json!({})).await;
    let status = call(&live.client, "mcp.servers.status", json!({})).await;
    for (method, reply) in [
        ("mcp.catalog", &catalogue),
        ("mcp.servers.list", &servers),
        ("mcp.servers.status", &status),
    ] {
        record_fields(&directory, method, reply);
        assert!(
            reply.get("servers").is_some_and(Value::is_array),
            "{method} has a servers array"
        );
    }
    let catalogue: McpCatalogResult = serde_json::from_value(catalogue)
        .unwrap_or_else(|_| panic!("catalogue matches the pinned contract"));
    let _: McpServersListResult = serde_json::from_value(servers)
        .unwrap_or_else(|_| panic!("list matches the pinned contract"));
    let _: McpServersStatusResult = serde_json::from_value(status)
        .unwrap_or_else(|_| panic!("status matches the pinned contract"));
    let deepwiki = catalogue
        .servers
        .iter()
        .find(|entry| entry.name == "deepwiki")
        .unwrap_or_else(|| panic!("the curated catalogue includes deepwiki"));
    assert!(
        matches!(transport_kind(&deepwiki.transport), TransportKind::Remote),
        "deepwiki is remote"
    );
    live.client.close().await;
    println!("LIVE_MCP_RESULT: PASSED");
}

#[tokio::test]
#[ignore = "requires the operator's running pinned gateway and fixture profile"]
async fn live_reload_gate_has_no_side_effects() {
    let Some(live) = begin().await else { return };
    let before_list = call(&live.client, "mcp.servers.list", json!({})).await;
    let before_status = call(&live.client, "mcp.servers.status", json!({})).await;
    let before_revision = call(&live.client, "config.get", json!({"key": "mtime"})).await;
    let gate = call(&live.client, "reload.mcp", json!({})).await;
    assert!(
        gate == json!({"status": "confirm_required", "message": "Reloading MCP refreshes every chat of this profile."}),
        "reload requires confirmation"
    );
    // This is the only reload request with an unknown key. The pinned canonical
    // handler rejects it before reconciliation or changing the confirmation policy.
    let refusal = live
        .client
        .call_raw(
            "reload.mcp",
            json!({"always": true}),
            CallOptions {
                timeout: Some(Duration::from_secs(5)),
            },
        )
        .await;
    assert!(
        matches!(refusal, Err(CallError::Rpc(error)) if error.code == 4001 && error.reason.as_deref() == Some("invalid_params")),
        "always is refused as invalid parameters"
    );
    let after_list = call(&live.client, "mcp.servers.list", json!({})).await;
    let after_status = call(&live.client, "mcp.servers.status", json!({})).await;
    let after_revision = call(&live.client, "config.get", json!({"key": "mtime"})).await;
    assert!(
        before_list == after_list,
        "server configuration is unchanged"
    );
    assert!(
        before_status["servers"] == after_status["servers"],
        "cached server state is unchanged"
    );
    assert!(
        before_revision == after_revision,
        "configuration modification time and server revision are unchanged"
    );
    let params = hermes_contract::overrides::reload_mcp::Params {
        session_id: None,
        confirm: None,
    };
    assert!(
        serde_json::to_value(params).expect("reload params encode") == json!({}),
        "normal reload params omit absent fields"
    );
    live.client.close().await;
    println!("LIVE_MCP_RESULT: PASSED");
}

struct ProbeCleanup {
    name: Option<String>,
}

impl Drop for ProbeCleanup {
    fn drop(&mut self) {
        let Some(name) = self.name.take() else { return };
        // A fresh runtime and connection also work while the test unwinds. Waiting
        // for this thread ensures cleanup finishes before the test process exits.
        let cleaned = std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return false;
            };
            runtime.block_on(async {
                let Ok(live) = connect_fixture().await else {
                    return false;
                };
                let _removed = live
                    .client
                    .call_raw(
                        "mcp.servers.remove",
                        json!({"name": name}),
                        CallOptions {
                            timeout: Some(Duration::from_secs(5)),
                        },
                    )
                    .await;
                // A refused add, or a removed server with a lost reply, is clean
                // too. Verify absence instead of trusting either write's reply.
                let listed = live.client.call_raw(
                    "mcp.servers.list",
                    json!({}),
                    CallOptions { timeout: Some(Duration::from_secs(5)) },
                ).await;
                let clean = matches!(listed, Ok(reply) if reply["servers"].as_array().is_some_and(|servers| servers.iter().all(|server| server["name"].as_str().is_some_and(|listed_name| listed_name != name))));
                live.client.close().await;
                clean
            })
        })
        .join()
        .unwrap_or(false);
        if !cleaned {
            eprintln!("LIVE_MCP_CLEANUP: FAILED (operator must check fixture probe servers)");
            if !std::thread::panicking() {
                panic!("probe cleanup failed");
            }
        }
    }
}

fn probe_name() -> String {
    let mut bytes = [0_u8; 4];
    // Read the operating system random device; never derive a name from a host identifier.
    assert!(
        std::fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut bytes))
            .is_ok(),
        "random bytes available"
    );
    format!(
        "talaria-probe-{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3]
    )
}

#[tokio::test]
#[ignore = "requires the operator's pinned fixture gateway and explicit write opt-in"]
async fn live_add_test_toggle_remove_on_fixture() {
    if !opted_in("TALARIA_PROBE_WRITES") {
        skip("fixture writes require TALARIA_PROBE_WRITES=1");
        return;
    }
    let Some(live) = begin().await else { return };
    let before = names(&list(&live.client).await);
    let name = probe_name();
    assert!(!before.contains(&name), "probe name is unused");
    // Arm before add: a dropped reply may still have committed the write.
    let mut cleanup = ProbeCleanup {
        name: Some(name.clone()),
    };
    let added = call(
        &live.client,
        "mcp.servers.add",
        json!({"name": name, "config": {"url": "http://127.0.0.1:9/mcp"}}),
    )
    .await;
    assert!(added["ok"] == true && added["name"] == name, "probe added");
    let tested = call(&live.client, "mcp.servers.test", json!({"name": name})).await;
    assert!(tested["ok"] == false, "unreachable probe test fails");
    assert!(
        tested["error"]
            .as_str()
            .is_some_and(|error| !error.trim().is_empty()),
        "failed probe includes an error"
    );
    for enabled in [false, true] {
        let toggled = hermes_client::mcp_api::set_server_enabled(&live.rest, &name, enabled).await;
        let Ok(toggled) = toggled else {
            panic!("probe toggle succeeds")
        };
        assert!(
            toggled.ok && toggled.name == name && toggled.enabled == enabled,
            "toggle reply matches request"
        );
        let rows = list(&live.client).await;
        let server = rows
            .servers
            .iter()
            .find(|server| server.name == name)
            .unwrap_or_else(|| panic!("probe remains listed"));
        assert_eq!(
            server.enabled, enabled,
            "toggle reads back through the socket"
        );
    }
    let removed = call(&live.client, "mcp.servers.remove", json!({"name": name})).await;
    assert!(
        removed["ok"] == true && removed["removed"] == true,
        "probe removed"
    );
    cleanup.name = None;
    assert!(
        before == names(&list(&live.client).await),
        "sorted server names restored"
    );
    live.client.close().await;
    println!("LIVE_MCP_RESULT: PASSED");
}
