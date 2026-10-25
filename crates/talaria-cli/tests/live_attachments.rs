//! A live mention, image and file against the pinned gateway. Issue 224.
//!
//! Ignored unless someone asks for it. A skip prints `SKIPPED: <reason>` and, when
//! `TALARIA_LIVE_REQUIRE=1`, fails the test.

use std::path::PathBuf;
use std::time::Duration;

use hermes_client::{
    CallOptions, Client, ConnectOptions, DiscoverOptions, EnsurePolicy, Purpose, Scope,
    connect_with, default_home, discover_with, handshake, mint_ticket,
};
use hermes_contract::methods::canonical::{image_attach_bytes, prompt_submit};
use hermes_contract::methods::legacy::{complete_path, file_attach};
use serde_json::{Value, json};
use talaria_app::attachments::AttachmentsState;
use talaria_app::launch::NewSession;
use talaria_app::sessions::create_params;
use talaria_core::attachments::CollapseRule;

fn skip(reason: impl std::fmt::Display) -> bool {
    println!("SKIPPED: {reason}");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live test skipped: {reason}");
    }
    true
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let chunk = ((bytes[index] as u32) << 16)
            | ((bytes[index + 1] as u32) << 8)
            | bytes[index + 2] as u32;
        for shift in [18, 12, 6, 0] {
            out.push(TABLE[((chunk >> shift) & 63) as usize] as char);
        }
        index += 3;
    }
    if index < bytes.len() {
        let rest = bytes.len() - index;
        let chunk = ((bytes[index] as u32) << 16)
            | (bytes.get(index + 1).copied().unwrap_or(0) as u32) << 8;
        out.push(TABLE[((chunk >> 18) & 63) as usize] as char);
        out.push(TABLE[((chunk >> 12) & 63) as usize] as char);
        if rest == 2 {
            out.push(TABLE[((chunk >> 6) & 63) as usize] as char);
            out.push('=');
        } else {
            out.push('=');
            out.push('=');
        }
    }
    out
}

async fn call(client: &Client, method: &str, params: Value) -> Result<Value, String> {
    client
        .call_raw(
            method,
            params,
            CallOptions {
                timeout: Some(Duration::from_secs(30)),
            },
        )
        .await
        .map_err(|error| error.to_string())
}

/// Mention `@calc`, stage the red image, submit it, and attach a file outside the project.
#[tokio::test]
#[ignore]
async fn live_mention_image_file() {
    let home = std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home);
    let endpoint = match discover_with(
        &home,
        &DiscoverOptions {
            control_timeout: Duration::from_secs(5),
            ensure: EnsurePolicy::Never,
        },
    )
    .await
    {
        Ok(endpoint) => endpoint,
        Err(error) => {
            skip(error);
            return;
        }
    };
    if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
        skip(reason);
        return;
    }
    let profile = match endpoint.resolve_profile("fixture") {
        Ok(profile) => profile,
        Err(error) => {
            skip(error);
            return;
        }
    };
    let ticket = match mint_ticket(
        &home,
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    {
        Ok(ticket) => ticket,
        Err(error) => {
            skip(error);
            return;
        }
    };
    let mut connection = match connect_with(&endpoint, ticket, &ConnectOptions::default()).await {
        Ok(connection) => connection,
        Err(error) => {
            skip(error);
            return;
        }
    };
    if let Err(error) = handshake(&mut connection).await {
        skip(error);
        return;
    }
    let (client, _inbound) = Client::start(connection.into_parts());

    let project = tempfile::tempdir().expect("project");
    let source = project.path().join("src");
    std::fs::create_dir(&source).expect("src");
    std::fs::write(
        source.join("calc.py"),
        "def average(values): return sum(values) / len(values)\n",
    )
    .expect("calc");
    let created = match call(
        &client,
        "session.create",
        create_params(&NewSession {
            request_id: talaria_app::launch::new_request_id(),
            source: "tui".to_string(),
            cwd: project.path().to_path_buf(),
            model: None,
            provider: None,
            reasoning: None,
        }),
    )
    .await
    {
        Ok(created) => created,
        Err(error) => {
            skip(error);
            return;
        }
    };
    let Some(session_id) = created["session_id"].as_str() else {
        skip("session.create returned no session");
        return;
    };
    let session_id = session_id.to_string();
    let completed = match call(
        &client,
        complete_path::NAME,
        json!({"session_id": session_id, "word": "@calc"}),
    )
    .await
    {
        Ok(completed) => completed,
        Err(error) => {
            skip(error);
            return;
        }
    };
    let items = completed["items"].as_array().cloned().unwrap_or_default();
    if items.is_empty() {
        skip("complete.path returned no items");
        return;
    }
    assert!(
        items.iter().any(|item| item["text"] == "@file:src/calc.py"),
        "complete.path did not offer the project file"
    );

    let mut state = AttachmentsState::new(CollapseRule::HERMES_DEFAULT, 16 * 1024 * 1024);
    state.on_edit("@calc", 5, true, std::time::Instant::now());

    let png = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../talaria-app/tests/data/red-1x1.png"),
    )
    .expect("red image");
    let staged = call(
        &client,
        image_attach_bytes::NAME,
        json!({
            "session_id": session_id,
            "content_base64": encode_base64(&png),
            "filename": "11111111-1111-1111-1111-111111111111.png",
        }),
    )
    .await
    .expect("image.attach_bytes");
    assert_eq!(staged["attached"], true);
    assert_eq!(staged["mime"], "image/png");
    let path = staged["path"]
        .as_str()
        .filter(|path| !path.is_empty())
        .expect("image path");

    let receipt = call(
        &client,
        prompt_submit::NAME,
        json!({
            "session_id": session_id,
            "text": "[Image #1] What colour is this image? One word.",
            "submission_id": "live-sub-1",
            "attachments": [{"path": path, "mime": "image/png"}],
        }),
    )
    .await
    .expect("prompt.submit");
    let status = receipt["status"].as_str().unwrap_or_default();
    assert!(status == "queued" || status == "started", "{receipt}");

    let outside = tempfile::tempdir().expect("outside");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or(0);
    let name = format!("notes-{nanos:08x}.txt");
    let notes = outside.path().join(&name);
    std::fs::write(&notes, b"hello").expect("notes");
    let attached = call(
        &client,
        file_attach::NAME,
        json!({"session_id": session_id, "path": notes.display().to_string()}),
    )
    .await
    .expect("file.attach");
    assert_eq!(attached["uploaded"], true);
    assert_eq!(attached["name"], name);
    assert!(
        attached["ref_text"]
            .as_str()
            .unwrap_or_default()
            .starts_with("@file:")
    );
}
