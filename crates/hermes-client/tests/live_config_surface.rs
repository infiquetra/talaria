//! Ignored live probe of the gateway's configuration calls.
//!
//! Each test sends only the rows it owns, prints one `ROW` and one `MATRIX_ROW`
//! line per row, and merges its rows into the temporary matrix under the Cargo
//! target's temporary folder. Replies are reduced to status codes, refusal
//! reasons, key names, counts and classes before anything is printed.
//!
//! A missing gateway, a missing `fixture` profile, or a pin mismatch prints
//! `LIVE_CONFIG_SURFACE_RESULT: SKIPPED (<reason>)` and returns.

mod config_probe;
mod support;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use config_probe::{
    Sent, authority_of, build_request, count_secret_leaves, http_reason, median_ms,
    mint_native_http, privacy_problem, probe_profile_name, safe_keys, send, write_probe_allowed,
};
use hermes_client::{
    CallError, CallOptions, Client, DiscoverOptions, Endpoint, EnsurePolicy, Inbound,
    InboundStream, Purpose, Scope, connect, discover_with, handshake, mint_ticket,
};
use serde_json::{Map, Value, json};
use support::{LiveEnv, live_preconditions, report_skip};

static PROBE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const PREFIX: &str = "LIVE_CONFIG_SURFACE_RESULT";
const CALL_TIMEOUT: Duration = Duration::from_secs(30);
const ORIGIN: &str = "http://localhost";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tk {
    Ip,
    Ih,
    N,
    No,
}

impl Tk {
    fn json(self) -> Value {
        match self {
            Tk::Ip => json!({"purpose": "interactive", "scope": "profile", "profile": "fixture"}),
            Tk::Ih => json!({"purpose": "interactive", "scope": "host", "profile": "fixture"}),
            Tk::N => json!({"purpose": "native-http", "scope": "profile", "profile": "fixture"}),
            Tk::No => Value::Null,
        }
    }
}

/// One matrix row the live tests own, with its static prediction.
struct Spec {
    id: &'static str,
    question: u8,
    call: &'static str,
    transport: &'static str,
    ticket: Tk,
    shape: &'static str,
    predicted: Option<(&'static str, Option<&'static str>)>,
    source: &'static str,
}

#[allow(clippy::too_many_arguments)]
const fn spec(
    id: &'static str,
    question: u8,
    call: &'static str,
    transport: &'static str,
    ticket: Tk,
    shape: &'static str,
    predicted: Option<(&'static str, Option<&'static str>)>,
    source: &'static str,
) -> Spec {
    Spec {
        id,
        question,
        call,
        transport,
        ticket,
        shape,
        predicted,
        source,
    }
}

const OK: Option<(&str, Option<&str>)> = Some(("ok", None));
const INVALID: Option<(&str, Option<&str>)> = Some(("rpc_error:4001", Some("invalid_params")));
const MISMATCH: Option<(&str, Option<&str>)> = Some(("rpc_error:4001", Some("profile_mismatch")));
const SCOPE: Option<(&str, Option<&str>)> = Some(("http:403", Some("profile_scope_mismatch")));
const UNAUTHORIZED: Option<(&str, Option<&str>)> = Some(("http:401", Some("Unauthorized")));

const CONFIG_GET: &str = "gateway/session_config.py:65-113";
const CONFIG_SET: &str = "gateway/session_tui_config.py:158-200";
const CONFIG_REST: &str = "hermes_cli/web_routers/config_env.py:80-91";
const TICKETS: &str = "hermes_cli/dashboard_auth/native_http.py:255-280";
const SCOPE_CHECK: &str = "hermes_cli/dashboard_auth/native_http.py:83-102";
const ROUTING: &str = "gateway/session_controls.py:147-168";

