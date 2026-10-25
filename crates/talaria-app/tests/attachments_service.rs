//! Gateway calls for mentions, images, files and submit. Issue 224.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hermes_client::{CallError, Client, RpcError};
use hermes_contract::methods::canonical::{self, prompt_submit};
use hermes_contract::methods::legacy::{
    clipboard_paste, complete_path, file_attach, image_attach, image_detach, input_detect_drop,
    paste_collapse, pdf_attach,
};
use hermes_contract::schemas::{
    CanonicalAttachedImageResult, CompletionItem, CompletionItemsResult, FileAttachResult,
};
use mock_gateway::MockGateway;
use serde_json::{Value, json};
use talaria_app::attachments::{
    AttachmentEffect, AttachmentEvent, AttachmentService, AttachmentsState, Feature, SubmitOutcome,
    refusal_notice,
};
use talaria_app::clipboard::{ClipboardImageReader, CommandError, CommandRunner, Platform};
use talaria_app::sessions::{SessionService, submit_frame};
use talaria_core::attachments::{
    Attachment, ChipId, CollapseRule, CompletionRequest, DropCandidate,
};
use talaria_core::session::{Effect, SessionId};

struct Idle;

impl CommandRunner for Idle {
    fn run(
        &self,
        program: &str,
        args: &[&str],
        stdout_to: Option<&Path>,
        timeout: Duration,
    ) -> Result<talaria_app::clipboard::CommandOutput, CommandError> {
        let _ = (program, args, stdout_to, timeout);
        Err(CommandError::NotFound)
    }
}

fn reader() -> ClipboardImageReader {
    ClipboardImageReader::new(Arc::new(Idle), Platform::Other, false)
}

fn service(
    client: Client,
    log: &Arc<Mutex<Vec<AttachmentEvent>>>,
    home: PathBuf,
) -> AttachmentService {
    let log = Arc::clone(log);
    AttachmentService::new(
        Some(client),
        Box::new(move |event| log.lock().expect("event log").push(event)),
        reader(),
        home,
    )
}

fn keys_of(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .expect("params object")
        .keys()
        .cloned()
        .collect()
}

fn session() -> SessionId {
    SessionId::new("local-session-1")
}

async fn settle() {
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
}

async fn settle_until(mut ready: impl FnMut() -> bool) {
    let started = std::time::Instant::now();
    while !ready() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "attachment task did not finish"
        );
        tokio::task::yield_now().await;
    }
    settle().await;
}

fn decode_base64(text: &str) -> Vec<u8> {
    fn value(byte: u8) -> u8 {
        match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("content_base64 is not standard base64"),
        }
    }
    assert!(!text.starts_with("data:"), "content_base64 must be plain");
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index + 1 < bytes.len() {
        let b0 = value(bytes[index]);
        let b1 = value(bytes[index + 1]);
        out.push((b0 << 2) | (b1 >> 4));
        if bytes.get(index + 2).is_none_or(|byte| *byte == b'=') {
            break;
        }
        let b2 = value(bytes[index + 2]);
        out.push((b1 << 4) | (b2 >> 2));
        if bytes.get(index + 3).is_none_or(|byte| *byte == b'=') {
            break;
        }
        let b3 = value(bytes[index + 3]);
        out.push((b2 << 6) | b3);
        index += 4;
    }
    out
}

fn uuid_filename(name: &str, ext: &str) -> bool {
    let Some((stem, found)) = name.rsplit_once('.') else {
        return false;
    };
    found == ext
        && stem.len() == 36
        && stem.chars().filter(|c| *c == '-').count() == 4
        && stem
            .chars()
            .all(|c| c == '-' || (c.is_ascii_hexdigit() && !c.is_ascii_uppercase()))
}

