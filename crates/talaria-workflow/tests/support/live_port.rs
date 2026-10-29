use hermes_client::{
    CallError, CallOptions, Client, DiscoverOptions, Endpoint, EnsurePolicy, InboundStream,
    ProfileId, Purpose, Recorder, RecorderOptions, Scope, connect, default_home, discover_with,
    handshake, local_forbidden_literals, mint_ticket,
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

pub struct LiveEnv {
    home: PathBuf,
    endpoint: Endpoint,
    profile: ProfileId,
}
pub struct LivePort {
    pub client: Client,
    _inbound: InboundStream,
}
impl LivePort {
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, CallError> {
        self.client
            .call_raw(method, params, CallOptions::default())
            .await
    }
}

pub async fn preconditions() -> Option<LiveEnv> {
    let home = default_home();
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = match discover_with(&home, &options).await {
        Ok(e) => e,
        Err(_) => return skip("no usable gateway"),
    };
    let profile = match endpoint.resolve_profile("fixture") {
        Ok(p) => p,
        Err(_) => return skip("fixture profile unavailable"),
    };
    let pin = include_str!("../../../hermes-contract/PINNED")
        .lines()
        .find_map(|line| line.strip_prefix("commit="))
        .unwrap();
    if !endpoint.code_sha().is_some_and(|sha| pin.starts_with(sha)) {
        return skip("gateway revision differs from pinned release");
    }
    Some(LiveEnv {
        home,
        endpoint,
        profile,
    })
}
pub fn skip<T>(reason: &str) -> Option<T> {
    println!("JOURNAL_LIVE_RESULT: SKIPPED ({reason})");
    assert_ne!(
        std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref(),
        Some("1"),
        "live gateway required"
    );
    None
}
pub async fn attach(env: &LiveEnv, recording: Option<&Recorder>) -> LivePort {
    let ticket = mint_ticket(
        &env.home,
        &env.endpoint,
        &env.profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .unwrap();
    let mut connection = connect(&env.endpoint, ticket).await.unwrap();
    if let Some(recorder) = recording {
        connection.set_recorder(recorder.clone(), 1)
    }
    handshake(&mut connection).await.unwrap();
    let (client, inbound) = Client::start(connection.into_parts());
    LivePort {
        client,
        _inbound: inbound,
    }
}
pub async fn wait_terminal(port: &LivePort, session: &str, admission: &str) -> Value {
    for _ in 0..180 {
        let receipt=port.call("prompt.receipt",serde_json::json!({"profile":"fixture","session_id":session,"admission_id":admission,"include_result":true})).await.unwrap();
        if receipt["status"] == "terminal" {
            return receipt;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    panic!("turn did not settle")
}
pub async fn changed_gateway(env: &LiveEnv) -> Option<LiveEnv> {
    let original = env.endpoint.instance_id().to_owned();
    for _ in 0..300 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let opts = DiscoverOptions {
            control_timeout: Duration::from_secs(5),
            ensure: EnsurePolicy::Never,
        };
        if let Ok(endpoint) = discover_with(&env.home, &opts).await
            && endpoint.instance_id() != original
        {
            let profile = endpoint.resolve_profile("fixture").ok()?;
            return Some(LiveEnv {
                home: env.home.clone(),
                endpoint,
                profile,
            });
        }
    }
    None
}
#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);
impl Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
static PARTS: OnceLock<Mutex<BTreeMap<u8, Vec<Value>>>> = OnceLock::new();
pub struct Capture {
    index: u8,
    bytes: Buffer,
    pub recorder: Recorder,
}
fn options() -> RecorderOptions {
    RecorderOptions {
        endpoint: "ws://fixture.invalid/api/ws".into(),
        subprotocols: vec!["hermes-gateway-v1".into()],
        scenario: Some("workflow idempotency".into()),
        substitutions: local_forbidden_literals()
            .into_iter()
            .map(|s| (s, "<redacted>".into()))
            .collect(),
        ..RecorderOptions::default()
    }
}
impl Capture {
    pub fn new(index: u8) -> Option<Self> {
        if std::env::var("TALARIA_RECORD_FIXTURE").ok().as_deref() != Some("1") {
            return None;
        }
        let bytes = Buffer::default();
        let recorder = Recorder::from_writer(bytes.clone(), options()).unwrap();
        Some(Self {
            index,
            bytes,
            recorder,
        })
    }
    pub fn finish(self) {
        assert_eq!(self.recorder.finish().unwrap().dropped, 0);
        let bytes = self.bytes.0.lock().unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        let frames = text
            .lines()
            .skip(1)
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|v| v["kind"] == "frame")
            .collect::<Vec<_>>();
        let mut parts = PARTS
            .get_or_init(|| Mutex::new(BTreeMap::new()))
            .lock()
            .unwrap();
        parts.insert(self.index, frames);
        if parts.len() == 3 {
            let name = ["p", "5", "-", "02", "-idempotency.jsonl"].concat();
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("fixtures")
                .join("frames")
                .join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let _ = std::fs::remove_file(&path);
            let output = Recorder::create(&path, options()).unwrap();
            for frames in parts.values() {
                for row in frames {
                    let text = serde_json::to_string(&row["frame"]).unwrap();
                    let generation = row["generation"].as_u64().unwrap_or(1);
                    if row["dir"] == "in" {
                        output.record_inbound(generation, &text)
                    } else {
                        output.record_outbound(generation, &text)
                    }
                }
            }
            assert_eq!(output.finish().unwrap().dropped, 0);
        }
    }
}