const SPECS: &[Spec] = &[
    spec(
        "C01",
        1,
        "config.get",
        "socket",
        Tk::Ip,
        "key=full",
        OK,
        CONFIG_GET,
    ),
    spec(
        "C02",
        1,
        "config.get",
        "socket",
        Tk::Ip,
        "key=mtime",
        OK,
        CONFIG_GET,
    ),
    spec(
        "C03",
        1,
        "config.get",
        "socket",
        Tk::Ip,
        "key=skin",
        OK,
        CONFIG_GET,
    ),
    spec(
        "C04",
        1,
        "config.get",
        "socket",
        Tk::Ip,
        "key=approvals.mode",
        OK,
        CONFIG_GET,
    ),
    spec(
        "C05",
        1,
        "config.get",
        "socket",
        Tk::Ip,
        "key=profile",
        OK,
        CONFIG_GET,
    ),
    spec(
        "C06",
        1,
        "config.get",
        "socket",
        Tk::Ip,
        "key=display.skin",
        INVALID,
        CONFIG_GET,
    ),
    spec(
        "C07",
        1,
        "config.get",
        "socket",
        Tk::Ip,
        "key=model",
        INVALID,
        CONFIG_GET,
    ),
    spec(
        "C08",
        2,
        "config.set",
        "socket",
        Tk::Ip,
        "key=display.skin,value",
        INVALID,
        CONFIG_SET,
    ),
    spec(
        "C09",
        2,
        "config.set",
        "socket",
        Tk::Ip,
        "key=model,value",
        Some(("rpc_error:4001", Some("use_session_mutation_model"))),
        "gateway/session_busy_controls.py:30-35",
    ),
    spec(
        "C10",
        2,
        "config.set",
        "socket",
        Tk::Ip,
        "key=skin,value",
        OK,
        CONFIG_SET,
    ),
    spec(
        "C11",
        3,
        "GET /api/config/schema",
        "rest",
        Tk::No,
        "",
        OK,
        "hermes_cli/web_routers/config_env.py:99-106",
    ),
    spec(
        "C12",
        3,
        "GET /api/config/schema",
        "rest",
        Tk::N,
        "",
        OK,
        "hermes_cli/web_routers/config_env.py:99-106",
    ),
    spec(
        "C13",
        3,
        "GET /api/config/defaults",
        "rest",
        Tk::No,
        "",
        OK,
        "hermes_cli/web_routers/config_env.py:94-96",
    ),
    spec(
        "C14",
        3,
        "GET /api/config/defaults",
        "rest",
        Tk::N,
        "",
        OK,
        "hermes_cli/web_routers/config_env.py:94-96",
    ),
    spec(
        "C15",
        4,
        "GET /api/config",
        "rest",
        Tk::N,
        "profile=fixture",
        OK,
        CONFIG_REST,
    ),
    spec(
        "C16",
        4,
        "GET /api/config",
        "rest",
        Tk::N,
        "profile=default",
        SCOPE,
        SCOPE_CHECK,
    ),
    spec(
        "C17",
        4,
        "GET /api/config",
        "rest",
        Tk::N,
        "include_defaults=false,profile=fixture",
        OK,
        CONFIG_REST,
    ),
    spec(
        "C18",
        4,
        "GET /api/config/raw",
        "rest",
        Tk::N,
        "profile=fixture",
        OK,
        "hermes_cli/web_routers/analytics.py:34-50",
    ),
    spec(
        "C19",
        5,
        "PUT /api/config",
        "rest",
        Tk::N,
        "config.display.show_cost,profile=fixture",
        OK,
        "hermes_cli/web_routers/config_env.py:117-156",
    ),
    spec(
        "C21",
        6,
        "GET /api/config",
        "rest",
        Tk::N,
        "profile=fixture",
        UNAUTHORIZED,
        TICKETS,
    ),
    spec(
        "C22",
        6,
        "GET /api/config",
        "rest",
        Tk::N,
        "profile=fixture",
        UNAUTHORIZED,
        TICKETS,
    ),
    spec(
        "C23",
        6,
        "GET /api/config",
        "rest",
        Tk::N,
        "profile=fixture",
        UNAUTHORIZED,
        TICKETS,
    ),
    spec(
        "C24",
        6,
        "GET /api/config",
        "rest",
        Tk::No,
        "profile=fixture",
        UNAUTHORIZED,
        "hermes_cli/web_server.py:456-476",
    ),
    spec(
        "C25",
        7,
        "model.options",
        "socket",
        Tk::Ip,
        "",
        OK,
        "gateway/session_config.py:300-330",
    ),
    spec(
        "C26",
        7,
        "model.options",
        "socket",
        Tk::Ip,
        "profile=fixture",
        None,
        "gateway/session_config.py:300-330",
    ),
    spec(
        "C27",
        8,
        "model.save_key",
        "socket",
        Tk::Ip,
        "api_key,slug",
        Some(("rpc_error:4002", None)),
        "tui_gateway/methods_complete.py:334-379",
    ),
    spec(
        "C28",
        8,
        "model.disconnect",
        "socket",
        Tk::Ip,
        "slug",
        Some(("rpc_error:4005", None)),
        "tui_gateway/methods_complete.py:382-408",
    ),
    spec(
        "C29",
        9,
        "GET /api/env",
        "rest",
        Tk::N,
        "profile=fixture",
        OK,
        "hermes_cli/web_routers/config_env.py:259-325",
    ),
    spec(
        "C30",
        9,
        "GET /api/providers/oauth",
        "rest",
        Tk::N,
        "",
        OK,
        "hermes_cli/web_routers/oauth.py:688-706",
    ),
    spec(
        "C31",
        9,
        "GET /api/providers/custom-endpoints",
        "rest",
        Tk::N,
        "",
        OK,
        "hermes_cli/web_routers/config_env.py:728-738",
    ),
    spec(
        "C34",
        10,
        "GET /api/model/info",
        "rest",
        Tk::No,
        "",
        OK,
        "hermes_cli/web_routers/models.py:90-135",
    ),
    spec(
        "C35",
        10,
        "GET /api/model/info",
        "rest",
        Tk::N,
        "",
        OK,
        "hermes_cli/web_routers/models.py:90-135",
    ),
    spec(
        "C36",
        10,
        "GET /api/model/options",
        "rest",
        Tk::N,
        "",
        OK,
        "hermes_cli/web_routers/models.py:138-168",
    ),
    spec(
        "C37",
        10,
        "GET /api/model/auxiliary",
        "rest",
        Tk::N,
        "",
        OK,
        "hermes_cli/web_routers/models.py:219-269",
    ),
    spec(
        "C38",
        10,
        "GET /api/model/moa",
        "rest",
        Tk::N,
        "",
        OK,
        "hermes_cli/web_routers/models.py:272-280",
    ),
    spec(
        "C41",
        12,
        "profiles.list",
        "socket",
        Tk::Ip,
        "",
        OK,
        "gateway/session_controls.py:302-303",
    ),
    spec(
        "C42",
        12,
        "GET /api/profiles",
        "rest",
        Tk::N,
        "",
        OK,
        "hermes_cli/web_routers/profiles.py:796-804",
    ),
    spec(
        "C43",
        13,
        "profiles.describe",
        "socket",
        Tk::Ip,
        "name=fixture",
        None,
        "tui_gateway/methods_profiles.py:349-383",
    ),
    spec(
        "C44",
        13,
        "profiles.configure",
        "socket",
        Tk::Ip,
        "name=fixture",
        OK,
        "tui_gateway/methods_profiles.py:386-409",
    ),
    spec(
        "C47",
        14,
        "config.get",
        "socket",
        Tk::Ip,
        "key=mtime,profile=default",
        MISMATCH,
        ROUTING,
    ),
    spec(
        "C48",
        14,
        "config.get",
        "socket",
        Tk::Ih,
        "key=profile,profile=default",
        OK,
        ROUTING,
    ),
    spec(
        "C49",
        14,
        "config.get",
        "socket",
        Tk::Ih,
        "key=mtime,profile=<unserved>",
        MISMATCH,
        ROUTING,
    ),
    spec(
        "C50",
        14,
        "GET /api/config",
        "rest",
        Tk::N,
        "profile=<unserved>",
        SCOPE,
        SCOPE_CHECK,
    ),
    spec(
        "C51",
        15,
        "runtime.describe",
        "socket",
        Tk::Ip,
        "",
        OK,
        "gateway/session_controls.py:492-502",
    ),
    spec(
        "C52",
        15,
        "GET /api/status",
        "rest",
        Tk::No,
        "",
        OK,
        "hermes_cli/web_routers/status.py:482-579",
    ),
    spec(
        "C53",
        15,
        "GET /api/health",
        "rest",
        Tk::No,
        "",
        OK,
        "hermes_cli/web_routers/status.py:119-130",
    ),
    spec(
        "C54",
        15,
        "identify",
        "control",
        Tk::No,
        "",
        OK,
        "gateway/control_socket.py:171-218",
    ),
    spec(
        "C59",
        17,
        "talaria.probe.nonexistent",
        "socket",
        Tk::Ip,
        "",
        Some(("unknown_method", Some("unknown_method"))),
        "gateway/session_controls.py:302-310",
    ),
    spec(
        "C60",
        17,
        "GET /api/talaria-probe-nonexistent",
        "rest",
        Tk::N,
        "",
        Some(("http:404", Some("no_such_api_endpoint"))),
        "hermes_cli/web_server_dashboard.py:234-240",
    ),
    spec(
        "C61",
        17,
        "GET /api/talaria-probe-nonexistent",
        "rest",
        Tk::No,
        "",
        UNAUTHORIZED,
        "hermes_cli/web_server.py:456-476",
    ),
];