fn image_result(mime: &str) -> Value {
    let result = CanonicalAttachedImageResult {
        attached: true,
        bytes: None,
        count: None,
        height: None,
        message: None,
        mime: mime.to_string(),
        name: None,
        path: Some("<tmp>/cache/red.png".to_string()),
        remainder: None,
        text: None,
        token_estimate: None,
        width: None,
    };
    serde_json::to_value(result).expect("image result")
}

fn png_bytes() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/red-1x1.png");
    std::fs::read(path).expect("red image")
}

/// `complete.path` sends exactly `session_id` and `word`, waits 2 s, and a later generation wins.
#[tokio::test]
async fn complete_path_frame_and_stale_reply() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    tokio::time::pause();
    let log = Arc::new(Mutex::new(Vec::new()));
    let attachment = service(client, &log, PathBuf::from("/srv/talaria-home"));
    let session = session();
    let first = CompletionItemsResult {
        items: vec![CompletionItem {
            text: "@file:first.py".to_string(),
            display: "first.py".to_string(),
            meta: String::new(),
            kind: None,
        }],
    };
    let second = CompletionItemsResult {
        items: vec![CompletionItem {
            text: "@file:second.py".to_string(),
            display: "second.py".to_string(),
            meta: String::new(),
            kind: None,
        }],
    };
    gateway.push_result_after(
        Duration::from_millis(200),
        serde_json::to_value(first).expect("first"),
    );
    gateway.push_result_after(
        Duration::from_millis(10),
        serde_json::to_value(second).expect("second"),
    );

    attachment.execute(AttachmentEffect::Complete {
        session: session.clone(),
        request: CompletionRequest {
            word: "@calc".to_string(),
            generation: 1,
        },
    });
    attachment.execute(AttachmentEffect::Complete {
        session: session.clone(),
        request: CompletionRequest {
            word: "@cal".to_string(),
            generation: 2,
        },
    });
    settle_until(|| gateway.requests(complete_path::NAME).len() == 2).await;
    let calls = gateway.requests(complete_path::NAME);
    assert_eq!(calls.len(), 2);
    for (call, word) in calls.iter().zip(["@calc", "@cal"]) {
        assert_eq!(
            keys_of(&call.params),
            BTreeSet::from(["session_id".to_string(), "word".to_string()])
        );
        assert_eq!(call.params["session_id"], session.as_str());
        assert_eq!(call.params["word"], word);
    }
    tokio::time::advance(Duration::from_millis(20)).await;
    settle_until(|| {
        log.lock()
            .expect("event log")
            .iter()
            .any(|event| matches!(event, AttachmentEvent::Completions { generation: 2, .. }))
    })
    .await;
    tokio::time::advance(Duration::from_millis(200)).await;
    settle_until(|| log.lock().expect("event log").len() == 2).await;
    let mut mention = talaria_core::attachments::MentionState::new();
    let now = std::time::Instant::now();
    mention.on_edit("@calc", 5, now);
    mention.on_edit("@cal", 4, now);
    for event in log.lock().expect("event log").iter() {
        if let AttachmentEvent::Completions {
            generation, items, ..
        } = event
        {
            mention.on_items(*generation, items.clone());
        }
    }
    let view = mention.view(std::time::Instant::now());
    let shown: Vec<&str> = view.rows.iter().map(|(text, _)| text.as_str()).collect();
    assert!(shown.contains(&"second.py"), "{shown:?}");
    assert!(!shown.contains(&"first.py"), "{shown:?}");

    gateway.set_silent(complete_path::NAME, true);
    log.lock().expect("event log").clear();
    attachment.execute(AttachmentEffect::Complete {
        session,
        request: CompletionRequest {
            word: "@slow".to_string(),
            generation: 3,
        },
    });
    settle_until(|| gateway.requests(complete_path::NAME).len() == 3).await;
    tokio::time::advance(Duration::from_millis(1_999)).await;
    settle().await;
    assert!(
        log.lock().expect("event log").is_empty(),
        "the completion wait is 2 s"
    );
    tokio::time::advance(Duration::from_millis(2)).await;
    settle_until(|| !log.lock().expect("event log").is_empty()).await;
    match log.lock().expect("event log").as_slice() {
        [
            AttachmentEvent::CompletionFailed {
                reason, generation, ..
            },
        ] => {
            assert_eq!(*generation, 3);
            assert_eq!(reason, "Hermes did not answer in time.");
        }
        other => panic!("expected a completion failure, got {other:?}"),
    }
}

