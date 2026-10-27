#[path = "../../hermes-client/tests/support/mod.rs"]
mod live_support;

use hermes_client::rest::{Method, NativeHttpMinter, RequestOpts, RestClient, RestOptions};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};

fn client(env: live_support::LiveEnv) -> Arc<RestClient> {
    let origin = env.endpoint.api_origin().to_owned();
    let minter = NativeHttpMinter::new(env.home, Arc::new(env.endpoint), env.fixture);
    Arc::new(
        RestClient::new(&origin, Arc::new(minter), RestOptions::default()).expect("fixture client"),
    )
}

async fn get(rest: &RestClient, path: &str) -> Value {
    rest.request(Method::Get, path, None, RequestOpts::default())
        .await
        .expect("memory read")
}

async fn set_paused(rest: &RestClient, paused: bool) {
    let reply = rest
        .request(
            Method::Put,
            "/api/curator/paused",
            Some(&json!({"paused":paused})),
            RequestOpts::default(),
        )
        .await
        .expect("pause write");
    assert_eq!(reply["paused"], paused);
}

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/rest/memory")
        .join(name);
    serde_json::from_slice(&std::fs::read(path).expect("fixture exists")).expect("fixture JSON")
}

fn same_shape(actual: &Value, expected: &Value) {
    match expected {
        Value::Object(fields) => {
            let object = actual.as_object().expect("object expected");
            for (key, value) in fields {
                same_shape(object.get(key).expect("missing field"), value);
            }
        }
        Value::Array(_) => assert!(actual.is_array(), "array expected"),
        Value::String(_) => assert!(
            actual.is_string() || actual.is_null(),
            "string or null expected"
        ),
        Value::Number(_) => assert!(
            actual.is_number() || actual.is_null(),
            "number or null expected"
        ),
        Value::Bool(_) => assert!(actual.is_boolean(), "boolean expected"),
        Value::Null => assert!(
            actual.is_null() || actual.is_string() || actual.is_number(),
            "nullable scalar expected"
        ),
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_alphanumeric() || (i > 0 && (b == b'_' || b == b'-')))
}

#[tokio::test]
#[ignore]
async fn reads_memory_and_curator_shapes() {
    let env = match live_support::live_preconditions().await {
        Ok(env) => env,
        Err(reason) => {
            live_support::report_skip("LIVE_MEMORY_RESULT", &reason);
            return;
        }
    };
    let rest = client(env);
    let status = get(&rest, "/api/memory").await;
    same_shape(&status, &fixture("status-builtin.json"));
    let status_fixture = fixture("status-builtin.json");
    for provider in status["providers"].as_array().expect("providers") {
        same_shape(provider, &status_fixture["providers"][0]);
        let name = provider["name"].as_str().expect("name");
        if !valid_name(name) {
            continue;
        }
        let declared = get(
            &rest,
            &format!("/api/memory/providers/{name}/config?surface=declared"),
        )
        .await;
        same_shape(&declared, &fixture("declared-empty.json"));
        let plain = get(&rest, &format!("/api/memory/providers/{name}/config")).await;
        let baseline = match name {
            "byterover" => Some("byterover-plain.json"),
            "retaindb" => Some("retaindb-plain.json"),
            _ => None,
        };
        if let Some(name) = baseline {
            let baseline = fixture(name);
            same_shape(&plain, &baseline);
            let fields = plain["fields"].as_array().expect("fields");
            for expected in baseline["fields"].as_array().expect("fixture fields") {
                let actual = fields
                    .iter()
                    .find(|field| field["key"] == expected["key"])
                    .expect("fixture field present");
                same_shape(actual, expected);
            }
        } else {
            assert!(
                plain["name"].is_string()
                    && plain["label"].is_string()
                    && plain["fields"].is_array()
                    && plain["setup"].is_object()
            );
        }
    }
    same_shape(&get(&rest, "/api/curator").await, &fixture("curator.json"));
    println!("LIVE_MEMORY_RESULT: PASSED");
}

fn restore_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/live_memory_pause_restore.json")
}

struct Restore {
    rest: Arc<RestClient>,
    paused: bool,
    path: PathBuf,
    armed: bool,
}
impl Drop for Restore {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let rest = self.rest.clone();
        let paused = self.paused;
        let path = self.path.clone();
        let _ = std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            let restored = runtime.block_on(async {
                rest.request(
                    Method::Put,
                    "/api/curator/paused",
                    Some(&json!({"paused":paused})),
                    RequestOpts::default(),
                )
                .await
                .is_ok()
                    && get(&rest, "/api/curator").await["paused"] == paused
            });
            if restored {
                let _ = std::fs::remove_file(path);
            }
        })
        .join();
    }
}

#[tokio::test]
#[ignore]
async fn pause_round_trip_on_fixture() {
    if std::env::var("TALARIA_PROBE_WRITES").as_deref() != Ok("1") {
        println!("LIVE_MEMORY_WRITE_RESULT: SKIPPED (write opt-in absent)");
        return;
    }
    let env = match live_support::live_preconditions().await {
        Ok(env) => env,
        Err(reason) => {
            live_support::report_skip("LIVE_MEMORY_WRITE_RESULT", &reason);
            return;
        }
    };
    let rest = client(env);
    let path = restore_path();
    if path.exists() {
        let saved: Value =
            serde_json::from_slice(&std::fs::read(&path).expect("read restore receipt"))
                .expect("restore receipt JSON");
        let paused = saved["paused"].as_bool().expect("restore value");
        set_paused(&rest, paused).await;
        assert_eq!(get(&rest, "/api/curator").await["paused"], paused);
        std::fs::remove_file(&path).expect("remove restore receipt");
    }
    let paused = get(&rest, "/api/curator").await["paused"]
        .as_bool()
        .expect("paused status");
    std::fs::create_dir_all(path.parent().expect("receipt directory")).expect("receipt directory");
    use std::io::Write;
    let mut receipt = std::fs::File::create(&path).expect("create restore receipt");
    receipt
        .write_all(&serde_json::to_vec(&json!({"paused":paused})).unwrap())
        .expect("write restore receipt");
    receipt.sync_all().expect("sync restore receipt");
    let mut guard = Restore {
        rest: rest.clone(),
        paused,
        path: path.clone(),
        armed: true,
    };
    set_paused(&rest, !paused).await;
    assert_eq!(get(&rest, "/api/curator").await["paused"], !paused);
    set_paused(&rest, paused).await;
    assert_eq!(get(&rest, "/api/curator").await["paused"], paused);
    std::fs::remove_file(path).expect("remove restore receipt");
    guard.armed = false;
    println!("LIVE_MEMORY_WRITE_RESULT: PASSED");
}