const TICKET_ROWS: &[&str] = &["C16", "C21", "C22", "C23", "C24"];
const WRITE_ROWS: &[&str] = &["C10", "C19"];
const CONTROL_ROWS: &[&str] = &["C59", "C60", "C61"];

fn spec_of(id: &str) -> &'static Spec {
    SPECS
        .iter()
        .find(|spec| spec.id == id)
        .unwrap_or_else(|| panic!("no spec for {id}"))
}

/// What one request produced, before a body is dropped.
struct Obs {
    outcome: String,
    reason: Option<String>,
    keys: Vec<String>,
    unsafe_key: bool,
    elapsed_ms: u64,
    mint_ms: Option<u64>,
    body: Value,
}

impl Obs {
    fn is_ok(&self) -> bool {
        self.outcome == "ok"
    }
}

enum RestTicket {
    No,
    Fresh,
    Given(String),
}

/// A recorded row before it becomes JSON.
struct Recorded {
    obs: Obs,
    notes: Vec<String>,
    side_effect: &'static str,
    live_effect: Option<&'static str>,
}

struct Probe {
    env: LiveEnv,
    authority: String,
    ip: Option<(Client, InboundStream)>,
    ih: Option<(Client, InboundStream)>,
    sent: usize,
    out: Vec<String>,
    rows: Vec<Value>,
    refuted: Vec<String>,
}

fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn fail(probe: Option<&Probe>, reason: &str) -> ! {
    if let Some(probe) = probe {
        println!("requests sent: {}", probe.sent);
    }
    println!("{PREFIX}: FAILED ({reason})");
    panic!("configuration surface probe failed: {reason}");
}

impl Probe {
    async fn start(env: LiveEnv) -> Probe {
        let authority = match authority_of(env.endpoint.api_origin()) {
            Ok(authority) => authority,
            Err(reason) => fail(None, &reason),
        };
        Probe {
            env,
            authority,
            ip: None,
            ih: None,
            sent: 0,
            out: Vec::new(),
            rows: Vec::new(),
            refuted: Vec::new(),
        }
    }

    fn say(&mut self, line: String) {
        println!("{line}");
        self.out.push(line);
    }

    async fn attach(&mut self, scope: Tk) -> Result<(), String> {
        let scope_value = match scope {
            Tk::Ip => Scope::profile(),
            Tk::Ih => Scope::host(Purpose::Interactive).map_err(|_| "host scope refused")?,
            _ => return Err("not a socket ticket".to_string()),
        };
        self.sent += 1;
        let ticket = mint_ticket(
            &self.env.home,
            &self.env.endpoint,
            &self.env.fixture,
            Purpose::Interactive,
            scope_value,
        )
        .await
        .map_err(|err| format!("ticket mint failed: {}", err.next_step()))?;
        self.sent += 1;
        let mut connection = connect(&self.env.endpoint, ticket)
            .await
            .map_err(|err| format!("connect failed: {}", err.next_step()))?;
        handshake(&mut connection)
            .await
            .map_err(|err| format!("handshake failed: {}", err.next_step()))?;
        let pair = Client::start(connection.into_parts());
        match scope {
            Tk::Ip => self.ip = Some(pair),
            _ => self.ih = Some(pair),
        }
        Ok(())
    }

    async fn socket(&mut self, tk: Tk, method: &str, params: Value) -> Result<Obs, String> {
        let present = match tk {
            Tk::Ip => self.ip.is_some(),
            _ => self.ih.is_some(),
        };
        if !present {
            self.attach(tk).await?;
        }
        let client = match tk {
            Tk::Ip => self.ip.as_ref().map(|(client, _)| client),
            _ => self.ih.as_ref().map(|(client, _)| client),
        }
        .ok_or("no client")?;
        self.sent += 1;
        let start = Instant::now();
        let result = client
            .call_raw(
                method,
                params,
                CallOptions {
                    timeout: Some(CALL_TIMEOUT),
                },
            )
            .await;
        let elapsed = elapsed_ms(start);
        let (outcome, reason, body) = match result {
            Ok(value) => ("ok".to_string(), None, value),
            Err(CallError::Rpc(err)) if err.code == -32601 => {
                ("unknown_method".to_string(), err.reason, Value::Null)
            }
            Err(CallError::Rpc(err)) => {
                (format!("rpc_error:{}", err.code), err.reason, Value::Null)
            }
            Err(CallError::TimedOut { .. }) => {
                return Ok(Obs {
                    outcome: "timeout".to_string(),
                    reason: None,
                    keys: Vec::new(),
                    unsafe_key: false,
                    elapsed_ms: 30_000,
                    mint_ms: None,
                    body: Value::Null,
                });
            }
            Err(CallError::Unknown { .. }) => ("connection_lost".to_string(), None, Value::Null),
            Err(other) => return Err(format!("call not recorded: {}", other.next_step())),
        };
        let (keys, unsafe_key) = safe_keys(&body);
        Ok(Obs {
            outcome,
            reason,
            keys,
            unsafe_key,
            elapsed_ms: elapsed,
            mint_ms: None,
            body,
        })
    }