/// `image.attach_bytes` sends three keys, plain base64, a sniffed filename, and waits 30 s.
#[tokio::test]
async fn image_attach_bytes_frame() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    tokio::time::pause();
    let log = Arc::new(Mutex::new(Vec::new()));
    let attachment = service(client, &log, PathBuf::from("/srv/talaria-home"));
    let session = session();
    gateway.push_result(image_result("image/png"));
    gateway.push_result(image_result("image/jpeg"));
    let png = png_bytes();

    attachment.execute(AttachmentEffect::AttachImage {
        session: session.clone(),
        chip: ChipId(1),
        bytes: png.clone(),
        ext: "png",
    });
    settle_until(|| log.lock().expect("event log").len() == 1).await;
    let calls = gateway.requests(canonical::image_attach_bytes::NAME);
    assert_eq!(calls.len(), 1);
    assert_eq!(
        keys_of(&calls[0].params),
        BTreeSet::from([
            "session_id".to_string(),
            "content_base64".to_string(),
            "filename".to_string(),
        ])
    );
    assert_eq!(calls[0].params["session_id"], session.as_str());
    let encoded = calls[0].params["content_base64"].as_str().expect("base64");
    assert_eq!(decode_base64(encoded), png);
    let filename = calls[0].params["filename"].as_str().expect("filename");
    assert!(uuid_filename(filename, "png"), "{filename}");

    attachment.execute(AttachmentEffect::AttachImage {
        session: session.clone(),
        chip: ChipId(2),
        bytes: b"\xff\xd8\xff".to_vec(),
        ext: "png",
    });
    settle_until(|| log.lock().expect("event log").len() == 2).await;
    let calls = gateway.requests(canonical::image_attach_bytes::NAME);
    let filename = calls[1].params["filename"].as_str().expect("filename");
    assert!(uuid_filename(filename, "jpg"), "{filename}");
    assert_eq!(
        decode_base64(calls[1].params["content_base64"].as_str().unwrap()),
        b"\xff\xd8\xff"
    );

    gateway.set_silent(canonical::image_attach_bytes::NAME, true);
    log.lock().expect("event log").clear();
    attachment.execute(AttachmentEffect::AttachImage {
        session,
        chip: ChipId(3),
        bytes: png,
        ext: "png",
    });
    settle_until(|| gateway.requests(canonical::image_attach_bytes::NAME).len() == 3).await;
    tokio::time::advance(Duration::from_secs(29)).await;
    settle().await;
    assert!(
        log.lock().expect("event log").is_empty(),
        "the image wait is 30 s"
    );
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::time::advance(Duration::from_millis(1)).await;
    settle_until(|| !log.lock().expect("event log").is_empty()).await;
    match log.lock().expect("event log").as_slice() {
        [AttachmentEvent::AttachFailed { notice, chip, .. }] => {
            assert_eq!(*chip, Some(ChipId(3)));
            assert_eq!(
                notice,
                "The attachment may not have reached Hermes; attach it again."
            );
        }
        other => panic!("expected an attach failure, got {other:?}"),
    }
}

fn sparse(path: &Path, len: u64, prefix: &[u8]) {
    let mut file = std::fs::File::create(path).expect("create");
    use std::io::Write;
    file.write_all(prefix).expect("prefix");
    file.set_len(len).expect("length");
}

struct ModeGuard(PathBuf);

impl Drop for ModeGuard {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o644));
    }
}