    async fn mint(&mut self) -> Result<(String, u64), String> {
        self.sent += 1;
        let start = Instant::now();
        let ticket =
            mint_native_http(&self.env.home, &self.env.endpoint, &self.env.fixture).await?;
        Ok((ticket, elapsed_ms(start)))
    }

    async fn rest(
        &mut self,
        verb: &str,
        target: &str,
        ticket: RestTicket,
        origin: bool,
        body: Option<&Value>,
    ) -> Result<Obs, String> {
        let (ticket, mint_ms) = match ticket {
            RestTicket::No => (None, None),
            RestTicket::Fresh => {
                let (ticket, ms) = self.mint().await?;
                (Some(ticket), Some(ms))
            }
            RestTicket::Given(ticket) => (Some(ticket), None),
        };
        let payload = body.map(|value| value.to_string().into_bytes());
        let request = build_request(
            verb,
            target,
            &self.authority,
            ticket.as_deref(),
            origin.then_some(ORIGIN),
            payload.as_deref(),
        );
        drop(ticket);
        self.sent += 1;
        let start = Instant::now();
        let sent = send(&self.authority, &request).await;
        let elapsed = elapsed_ms(start);
        drop(request);
        let reply = match sent {
            Sent::Reply(reply) => reply,
            Sent::TimedOut => {
                return Ok(Obs {
                    outcome: "timeout".to_string(),
                    reason: None,
                    keys: Vec::new(),
                    unsafe_key: false,
                    elapsed_ms: 30_000,
                    mint_ms,
                    body: Value::Null,
                });
            }
            Sent::Failed(reason) => return Err(reason),
        };
        let parsed: Value = serde_json::from_slice(&reply.body).unwrap_or(Value::Null);
        let ok = (200..300).contains(&reply.status);
        let (keys, unsafe_key) = if ok {
            safe_keys(&parsed)
        } else {
            (Vec::new(), false)
        };
        Ok(Obs {
            outcome: if ok {
                "ok".to_string()
            } else {
                format!("http:{}", reply.status)
            },
            reason: if ok {
                None
            } else {
                http_reason(&parsed).map(str::to_string)
            },
            keys,
            unsafe_key,
            elapsed_ms: elapsed,
            mint_ms,
            body: if ok { parsed } else { Value::Null },
        })
    }

    /// Discovery with the never-start policy, for the state note after a 401.
    async fn gateway_state(&mut self) -> String {
        self.sent += 1;
        let options = DiscoverOptions {
            ensure: EnsurePolicy::Never,
            ..Default::default()
        };
        match discover_with(&self.env.home, &options).await {
            Ok(endpoint) => match endpoint.state() {
                hermes_client::GatewayState::Ready => "gateway-state: ready".to_string(),
                hermes_client::GatewayState::DrainingForRestart => {
                    "gateway-state: draining".to_string()
                }
            },
            Err(err) => format!("gateway-state: {}", discovery_kind(&err)),
        }
    }

    async fn once(&mut self, id: &str) -> Result<Obs, String> {
        let unserved = self.unserved();
        let spec = spec_of(id);
        match id {
            "C01" => {
                self.socket(spec.ticket, "config.get", json!({"key": "full"}))
                    .await
            }
            "C02" => {
                self.socket(spec.ticket, "config.get", json!({"key": "mtime"}))
                    .await
            }
            "C03" => {
                self.socket(spec.ticket, "config.get", json!({"key": "skin"}))
                    .await
            }
            "C04" => {
                self.socket(spec.ticket, "config.get", json!({"key": "approvals.mode"}))
                    .await
            }
            "C05" => {
                self.socket(spec.ticket, "config.get", json!({"key": "profile"}))
                    .await
            }
            "C06" => {
                self.socket(spec.ticket, "config.get", json!({"key": "display.skin"}))
                    .await
            }
            "C07" => {
                self.socket(spec.ticket, "config.get", json!({"key": "model"}))
                    .await
            }
            "C08" => {
                guard_set("display.skin")?;
                let params = json!({"key": "display.skin", "value": "default"});
                self.socket(spec.ticket, "config.set", params).await
            }
            "C09" => {
                guard_set("model")?;
                let params = json!({"key": "model", "value": "talaria-probe"});
                self.socket(spec.ticket, "config.set", params).await
            }
            "C11" | "C13" | "C34" | "C52" | "C53" | "C61" => {
                let target = route_of(spec.call);
                self.rest("GET", target, RestTicket::No, false, None).await
            }
            "C12" | "C14" | "C30" | "C31" | "C35" | "C36" | "C37" | "C38" | "C42" | "C60" => {
                let target = route_of(spec.call);
                self.rest("GET", target, RestTicket::Fresh, false, None)
                    .await
            }
            "C15" | "C22" => {
                let origin = id == "C22";
                let target = "/api/config?profile=fixture";
                self.rest("GET", target, RestTicket::Fresh, origin, None)
                    .await
            }
            "C16" => {
                let target = "/api/config?profile=default";
                self.rest("GET", target, RestTicket::Fresh, false, None)
                    .await
            }
            "C17" => {
                let target = "/api/config?include_defaults=false&profile=fixture";
                self.rest("GET", target, RestTicket::Fresh, false, None)
                    .await
            }
            "C18" => {
                let target = "/api/config/raw?profile=fixture";
                self.rest("GET", target, RestTicket::Fresh, false, None)
                    .await
            }
            "C21" => {
                let (ticket, mint_ms) = self.mint().await?;
                let target = "/api/config?profile=fixture";
                let first = self
                    .rest(
                        "GET",
                        target,
                        RestTicket::Given(ticket.clone()),
                        false,
                        None,
                    )
                    .await?;
                drop(first);
                let mut obs = self
                    .rest("GET", target, RestTicket::Given(ticket), false, None)
                    .await?;
                obs.mint_ms = Some(mint_ms);
                Ok(obs)
            }
            "C23" => {
                let (ticket, mint_ms) = self.mint().await?;
                tokio::time::sleep(Duration::from_secs(31)).await;
                let target = "/api/config?profile=fixture";
                let mut obs = self
                    .rest("GET", target, RestTicket::Given(ticket), false, None)
                    .await?;
                obs.mint_ms = Some(mint_ms);
                Ok(obs)
            }
            "C24" => {
                let target = "/api/config?profile=fixture";
                self.rest("GET", target, RestTicket::No, false, None).await
            }
            "C25" => self.socket(spec.ticket, "model.options", json!({})).await,
            "C26" => {
                let params = json!({"profile": "fixture"});
                self.socket(spec.ticket, "model.options", params).await
            }
            "C27" => {
                let params = json!({"slug": "talaria-probe-nonexistent", "api_key": "placeholder"});
                self.socket(spec.ticket, "model.save_key", params).await
            }
            "C28" => {
                let params = json!({"slug": "talaria-probe-nonexistent"});
                self.socket(spec.ticket, "model.disconnect", params).await
            }
            "C29" => {
                let target = "/api/env?profile=fixture";
                self.rest("GET", target, RestTicket::Fresh, false, None)
                    .await
            }
            "C41" => self.socket(spec.ticket, "profiles.list", json!({})).await,
            "C43" => {
                let params = json!({"name": "fixture"});
                self.socket(spec.ticket, "profiles.describe", params).await
            }
            "C44" => {
                let params = json!({"name": "fixture"});
                self.socket(spec.ticket, "profiles.configure", params).await
            }
            "C47" => {
                let params = json!({"key": "mtime", "profile": "default"});
                self.socket(spec.ticket, "config.get", params).await
            }
            "C48" => {
                let params = json!({"key": "profile", "profile": "default"});
                self.socket(spec.ticket, "config.get", params).await
            }
            "C49" => {
                let params = json!({"key": "mtime", "profile": unserved});
                self.socket(spec.ticket, "config.get", params).await
            }
            "C50" => {
                let target = format!("/api/config?profile={unserved}");
                self.rest("GET", &target, RestTicket::Fresh, false, None)
                    .await
            }
            "C51" => {
                self.socket(spec.ticket, "runtime.describe", json!({}))
                    .await
            }
            "C54" => Ok(self.identify().await),
            "C59" => {
                self.socket(spec.ticket, "talaria.probe.nonexistent", json!({}))
                    .await
            }
            other => Err(format!("{other} is not a read row")),
        }
    }

    async fn identify(&mut self) -> Obs {
        self.sent += 1;
        let options = DiscoverOptions {
            ensure: EnsurePolicy::Never,
            ..Default::default()
        };
        let start = Instant::now();
        let found = discover_with(&self.env.home, &options).await;
        let elapsed = elapsed_ms(start);
        match found {
            Ok(endpoint) => Obs {
                outcome: "ok".to_string(),
                reason: None,
                keys: endpoint_keys(&endpoint),
                unsafe_key: false,
                elapsed_ms: elapsed,
                mint_ms: None,
                body: Value::Null,
            },
            Err(err) => Obs {
                outcome: "connection_lost".to_string(),
                reason: None,
                keys: Vec::new(),
                unsafe_key: false,
                elapsed_ms: elapsed,
                mint_ms: None,
                body: Value::Null,
            }
            .noted(discovery_kind(&err)),
        }
    }

    fn unserved(&self) -> String {
        static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let endpoint = &self.env.endpoint;
        NAME.get_or_init(|| {
            let mut salt = 0;
            loop {
                let name = probe_profile_name(salt);
                let served = endpoint.profiles().iter().any(|p| p.name == name);
                let parked = endpoint.parked().iter().any(|(parked, _)| *parked == name);
                if !served && !parked {
                    return name;
                }
                salt += 1;
            }
        })
        .clone()
    }

    /// Runs a read row: a discarded warm-up and three timed runs, or one run.
    async fn read_row(&mut self, id: &str) -> Result<(), String> {
        let single = TICKET_ROWS.contains(&id) || CONTROL_ROWS.contains(&id);
        let mut warm_up = None;
        if !single {
            warm_up = Some(self.once(id).await?.outcome);
        }
        let runs = if single { 1 } else { 3 };
        let mut timed = Vec::new();
        for _ in 0..runs {
            timed.push(self.once(id).await?);
        }
        let latency = median_ms(&timed.iter().map(|o| o.elapsed_ms).collect::<Vec<_>>());
        let mints: Vec<u64> = timed.iter().filter_map(|o| o.mint_ms).collect();
        let mut notes = Vec::new();
        let outcomes: Vec<String> = timed.iter().map(|o| o.outcome.clone()).collect();
        let differ = outcomes.iter().any(|o| *o != outcomes[0]);
        let pick = if differ {
            timed.iter().position(|o| !o.is_ok()).unwrap_or(0)
        } else {
            timed.len() - 1
        };
        let mut obs = timed.swap_remove(pick);
        obs.elapsed_ms = latency;
        if id == "C29" {
            // Keyed by environment variable names, which an operator can choose.
            obs.keys.clear();
            obs.unsafe_key = false;
        }
        notes.extend(self.row_notes(id, &obs));
        if !mints.is_empty() {
            notes.push(format!("mint_ms: {}", median_ms(&mints)));
        }
        if obs.outcome == "http:401" {
            notes.push(self.gateway_state().await);
        }
        if differ {
            let mut seen: Vec<(String, usize)> = Vec::new();
            for outcome in &outcomes {
                match seen.iter_mut().find(|(o, _)| o == outcome) {
                    Some((_, count)) => *count += 1,
                    None => seen.push((outcome.clone(), 1)),
                }
            }
            let list: Vec<String> = seen.iter().map(|(o, n)| format!("{o} {n}")).collect();
            notes.push(format!("outcomes-differ: {}", list.join(", ")));
        }
        if let Some(warm) = warm_up
            && warm != obs.outcome
        {
            notes.push(format!("warm-up: {warm}"));
        }
        self.record(
            id,
            Recorded {
                obs,
                notes,
                side_effect: "none",
                live_effect: None,
            },
        );
        Ok(())
    }