fn refusal_notice_of(events: &[AttachmentEvent]) -> String {
    events
        .iter()
        .find_map(|event| match event {
            AttachmentEvent::DropRefused { notice, .. } => Some(notice.clone()),
            _ => None,
        })
        .expect("a refusal")
}

/// Size, type, permission and credential checks refuse before any gateway call.
#[tokio::test]
async fn drop_checks_and_refusals_without_call() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    tokio::time::pause();
    let home_dir = tempfile::tempdir().expect("home");
    let project = tempfile::tempdir().expect("project");
    let log = Arc::new(Mutex::new(Vec::new()));
    let attachment = service(client, &log, home_dir.path().to_path_buf());
    let session = session();
    let big_image = project.path().join("big.png");
    sparse(&big_image, 20 * 1024 * 1024 + 1, b"\x89PNG\r\n\x1a\n");
    let bmp = project.path().join("x.png");
    std::fs::write(&bmp, b"BM").expect("bmp");
    let svg = project.path().join("pic.svg");
    std::fs::write(&svg, b"<svg></svg>").expect("svg");
    let big_file = project.path().join("big.txt");
    sparse(&big_file, 50 * 1024 * 1024 + 1, b"hello");
    let locked_dir = project.path().join("locked");
    std::fs::create_dir(&locked_dir).expect("locked dir");
    let locked = locked_dir.join("notes.txt");
    std::fs::write(&locked, b"secret").expect("locked");
    let _guard = ModeGuard(locked.clone());
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("mode");
    let ssh = home_dir.path().join(".ssh/id_ed25519");
    std::fs::create_dir_all(ssh.parent().expect("ssh dir")).expect("ssh dir");
    std::fs::write(&ssh, b"key").expect("key");
    let env = project.path().join(".env");
    std::fs::write(&env, b"TOKEN=1").expect("env");
    std::fs::write(project.path().join("notes.txt"), b"hello").expect("notes");
    let folder = project.path().join("subdir");
    std::fs::create_dir(&folder).expect("folder");
    let winner = project.path().join("winner.txt");
    std::fs::write(&winner, b"win").expect("winner");

    let run = |path: PathBuf, original: &str| {
        log.lock().expect("event log").clear();
        attachment.execute(AttachmentEffect::ResolveDrop {
            session: session.clone(),
            folder: project.path().to_path_buf(),
            candidates: vec![DropCandidate {
                path,
                remainder: String::new(),
            }],
            original: original.to_string(),
        });
    };

    run(big_image, "big.png");
    settle_until(|| !log.lock().expect("event log").is_empty()).await;
    assert_eq!(
        refusal_notice_of(&log.lock().expect("log")),
        "That image is larger than 20 MiB."
    );
    run(bmp, "x.png");
    settle_until(|| !log.lock().expect("event log").is_empty()).await;
    assert_eq!(
        refusal_notice_of(&log.lock().expect("log")),
        "Hermes accepts PNG, JPEG, GIF and WebP images."
    );
    run(svg, "pic.svg");
    settle_until(|| !log.lock().expect("event log").is_empty()).await;
    assert_eq!(
        refusal_notice_of(&log.lock().expect("log")),
        "Hermes accepts PNG, JPEG, GIF and WebP images."
    );
    run(big_file, "big.txt");
    settle_until(|| !log.lock().expect("event log").is_empty()).await;
    assert_eq!(
        refusal_notice_of(&log.lock().expect("log")),
        "Talaria attaches files up to 50 MiB."
    );
    run(locked, "notes.txt");
    settle_until(|| !log.lock().expect("event log").is_empty()).await;
    assert_eq!(
        refusal_notice_of(&log.lock().expect("log")),
        "Talaria cannot read notes.txt."
    );
    run(ssh, "id_ed25519");
    settle_until(|| !log.lock().expect("event log").is_empty()).await;
    assert_eq!(
        refusal_notice_of(&log.lock().expect("log")),
        "Talaria does not attach credential files."
    );
    run(env, ".env");
    settle_until(|| !log.lock().expect("event log").is_empty()).await;
    assert_eq!(
        refusal_notice_of(&log.lock().expect("log")),
        "Talaria does not attach credential files."
    );
    assert!(
        gateway
            .requests(canonical::image_attach_bytes::NAME)
            .is_empty()
    );
    assert!(gateway.requests(file_attach::NAME).is_empty());

    log.lock().expect("event log").clear();
    let missing = PathBuf::from("/srv/talaria-missing/nope.txt");
    attachment.execute(AttachmentEffect::ResolveDrop {
        session: session.clone(),
        folder: project.path().to_path_buf(),
        candidates: vec![DropCandidate {
            path: missing,
            remainder: String::new(),
        }],
        original: "/srv/talaria-missing/nope.txt".to_string(),
    });
    settle_until(|| !log.lock().expect("event log").is_empty()).await;
    assert!(matches!(
        log.lock().expect("log").as_slice(),
        [AttachmentEvent::DropNotAFile { original, .. }] if original == "/srv/talaria-missing/nope.txt"
    ));

    log.lock().expect("event log").clear();
    attachment.execute(AttachmentEffect::ResolveDrop {
        session: session.clone(),
        folder: project.path().to_path_buf(),
        candidates: vec![DropCandidate {
            path: folder.clone(),
            remainder: String::new(),
        }],
        original: folder.display().to_string(),
    });
    settle_until(|| !log.lock().expect("event log").is_empty()).await;
    assert!(matches!(
        log.lock().expect("log").as_slice(),
        [AttachmentEvent::DropNotAFile { .. }]
    ));

    log.lock().expect("event log").clear();
    attachment.execute(AttachmentEffect::ResolveDrop {
        session: session.clone(),
        folder: project.path().to_path_buf(),
        candidates: vec![DropCandidate {
            path: PathBuf::from("./notes.txt"),
            remainder: String::new(),
        }],
        original: "./notes.txt".to_string(),
    });
    settle_until(|| !log.lock().expect("event log").is_empty()).await;
    match log.lock().expect("log").as_slice() {
        [AttachmentEvent::DropFile { path, .. }] => {
            assert_eq!(path, &project.path().join("notes.txt"));
        }
        other => panic!("relative path resolves in the session folder, got {other:?}"),
    }

    log.lock().expect("event log").clear();
    attachment.execute(AttachmentEffect::ResolveDrop {
        session,
        folder: project.path().to_path_buf(),
        candidates: vec![
            DropCandidate {
                path: folder,
                remainder: "winner.txt".to_string(),
            },
            DropCandidate {
                path: winner.clone(),
                remainder: String::new(),
            },
        ],
        original: format!("{} winner.txt", project.path().display()),
    });
    settle_until(|| !log.lock().expect("event log").is_empty()).await;
    match log.lock().expect("log").as_slice() {
        [AttachmentEvent::DropFile { path, .. }] => assert_eq!(path, &winner),
        other => panic!("the folder is skipped and the file wins, got {other:?}"),
    }
    assert!(
        gateway
            .requests(canonical::image_attach_bytes::NAME)
            .is_empty()
    );
    assert!(gateway.requests(file_attach::NAME).is_empty());
}

fn file_result(name: &str, uploaded: bool, ref_text: &str) -> Value {
    let result = FileAttachResult {
        attached: true,
        name: name.to_string(),
        path: "<tmp>/cache/notes.txt".to_string(),
        ref_path: "notes.txt".to_string(),
        ref_text: ref_text.to_string(),
        uploaded,
    };
    serde_json::to_value(result).expect("file result")
}

/// `file.attach` inserts `ref_text`, or a sanitized file label when the file was copied.
#[tokio::test]
async fn file_attach_inside_and_outside() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    tokio::time::pause();
    let dir = tempfile::tempdir().expect("dir");
    let log = Arc::new(Mutex::new(Vec::new()));
    let attachment = service(client, &log, PathBuf::from("/srv/talaria-home"));
    let session = session();
    let inside = dir.path().join("notes.txt");
    std::fs::write(&inside, b"hello").expect("notes");
    gateway.push_result(file_result("notes.txt", false, "@file:notes.txt"));
    gateway.push_result(file_result("a\u{1b}b.txt", true, "@file:a b.txt"));

    attachment.execute(AttachmentEffect::AttachFile {
        session: session.clone(),
        path: inside.clone(),
        remainder: String::new(),
    });
    settle_until(|| !log.lock().expect("event log").is_empty()).await;
    let calls = gateway.requests(file_attach::NAME);
    assert_eq!(calls.len(), 1);
    assert_eq!(
        keys_of(&calls[0].params),
        BTreeSet::from(["session_id".to_string(), "path".to_string()])
    );
    assert_eq!(calls[0].params["session_id"], session.as_str());
    assert_eq!(calls[0].params["path"], inside.display().to_string());
    assert!(inside.is_absolute());
    match log.lock().expect("log").as_slice() {
        [
            AttachmentEvent::FileAttached {
                name,
                ref_text,
                uploaded,
                ..
            },
        ] => {
            assert_eq!(name, "notes.txt");
            assert_eq!(ref_text, "@file:notes.txt");
            assert!(!uploaded);
        }
        other => panic!("expected the reference text, got {other:?}"),
    }

    log.lock().expect("event log").clear();
    let outside = dir.path().join("outside.txt");
    std::fs::write(&outside, b"out").expect("outside");
    attachment.execute(AttachmentEffect::AttachFile {
        session,
        path: outside,
        remainder: " please".to_string(),
    });
    settle_until(|| !log.lock().expect("event log").is_empty()).await;
    match log.lock().expect("log").as_slice() {
        [
            AttachmentEvent::FileAttached {
                name,
                uploaded,
                remainder,
                ..
            },
        ] => {
            assert_eq!(name, "a\u{241b}b.txt");
            assert!(*uploaded);
            assert_eq!(remainder, " please");
            assert!(!name.contains('\u{1b}'));
        }
        other => panic!("expected a sanitized file name, got {other:?}"),
    }
}