    /// Shape facts for the rows whose notes carry counts, classes or booleans.
    fn row_notes(&mut self, id: &str, obs: &Obs) -> Vec<String> {
        let body = &obs.body;
        let mut notes = Vec::new();
        if !obs.is_ok() {
            return notes;
        }
        match id {
            "C01" => {
                let (keys, _) = safe_keys(&body["config"]);
                notes.push(format!("config-keys: {}", keys.join(", ")));
            }
            "C11" | "C12" => {
                let line = schema_facts(id, body);
                self.say(line);
            }
            "C15" => notes.push(count_secret_leaves(body).note()),
            "C29" => {
                if let Value::Object(map) = body {
                    let fields = map
                        .values()
                        .next()
                        .map(|entry| safe_keys(entry).0.join(", "))
                        .unwrap_or_default();
                    notes.push(format!("entries: {}", map.len()));
                    notes.push(format!("entry-fields: {fields}"));
                }
            }
            "C41" | "C42" => notes.extend(self.profile_notes(body)),
            "C48" => {
                let default = self
                    .env
                    .endpoint
                    .profiles()
                    .iter()
                    .find(|p| p.name == "default")
                    .map(|p| p.id.as_ref().to_string());
                let home = body["home"].as_str().map(str::to_string);
                let matches = default.is_some() && home == default;
                notes.push(format!("home-matches-default: {matches}"));
            }
            _ => {}
        }
        notes
    }

    fn profile_notes(&self, body: &Value) -> Vec<String> {
        let names: BTreeSet<String> = profile_names(body);
        let served: BTreeSet<String> = self
            .env
            .endpoint
            .profiles()
            .iter()
            .map(|p| p.name.clone())
            .collect();
        let parked = self
            .env
            .endpoint
            .parked()
            .iter()
            .any(|(name, _)| names.contains(name));
        vec![
            format!("profiles: {}", names.len()),
            format!("same-as-served: {}", names == served),
            format!("includes-parked: {parked}"),
        ]
    }

    fn record(&mut self, id: &str, recorded: Recorded) {
        let spec = spec_of(id);
        let Recorded {
            obs,
            mut notes,
            side_effect,
            live_effect,
        } = recorded;
        let verdict = match spec.predicted {
            None => "as-predicted".to_string(),
            Some((outcome, reason)) => {
                let same = obs.outcome == outcome
                    && reason.is_none_or(|r| obs.reason.as_deref() == Some(r));
                if same {
                    "as-predicted".to_string()
                } else {
                    let expected = match reason {
                        Some(r) => format!("{outcome} {r}"),
                        None => outcome.to_string(),
                    };
                    notes.push(format!("refutes: expected {expected}"));
                    self.refuted.push(id.to_string());
                    format!("REFUTED (expected {expected})")
                }
            }
        };
        if obs.unsafe_key {
            notes.push("unsafe key name dropped".to_string());
        }
        let row = json!({
            "id": spec.id,
            "question": spec.question,
            "call": spec.call,
            "transport": spec.transport,
            "ticket": spec.ticket.json(),
            "request_shape": spec.shape,
            "outcome": obs.outcome,
            "reason": obs.reason,
            "result_keys": obs.keys,
            "latency_ms": obs.elapsed_ms,
            "side_effect": side_effect,
            "live_effect": live_effect,
            "evidence": "live",
            "source": spec.source,
            "notes": notes.join("; "),
        });
        let reason = obs.reason.clone().unwrap_or_else(|| "-".to_string());
        self.say(format!(
            "ROW {} {} {} {} {}",
            spec.id, obs.outcome, reason, obs.elapsed_ms, verdict
        ));
        self.say(format!("MATRIX_ROW {row}"));
        self.rows.push(row);
    }

    /// Merges this test's rows into the temporary matrix and checks the printed output.
    fn finish(&mut self, owned: &[&str]) {
        let missing: Vec<&str> = owned
            .iter()
            .copied()
            .filter(|id| !self.rows.iter().any(|row| row["id"] == *id))
            .collect();
        if !missing.is_empty() {
            fail(Some(self), "a row was not recorded");
        }
        if let Err(reason) = merge_matrix(&self.rows) {
            fail(Some(self), &reason);
        }
        let sent = format!("requests sent: {}", self.sent);
        self.say(sent);
        let text = self.out.join("\n");
        if let Some(problem) = privacy_problem(&text) {
            fail(
                Some(self),
                &format!("printed output failed the privacy scan at {problem}"),
            );
        }
        println!(
            "refuted rows: {}",
            if self.refuted.is_empty() {
                "none".to_string()
            } else {
                self.refuted.join(", ")
            }
        );
        println!("hermes release: {}", hermes_contract::pinned::TAG);
        println!("{PREFIX}: PASSED");
    }
}

impl Obs {
    fn noted(mut self, kind: &str) -> Obs {
        self.reason = Some(kind.to_string());
        self
    }
}

fn guard_set(key: &str) -> Result<(), String> {
    if config_probe::config_set_allowed(key, false) {
        Ok(())
    } else {
        Err(format!("config.set {key} needs the write opt-in"))
    }
}

fn route_of(call: &str) -> &str {
    call.split_once(' ').map_or(call, |(_, route)| route)
}

fn discovery_kind(err: &hermes_client::DiscoveryError) -> &'static str {
    use hermes_client::DiscoveryError as E;
    match err {
        E::NoGateway => "no-gateway",
        E::ControlTimeout => "control-timeout",
        E::InvalidIdentity => "invalid-identity",
        _ => "discovery-error",
    }
}

/// The `Endpoint` accessor names whose value is present.
fn endpoint_keys(endpoint: &Endpoint) -> Vec<String> {
    let mut keys = vec![
        "api_origin",
        "authority_epoch",
        "instance_id",
        "runtime_protocol",
        "state",
    ];
    if !endpoint.capabilities().is_empty() {
        keys.push("capabilities");
    }
    if endpoint.code_sha().is_some() {
        keys.push("code_sha");
    }
    if endpoint.code_version().is_some() {
        keys.push("code_version");
    }
    if !endpoint.parked().is_empty() {
        keys.push("parked");
    }
    if !endpoint.profiles().is_empty() {
        keys.push("profiles");
    }
    keys.sort_unstable();
    keys.into_iter().map(str::to_string).collect()
}

/// Profile names from a `profiles.list` result or a `GET /api/profiles` body, in memory only.
fn profile_names(body: &Value) -> BTreeSet<String> {
    let list = body.get("profiles").unwrap_or(body);
    let mut names = BTreeSet::new();
    if let Value::Array(items) = list {
        for item in items {
            let name = item
                .as_str()
                .or_else(|| item.get("name").and_then(Value::as_str));
            if let Some(name) = name {
                names.insert(name.to_string());
            }
        }
    }
    names
}

/// One line of fixed Hermes names and counts about the schema reply.
fn schema_facts(id: &str, body: &Value) -> String {
    let fields = body["fields"].as_object();
    let count = fields.map_or(0, Map::len);
    let mut attributes = BTreeSet::new();
    let mut has_default = false;
    let mut has_restart = false;
    for entry in fields.into_iter().flat_map(|map| map.values()) {
        if let Value::Object(entry) = entry {
            for name in entry.keys() {
                has_default |= name == "default";
                has_restart |= name.contains("restart");
                if config_probe::key_name_is_safe(name) {
                    attributes.insert(name.clone());
                }
            }
        }
    }
    let categories: Vec<&str> = body["category_order"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|name| config_probe::key_name_is_safe(name))
        .collect();
    // Segments of 40 or more characters, which a privacy scan must allow by name.
    let long: BTreeSet<&str> = fields
        .into_iter()
        .flat_map(|map| map.keys())
        .flat_map(|name| name.split('.'))
        .filter(|segment| segment.len() >= 40 && is_snake(segment))
        .collect();
    let present = |key: &str| fields.is_some_and(|map| map.contains_key(key));
    let named: Vec<String> = [
        "stt.provider",
        "tts.provider",
        "memory.provider",
        "terminal.backend",
    ]
    .iter()
    .map(|key| format!("{key}={}", present(key)))
    .collect();
    let attributes: Vec<&str> = attributes.iter().map(String::as_str).collect();
    format!(
        "SCHEMA_FACTS {id} fields={count} long-segments={} categories={} attributes={} default={has_default} restart={has_restart} {}",
        long.iter().copied().collect::<Vec<_>>().join("|"),
        categories.join("|"),
        attributes.join("|"),
        named.join(" ")
    )
}

fn is_snake(text: &str) -> bool {
    text.contains('_')
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn manifest_path(parts: &[&str]) -> PathBuf {
    let mut path = Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
    for part in parts {
        path = path.join(part);
    }
    path
}

/// Seeds the temporary matrix from the static rows, replaces this test's rows
/// by id, sorts by id, and renames a sibling file over the original.
fn merge_matrix(rows: &[Value]) -> Result<(), String> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("config-surface");
    std::fs::create_dir_all(&dir).map_err(|_| "matrix folder could not be made")?;
    let file = dir.join("matrix.json");
    let mut document: Value = match std::fs::read(&file) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| "matrix did not parse")?,
        Err(_) => {
            let seed = manifest_path(&[
                "..",
                "..",
                "fixtures",
                "configuration-surface",
                "static-rows.json",
            ]);
            let bytes = std::fs::read(seed).map_err(|_| "static rows could not be read")?;
            let seed: Value =
                serde_json::from_slice(&bytes).map_err(|_| "static rows did not parse")?;
            json!({"hermes_release": hermes_contract::pinned::TAG, "rows": seed["rows"].clone()})
        }
    };
    let list = document["rows"]
        .as_array_mut()
        .ok_or("matrix has no rows")?;
    for row in rows {
        list.retain(|existing| existing["id"] != row["id"]);
        list.push(row.clone());
    }
    list.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    let text = serde_json::to_string_pretty(&document).map_err(|_| "matrix did not encode")?;
    let temporary = dir.join("matrix.json.tmp");
    std::fs::write(&temporary, format!("{text}\n")).map_err(|_| "matrix could not be written")?;
    std::fs::rename(&temporary, &file).map_err(|_| "matrix could not be renamed")?;
    Ok(())
}

async fn prepare() -> Option<Probe> {
    match live_preconditions().await {
        Ok(env) => Some(Probe::start(env).await),
        Err(reason) => {
            report_skip(PREFIX, &reason);
            None
        }
    }
}

const READ_ROWS: &[&str] = &[
    "C01", "C02", "C03", "C04", "C05", "C06", "C07", "C08", "C09", "C11", "C12", "C13", "C14",
    "C15", "C17", "C18", "C25", "C26", "C27", "C28", "C29", "C30", "C31", "C34", "C35", "C36",
    "C37", "C38", "C41", "C42", "C43", "C44", "C47", "C48", "C49", "C50", "C51", "C52", "C53",
    "C54", "C59", "C60", "C61",
];

#[tokio::test]
#[ignore]
async fn probe_reads_on_fixture() {
    let _lock = PROBE_LOCK.lock().await;
    let Some(mut probe) = prepare().await else {
        return;
    };
    for id in READ_ROWS {
        if let Err(reason) = probe.read_row(id).await {
            fail(Some(&probe), &format!("{id}: {reason}"));
        }
    }
    let controls = [
        ("C59", "unknown_method"),
        ("C60", "http:404"),
        ("C61", "http:401"),
    ];
    for (id, want) in controls {
        let row = probe.rows.iter().find(|row| row["id"] == id);
        if row.is_none_or(|row| row["outcome"] != want) {
            fail(Some(&probe), &format!("control row {id} differs"));
        }
    }
    probe.finish(READ_ROWS);
}

#[tokio::test]
#[ignore]
async fn probe_ticket_rules() {
    let _lock = PROBE_LOCK.lock().await;
    let Some(mut probe) = prepare().await else {
        return;
    };
    for id in TICKET_ROWS {
        if let Err(reason) = probe.read_row(id).await {
            fail(Some(&probe), &format!("{id}: {reason}"));
        }
    }
    probe.finish(TICKET_ROWS);
}