fn rpc(code: i64, reason: &str) -> CallError {
    CallError::Rpc(RpcError {
        code,
        message: reason.to_string(),
        reason: Some(reason.to_string()),
        data: None,
    })
}

/// Each gateway refusal becomes the sentence for that row.
#[test]
fn refusals_map_to_notices() {
    let rows = [
        (
            rpc(4001, "invalid_params"),
            Feature::ImageAttach,
            "Hermes could not accept that attachment (unsupported or damaged).",
        ),
        (
            rpc(4001, "permission_denied"),
            Feature::FileAttach,
            "This session is not attached on this connection yet; try again in a moment.",
        ),
        (
            rpc(4001, "storage_unavailable"),
            Feature::ImageAttach,
            "Hermes could not store the attachment (disk or permissions).",
        ),
        (
            rpc(5001, "disk"),
            Feature::FileAttach,
            "Hermes could not store the attachment (disk or permissions).",
        ),
        (
            rpc(4001, "not_found"),
            Feature::ImageAttach,
            "Hermes does not know this session.",
        ),
        (
            rpc(-32601, "method_not_found"),
            Feature::Completion,
            "This Hermes gateway does not support @ completion.",
        ),
        (
            rpc(-32601, "method_not_found"),
            Feature::FileAttach,
            "This Hermes gateway does not support file attachments.",
        ),
        (
            rpc(-32601, "method_not_found"),
            Feature::ImageAttach,
            "This Hermes gateway does not support image attachments.",
        ),
        (
            CallError::TimedOut {
                method: canonical::image_attach_bytes::NAME.to_string(),
                after: Duration::from_secs(30),
            },
            Feature::ImageAttach,
            "The attachment may not have reached Hermes; attach it again.",
        ),
        (
            CallError::Unknown {
                method: file_attach::NAME.to_string(),
            },
            Feature::FileAttach,
            "The attachment may not have reached Hermes; attach it again.",
        ),
        (
            rpc(4001, "nope"),
            Feature::ImageAttach,
            "Hermes refused: nope",
        ),
    ];
    for (error, feature, sentence) in rows {
        assert_eq!(refusal_notice(&error, feature), sentence, "{error:?}");
    }
}

/// An empty attachment list is omitted. Two images stay in text order. Outcomes do not retry.
#[test]
fn submit_attachments_and_outcomes() {
    let empty = submit_frame("local-session-1", "hello", "sub-1", &[]);
    assert!(empty.get("attachments").is_none());
    assert!(empty.get("queued").is_none());
    let encoded = empty.to_string();
    assert!(!encoded.contains("queued"));
    assert!(!encoded.contains("unattended"));
    assert_eq!(
        keys_of(&empty),
        BTreeSet::from([
            "session_id".to_string(),
            "text".to_string(),
            "submission_id".to_string(),
        ])
    );

    let ordered = submit_frame(
        "local-session-1",
        "[Image #2] then [Image #1]",
        "sub-1",
        &[
            Attachment {
                path: "<tmp>/cache/b.png".to_string(),
                mime: "image/png".to_string(),
            },
            Attachment {
                path: "<tmp>/cache/a.png".to_string(),
                mime: "image/jpeg".to_string(),
            },
        ],
    );
    assert!(ordered.get("queued").is_none());
    assert!(!ordered.to_string().contains("unattended"));
    assert_eq!(ordered["attachments"][0]["path"], "<tmp>/cache/b.png");
    assert_eq!(ordered["attachments"][1]["path"], "<tmp>/cache/a.png");

    let mut state = AttachmentsState::new(CollapseRule::HERMES_DEFAULT, 16 * 1024 * 1024);
    let plain = state.on_submit("hello").expect("plain text submits");
    assert_eq!(plain.text, "hello");
    assert!(plain.attachments.is_empty());

    let session = session();
    let (first, _) = state.chips.begin_image("", &session).expect("image");
    state.chips.image_ready(
        first,
        "<tmp>/cache/a.png".to_string(),
        "image/png".to_string(),
    );
    let (second, _) = state
        .chips
        .begin_image("[Image #1]", &session)
        .expect("image");
    state.chips.image_ready(
        second,
        "<tmp>/cache/b.png".to_string(),
        "image/png".to_string(),
    );
    let draft = "[Image #2] then [Image #1]";
    let expanded = state.on_submit(draft).expect("ready images submit");
    assert_eq!(expanded.attachments[0].path, "<tmp>/cache/b.png");
    assert_eq!(expanded.attachments[1].path, "<tmp>/cache/a.png");
    assert!(!expanded.text.is_empty());

    let _ = state.on_submit_outcome(SubmitOutcome::Receipt);
    let sent = state.on_submit(draft).expect("sent labels are plain");
    assert!(sent.attachments.is_empty());

    let mut kept = AttachmentsState::new(CollapseRule::HERMES_DEFAULT, 16 * 1024 * 1024);
    let (id, _) = kept.chips.begin_image("", &session).expect("image");
    kept.chips
        .image_ready(id, "<tmp>/cache/a.png".to_string(), "image/png".to_string());
    let _ = kept.on_submit("[Image #1]");
    let effects = kept.on_submit_outcome(SubmitOutcome::Refused {
        reason: "permission_denied".to_string(),
    });
    assert!(effects.iter().all(|effect| !matches!(
        effect,
        AttachmentEffect::Notice(text) if text.contains("no longer has")
    )));
    let again = kept.on_submit("[Image #1]").expect("still ready");
    assert_eq!(again.attachments.len(), 1);

    let effects = kept.on_submit_outcome(SubmitOutcome::Refused {
        reason: "invalid_params".to_string(),
    });
    assert!(effects.iter().any(|effect| matches!(
        effect,
        AttachmentEffect::Notice(text)
            if text == "Hermes no longer has the attached images; attach them again."
    )));
    let plain = kept.on_submit("[Image #1]").expect("labels became plain");
    assert!(plain.attachments.is_empty());

    let mut lost = AttachmentsState::new(CollapseRule::HERMES_DEFAULT, 16 * 1024 * 1024);
    let (id, _) = lost.chips.begin_image("", &session).expect("image");
    lost.chips
        .image_ready(id, "<tmp>/cache/a.png".to_string(), "image/png".to_string());
    let _ = lost.on_submit("[Image #1]");
    let effects = lost.on_submit_outcome(SubmitOutcome::Unknown);
    assert!(effects.is_empty());
    let still = lost
        .on_submit("[Image #1]")
        .expect("a lost reply keeps the image");
    assert_eq!(still.attachments.len(), 1);
}