#[tokio::test]
#[ignore]
async fn probe_writes_round_trip_on_fixture() {
    let opt_in = std::env::var("TALARIA_PROBE_WRITES").ok();
    if write_probe_allowed("fixture", opt_in.as_deref()).is_err() {
        println!("{PREFIX}: SKIPPED (write opt-in not set)");
        println!("requests sent: 0");
        return;
    }
    let _lock = PROBE_LOCK.lock().await;
    let Some(mut probe) = prepare().await else {
        return;
    };
    let explicit = "/api/config?include_defaults=false&profile=fixture";
    for id in WRITE_ROWS {
        let result = match *id {
            "C10" => write_skin(&mut probe, explicit).await,
            _ => write_show_cost(&mut probe, explicit).await,
        };
        if let Err(reason) = result {
            fail(Some(&probe), &format!("{id}: {reason}"));
        }
    }
    if probe
        .rows
        .iter()
        .any(|row| row["side_effect"] == "write-residue")
    {
        fail(Some(&probe), "restore left the file different");
    }
    probe.finish(WRITE_ROWS);
}

async fn explicit_document(probe: &mut Probe, target: &str) -> Result<Value, String> {
    let obs = probe
        .rest("GET", target, RestTicket::Fresh, false, None)
        .await?;
    if !obs.is_ok() {
        return Err(format!("explicit configuration read gave {}", obs.outcome));
    }
    Ok(obs.body)
}

fn not_probed(probe: &mut Probe, id: &str) {
    let spec = spec_of(id);
    let row = json!({
        "id": spec.id,
        "question": spec.question,
        "call": spec.call,
        "transport": spec.transport,
        "ticket": spec.ticket.json(),
        "request_shape": spec.shape,
        "outcome": "not-probed",
        "reason": "not sent: key not explicit in fixture configuration",
        "result_keys": Value::Null,
        "latency_ms": Value::Null,
        "side_effect": "not-probed",
        "live_effect": Value::Null,
        "evidence": "static",
        "source": spec.source,
        "notes": "not sent: key not explicit in fixture configuration",
    });
    probe.say(format!("ROW {id} not-probed - - key not explicit"));
    probe.say(format!("MATRIX_ROW {row}"));
    probe.rows.push(row);
}

async fn write_skin(probe: &mut Probe, explicit: &str) -> Result<(), String> {
    let before = explicit_document(probe, explicit).await?;
    let Some(original) = before["display"]["skin"].as_str().map(str::to_string) else {
        not_probed(probe, "C10");
        return Ok(());
    };
    // The probe reads the current value over the socket first, as a read row does.
    let current = probe
        .socket(Tk::Ip, "config.get", json!({"key": "skin"}))
        .await?;
    drop(current);
    let target = if original == "mono" {
        "default"
    } else {
        "mono"
    };
    if !config_probe::config_set_allowed("skin", true) {
        return Err("write guard refused skin".to_string());
    }
    drain(probe).await;
    let write = probe
        .socket(
            Tk::Ip,
            "config.set",
            json!({"key": "skin", "value": target}),
        )
        .await?;
    let broadcast = saw_skin_changed(probe).await;
    let read = probe
        .socket(Tk::Ip, "config.get", json!({"key": "skin"}))
        .await?;
    let live_effect = if read.body["value"].as_str() == Some(target) {
        "applies-now"
    } else {
        "unknown"
    };
    let restore = probe
        .socket(
            Tk::Ip,
            "config.set",
            json!({"key": "skin", "value": original}),
        )
        .await?;
    let after = explicit_document(probe, explicit).await?;
    let restored = restore.is_ok() && after == before;
    probe.say(format!(
        "SKIN_CHANGED_BROADCAST: {}",
        if broadcast { "seen" } else { "not seen" }
    ));
    let mut notes = Vec::new();
    if !restored {
        notes.push("restore left the file different".to_string());
    }
    probe.record(
        "C10",
        Recorded {
            obs: write,
            notes,
            side_effect: if restored {
                "write-restored"
            } else {
                "write-residue"
            },
            live_effect: Some(live_effect),
        },
    );
    Ok(())
}

async fn write_show_cost(probe: &mut Probe, explicit: &str) -> Result<(), String> {
    let before = explicit_document(probe, explicit).await?;
    let Some(original) = before["display"]["show_cost"].as_bool() else {
        not_probed(probe, "C19");
        return Ok(());
    };
    let body =
        |value: bool| json!({"config": {"display": {"show_cost": value}}, "profile": "fixture"});
    let write = probe
        .rest(
            "PUT",
            "/api/config",
            RestTicket::Fresh,
            false,
            Some(&body(!original)),
        )
        .await?;
    let read = explicit_document(probe, explicit).await?;
    let live_effect = if read["display"]["show_cost"].as_bool() == Some(!original) {
        "applies-now"
    } else {
        "unknown"
    };
    let restore = probe
        .rest(
            "PUT",
            "/api/config",
            RestTicket::Fresh,
            false,
            Some(&body(original)),
        )
        .await?;
    let after = explicit_document(probe, explicit).await?;
    let restored = restore.is_ok() && after == before;
    let mut notes = Vec::new();
    if let Some(mint) = write.mint_ms {
        notes.push(format!("mint_ms: {mint}"));
    }
    if !restored {
        notes.push("restore left the file different".to_string());
    }
    probe.record(
        "C19",
        Recorded {
            obs: write,
            notes,
            side_effect: if restored {
                "write-restored"
            } else {
                "write-residue"
            },
            live_effect: Some(live_effect),
        },
    );
    Ok(())
}

/// Discards events already queued on the interactive connection.
async fn drain(probe: &mut Probe) {
    if let Some((_, inbound)) = probe.ip.as_mut() {
        while let Ok(Some(_)) =
            tokio::time::timeout(Duration::from_millis(50), inbound.next()).await
        {}
    }
}

/// Whether a `skin.changed` event arrives within two seconds.
async fn saw_skin_changed(probe: &mut Probe) -> bool {
    let Some((_, inbound)) = probe.ip.as_mut() else {
        return false;
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match tokio::time::timeout(left, inbound.next()).await {
            Ok(Some(Inbound::Event(event))) if event.raw["type"] == "skin.changed" => return true,
            Ok(Some(_)) => {}
            _ => return false,
        }
    }
    false
}