/// The allowed calls are the positive control. The six legacy attach names are never sent.
#[tokio::test]
async fn never_calls_legacy_attach_methods() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    tokio::time::pause();
    let log = Arc::new(Mutex::new(Vec::new()));
    let attachment = service(client.clone(), &log, PathBuf::from("/srv/talaria-home"));
    let session = session();
    let directory = tempfile::tempdir().expect("project");
    let notes = directory.path().join("notes.txt");
    std::fs::write(&notes, b"notes").expect("notes");
    gateway.push_result(serde_json::to_value(CompletionItemsResult { items: Vec::new() }).unwrap());
    gateway.push_result(file_result("notes.txt", false, "@file:notes.txt"));
    gateway.push_result(image_result("image/png"));
    gateway.push_result(json!({"status": "queued"}));

    attachment.execute(AttachmentEffect::Complete {
        session: session.clone(),
        request: CompletionRequest {
            word: "@calc".to_string(),
            generation: 1,
        },
    });
    settle_until(|| log.lock().expect("event log").len() == 1).await;
    attachment.execute(AttachmentEffect::AttachFile {
        session: session.clone(),
        path: notes,
        remainder: String::new(),
    });
    settle_until(|| log.lock().expect("event log").len() == 2).await;
    attachment.execute(AttachmentEffect::AttachImage {
        session: session.clone(),
        chip: ChipId(1),
        bytes: png_bytes(),
        ext: "png",
    });
    settle_until(|| log.lock().expect("event log").len() == 3).await;
    let sessions = SessionService::with_client("launch", client);
    let _ = sessions
        .execute(Effect::SendPrompt {
            session_id: session,
            text: "[Image #1] What colour is this image? One word.".to_string(),
            submission_id: "sub-1".to_string(),
            attachments: vec![Attachment {
                path: "<tmp>/cache/red.png".to_string(),
                mime: "image/png".to_string(),
            }],
        })
        .await;
    settle().await;

    let seen: Vec<String> = gateway
        .seen()
        .into_iter()
        .filter_map(|frame| frame.method)
        .collect();
    for name in [
        complete_path::NAME,
        file_attach::NAME,
        canonical::image_attach_bytes::NAME,
        prompt_submit::NAME,
    ] {
        assert!(seen.iter().any(|method| method == name), "missing {name}");
    }
    for name in [
        clipboard_paste::NAME,
        image_attach::NAME,
        image_detach::NAME,
        input_detect_drop::NAME,
        pdf_attach::NAME,
        paste_collapse::NAME,
    ] {
        assert!(seen.iter().all(|method| method != name), "saw {name}");
    }
}
