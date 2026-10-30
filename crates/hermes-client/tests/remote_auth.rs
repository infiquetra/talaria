//! Authentication and token storage against the gated gateway double.

// Avoid formatting token-bearing values when an assertion fails.
#![allow(clippy::err_expect)]

#[path = "support/mock_gated.rs"]
mod mock_gated;

use hermes_client::remote::{
    RemoteConfig, RemoteName, SignInError, SignInOptions, TokenSet, TokenStore, WsTicket,
};
use hermes_client::remote::{loopback::CallbackListener, native_auth, pkce, status, ws_ticket};
use mock_gated::{MockGateway, Options};
use std::collections::HashSet;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;

const VERIFIER: &str = "SENTINEL-verifier-for-testing-only";
const STATE: &str = "SENTINEL-state-for-testing-only";
fn config(base: Url) -> RemoteConfig {
    RemoteConfig {
        name: RemoteName::for_test("work"),
        base_url: base,
        provider: Some("basic".into()),
    }
}
fn options() -> SignInOptions {
    SignInOptions {
        no_browser: true,
        paste: false,
        cancel: tokio_util::sync::CancellationToken::new(),
    }
}
fn tokens(expires_at: i64, saved_at: i64) -> TokenSet {
    TokenSet::for_test("SENTINEL-access", "SENTINEL-refresh", expires_at, saved_at)
}
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
fn seed(store: &TokenStore, expires: i64, saved: i64, mode: u32) {
    let path = store.path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, serde_json::json!({"version":1,"access_token":"SENTINEL-access","refresh_token":"SENTINEL-refresh","expires_at":expires,"provider":"basic","user_id":"fixture","saved_at":saved}).to_string()).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}
fn store(home: &std::path::Path) -> TokenStore {
    TokenStore::new(home, RemoteName::for_test("work"))
}
async fn get(url: &Url) -> (u16, Vec<(String, String)>, String) {
    let port = url.port_or_known_default().unwrap();
    let mut stream = tokio::net::TcpStream::connect((url.host_str().unwrap(), port))
        .await
        .unwrap();
    let target = match url.query() {
        Some(q) => format!("{}?{q}", url.path()),
        None => url.path().to_owned(),
    };
    stream
        .write_all(
            format!(
                "GET {target} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
                url.host_str().unwrap()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let raw = String::from_utf8(bytes).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap();
    let mut lines = head.lines();
    let code = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers = lines
        .filter_map(|line| {
            line.split_once(':')
                .map(|(a, b)| (a.to_ascii_lowercase(), b.trim().to_owned()))
        })
        .collect();
    (code, headers, body.to_owned())
}
async fn code_for(gateway: &MockGateway) -> String {
    let challenge = {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
        use sha2::{Digest, Sha256};
        URL_SAFE_NO_PAD.encode(Sha256::digest(VERIFIER.as_bytes()))
    };
    let mut url = gateway.base();
    url.set_path("/hermes/auth/native/authorize");
    url.query_pairs_mut()
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("redirect_uri", "http://127.0.0.1:1/callback")
        .append_pair("state", STATE);
    let (status, headers, _) = get(&url).await;
    assert_eq!(status, 302);
    let location = headers
        .iter()
        .find(|(k, _)| k == "location")
        .unwrap()
        .1
        .clone();
    Url::parse(&location)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "code")
        .unwrap()
        .1
        .into_owned()
}

#[test]
fn pkce_matches_rfc7636_vector() {
    assert_eq!(
        pkce::challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}

#[test]
fn pkce_pair_is_fresh_43_chars_url_safe() {
    let mut seen = HashSet::new();
    for _ in 0..1000 {
        let pair = pkce::generate();
        for value in [&pair.verifier, &pair.state] {
            assert_eq!(value.len(), 43);
            assert!(
                value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            );
            assert!(seen.insert(value.clone()), "reused random value");
        }
        assert!(
            pair.verifier != pair.state,
            "verifier and state must differ"
        );
        assert_eq!(pair.challenge, pkce::challenge(&pair.verifier));
    }
}

#[test]
fn authorize_url_is_canonical_and_keeps_prefix() {
    let base = Url::parse("https://gateway.example.com/hermes/").unwrap();
    let redirect = Url::parse("http://127.0.0.1:43121/callback").unwrap();
    let url = pkce::authorize_url(&base, "challenge", &redirect, STATE, Some("basic"));
    assert_eq!(url.path(), "/hermes/auth/native/authorize");
    assert!(
        url.as_str()
            .contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A43121%2Fcallback")
    );
    let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
    assert_eq!(
        query.get("code_challenge_method").map(|s| s.as_ref()),
        Some("S256")
    );
    assert_eq!(
        query.get("code_challenge").map(|s| s.as_ref()),
        Some("challenge")
    );
    assert_eq!(
        query.get("redirect_uri").map(|s| s.as_ref()),
        Some(redirect.as_str())
    );
    assert_eq!(query.get("provider").map(|s| s.as_ref()), Some("basic"));
    assert!(query.get("state").is_some_and(|s| s.as_ref() == STATE));
    let no_provider = pkce::authorize_url(&base, "challenge", &redirect, STATE, None);
    assert!(!no_provider.query_pairs().any(|(k, _)| k == "provider"));
}

#[tokio::test]
async fn redirect_uri_is_ip_literal_http_callback() {
    let listener = CallbackListener::bind().await.unwrap();
    let url = listener.redirect_uri();
    assert_eq!(url.scheme(), "http");
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.port().is_some_and(|p| p > 0));
    assert_eq!(url.path(), "/callback");
    assert!(!url.as_str().contains("localhost"));
}

#[tokio::test]
async fn other_paths_get_404_and_keep_waiting() {
    let listener = CallbackListener::bind().await.unwrap();
    let url = listener.redirect_uri();
    let task = tokio::spawn(listener.wait(STATE, Duration::from_secs(600)));
    let mut other = url.clone();
    other.set_path("/favicon.ico");
    assert_eq!(get(&other).await.0, 404);
    assert!(!task.is_finished());
    let mut callback = url;
    callback
        .query_pairs_mut()
        .append_pair("code", "SENTINEL-code")
        .append_pair("state", STATE);
    assert_eq!(get(&callback).await.0, 200);
    assert!(task.await.unwrap().unwrap() == "SENTINEL-code");
}

#[tokio::test]
async fn callback_completes_once_then_port_closes() {
    let gateway = MockGateway::start(Options {
        auto_approve: true,
        ..Options::default()
    })
    .await;
    let listener = CallbackListener::bind().await.unwrap();
    let url = listener.redirect_uri();
    let code = code_for(&gateway).await;
    let task = tokio::spawn(listener.wait(STATE, Duration::from_secs(600)));
    let mut callback = url.clone();
    callback
        .query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", STATE);
    assert_eq!(get(&callback).await.0, 200);
    let received = task.await.unwrap().unwrap();
    let _ = native_auth::exchange(&config(gateway.base()), &received, VERIFIER)
        .await
        .unwrap();
    assert_eq!(gateway.counts().token, 1);
    assert!(
        tokio::net::TcpStream::connect((url.host_str().unwrap(), url.port().unwrap()))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn callback_state_mismatch_never_redeems() {
    let gateway = MockGateway::start(Options::default()).await;
    let listener = CallbackListener::bind().await.unwrap();
    let mut url = listener.redirect_uri();
    let task = tokio::spawn(listener.wait(STATE, Duration::from_secs(600)));
    url.query_pairs_mut()
        .append_pair("code", "SENTINEL-code")
        .append_pair("state", "wrong");
    let _ = get(&url).await;
    assert!(matches!(
        task.await.unwrap(),
        Err(SignInError::StateMismatch)
    ));
    assert_eq!(gateway.counts().token, 0);
}

#[tokio::test]
async fn callback_error_param_is_reported() {
    let listener = CallbackListener::bind().await.unwrap();
    let mut url = listener.redirect_uri();
    let task = tokio::spawn(listener.wait(STATE, Duration::from_secs(600)));
    url.query_pairs_mut().append_pair("error", "access_denied");
    let _ = get(&url).await;
    assert!(
        matches!(task.await.unwrap(),Err(SignInError::Denied(reason)) if reason=="access_denied")
    );
}

#[tokio::test(start_paused = true)]
async fn listener_times_out_and_closes_port() {
    let listener = CallbackListener::bind().await.unwrap();
    let url = listener.redirect_uri();
    let task = tokio::spawn(listener.wait(STATE, Duration::from_secs(600)));
    tokio::time::advance(Duration::from_secs(600)).await;
    assert!(matches!(task.await.unwrap(), Err(SignInError::TimedOut)));
    assert!(
        tokio::net::TcpStream::connect((url.host_str().unwrap(), url.port().unwrap()))
            .await
            .is_err()
    );
}

#[test]
fn paste_back_reads_without_echo_and_completes() {
    use portable_pty::{PtySize, native_pty_system};
    use rustix::termios::{LocalModes, tcgetattr};
    let pair = native_pty_system().openpty(PtySize::default()).unwrap();
    let tty = pair.master.tty_name().unwrap();
    let mut slave = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(tty)
        .unwrap();
    let check = slave.try_clone().unwrap();
    let task = std::thread::spawn(move || {
        let mut prompt = Vec::new();
        let result = native_auth::read_paste(&mut slave, &mut prompt, STATE);
        (result, prompt)
    });
    let began = std::time::Instant::now();
    while tcgetattr(&check)
        .unwrap()
        .local_modes
        .contains(LocalModes::ECHO)
        && !task.is_finished()
        && began.elapsed() < Duration::from_secs(2)
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    if task.is_finished() {
        let _ = task.join().expect("paste reader");
        panic!("paste reader ended before input");
    }
    assert!(
        !tcgetattr(&check)
            .unwrap()
            .local_modes
            .contains(LocalModes::ECHO)
    );
    let mut writer = pair.master.take_writer().unwrap();
    writeln!(
        writer,
        "http://127.0.0.1:1/callback?code=SENTINEL-code&state={STATE}"
    )
    .unwrap();
    let (result, prompt) = task.join().unwrap();
    assert!(result.is_ok());
    assert!(!String::from_utf8_lossy(&prompt).contains("SENTINEL-"));
    let mut bad = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(pair.master.tty_name().unwrap())
        .unwrap();
    let mut output = Vec::new();
    writeln!(writer, "not a URL").unwrap();
    let error = native_auth::read_paste(&mut bad, &mut output, STATE)
        .err()
        .expect("bad paste rejected");
    let text = error.to_string();
    assert!(!text.contains("not a URL") && !text.contains('\n'));
}

#[tokio::test]
async fn authorize_url_printed_once_to_stderr_only() {
    if std::env::var("TALARIA_TEST_SIGN_IN_CHILD").as_deref() != Ok("1") {
        let directory = tempfile::tempdir().unwrap();
        let browser = directory.path().join("browser");
        let marker = directory.path().join("called");
        std::fs::write(
            &browser,
            "#!/bin/sh\nprintf called > \"$TALARIA_BROWSER_MARKER\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&browser, std::fs::Permissions::from_mode(0o700)).unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "authorize_url_printed_once_to_stderr_only",
                "--nocapture",
            ])
            .env("TALARIA_TEST_SIGN_IN_CHILD", "1")
            .env("BROWSER", browser)
            .env("TALARIA_BROWSER_MARKER", &marker)
            .output()
            .expect("sign-in test process");
        assert!(
            result.status.success(),
            "sign-in subprocess did not complete"
        );
        assert!(
            !String::from_utf8_lossy(&result.stdout).contains("http://"),
            "authorize URL reached stdout"
        );
        assert!(!marker.exists(), "browser opened despite --no-browser");
        return;
    }
    let gateway = MockGateway::start(Options {
        auto_approve: true,
        ..Options::default()
    })
    .await;
    let cfg = config(gateway.base());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let stdout = Arc::new(Mutex::new(Vec::new()));
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let browsers = Arc::new(AtomicUsize::new(0));
    let count = browsers.clone();
    let io = native_auth::SignInIo {
        stdout: stdout.clone(),
        stderr: stderr.clone(),
        browser: Arc::new(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
        }),
        authorize: tx,
    };
    let task = tokio::spawn(async move { native_auth::sign_in_with_io(&cfg, options(), io).await });
    let authorize = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .expect("authorize URL");
    let (status, headers, _) = get(&authorize).await;
    assert_eq!(status, 302);
    let callback = Url::parse(&headers.iter().find(|(k, _)| k == "location").unwrap().1).unwrap();
    let _ = get(&callback).await;
    assert!(
        tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .expect("sign-in timed out")
            .unwrap()
            .is_ok()
    );
    let out = stdout.lock().unwrap();
    let err = stderr.lock().unwrap();
    assert!(out.is_empty());
    let text = String::from_utf8_lossy(&err);
    assert_eq!(
        text.matches("Open this URL in a browser to sign in:")
            .count(),
        1
    );
    assert_eq!(text.lines().filter(|l| l.starts_with("http")).count(), 1);
    assert_eq!(browsers.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn exchange_parses_bearer_payload() {
    let gateway = MockGateway::start(Options {
        auto_approve: true,
        ..Options::default()
    })
    .await;
    let code = code_for(&gateway).await;
    let result = native_auth::exchange(&config(gateway.base()), &code, VERIFIER)
        .await
        .unwrap();
    assert!(result.access_for_test() == "SENTINEL-access");
    assert!(result.refresh_for_test() == "SENTINEL-refresh");
    assert_eq!(result.provider, "basic");
    assert_eq!(result.user_id, "fixture");
    assert!(result.expires_at > now());
    assert_eq!(gateway.counts().token, 1);
    let expired = code_for(&gateway).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(121)).await;
    assert!(matches!(
        native_auth::exchange(&config(gateway.base()), &expired, VERIFIER).await,
        Err(SignInError::CodeRejected)
    ));
    assert_eq!(gateway.counts().token, 2);
}

#[tokio::test]
async fn exchange_failures_are_never_retried() {
    let mut messages = HashSet::new();
    for status in [400, 422, 500] {
        let g = MockGateway::start(Options {
            token_status: Some(status),
            ..Options::default()
        })
        .await;
        let result = native_auth::exchange(&config(g.base()), "SENTINEL-code", VERIFIER).await;
        let err = result.err().expect("exchange fails");
        messages.insert(err.to_string());
        assert_eq!(g.counts().token, 1);
    }
    let g = MockGateway::start(Options {
        token_drop: true,
        ..Options::default()
    })
    .await;
    let err = native_auth::exchange(&config(g.base()), "SENTINEL-code", VERIFIER)
        .await
        .err()
        .expect("dropped exchange");
    messages.insert(err.to_string());
    assert_eq!(g.counts().token, 1);
    assert!(messages.len() >= 4);
    assert!(messages.iter().any(|s| s.contains("refused or expired")));
    assert!(messages.iter().any(|s| s.contains("HTTP 422")));
    assert!(
        messages
            .iter()
            .filter(|s| s.contains("outcome unknown"))
            .count()
            >= 2
    );
    let g = MockGateway::start(Options {
        token_stall: true,
        ..Options::default()
    })
    .await;
    let cfg = config(g.base());
    let task =
        tokio::spawn(async move { native_auth::exchange(&cfg, "SENTINEL-code", VERIFIER).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while g.counts().token == 0 {
            if task.is_finished() {
                panic!("exchange ended before request");
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("token request");
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(30)).await;
    assert!(task.await.unwrap().is_err());
    assert_eq!(g.counts().token, 1);
}

#[tokio::test]
async fn refresh_saves_before_use_and_accepts_unchanged_strings() {
    let home = tempfile::tempdir().unwrap();
    let store = store(home.path());
    seed(&store, now() + 60, 1, 0o600);
    let g = MockGateway::start(Options {
        ticket_snapshot: Some(store.path()),
        ..Options::default()
    })
    .await;
    let _ = ws_ticket::mint_ws_ticket(&config(g.base()), &store)
        .await
        .unwrap();
    let count = g.counts();
    assert_eq!(count.refresh, 1);
    assert_eq!(count.tickets, 1);
    let saved: serde_json::Value = serde_json::from_slice(
        count
            .ticket_snapshot
            .as_deref()
            .expect("saved before ticket"),
    )
    .unwrap();
    assert!(saved["expires_at"].as_i64().unwrap() > now());
    assert!(
        saved["access_token"] == "SENTINEL-access" && saved["refresh_token"] == "SENTINEL-refresh",
        "unchanged token strings must be accepted"
    );
    assert!(store.load().unwrap().is_some());
}

#[tokio::test]
async fn refresh_session_expired_clears_store() {
    let home = tempfile::tempdir().unwrap();
    let store = store(home.path());
    seed(&store, now() - 1, 1, 0o600);
    let g = MockGateway::start(Options {
        refresh_expired: true,
        ..Options::default()
    })
    .await;
    let error = native_auth::refresh(&config(g.base()), &store)
        .await
        .err()
        .expect("expired");
    assert!(matches!(
        error,
        hermes_client::remote::RefreshError::SessionExpired
    ));
    assert!(!store.path().exists());
    assert!(error.to_string().contains("talaria remote login"));
}

#[tokio::test]
async fn refresh_503_keeps_tokens() {
    let home = tempfile::tempdir().unwrap();
    let store = store(home.path());
    seed(&store, now() - 1, 1, 0o600);
    let before = std::fs::read(store.path()).unwrap();
    let g = MockGateway::start(Options {
        refresh_down: true,
        ..Options::default()
    })
    .await;
    assert!(matches!(
        native_auth::refresh(&config(g.base()), &store).await,
        Err(hermes_client::remote::RefreshError::ProviderUnreachable)
    ));
    assert!(std::fs::read(store.path()).unwrap() == before);
    assert_eq!(g.counts().refresh, 1);
}

#[tokio::test]
async fn refresh_is_single_flight_and_rotating_safe() {
    let home = tempfile::tempdir().unwrap();
    let store = store(home.path());
    seed(&store, now() - 1, 1, 0o600);
    let g = MockGateway::start(Options {
        rotating: true,
        ..Options::default()
    })
    .await;
    let cfg = config(g.base());
    let (a, b) = tokio::join!(
        native_auth::refresh(&cfg, &store),
        native_auth::refresh(&cfg, &store)
    );
    assert!(a.is_ok());
    assert!(b.is_ok());
    assert_eq!(g.counts().refresh, 1);
}

#[tokio::test]
async fn refresh_uses_newer_file_from_another_process() {
    let home = tempfile::tempdir().unwrap();
    let store = store(home.path());
    let old = tokens(now() - 1, 1);
    seed(&store, now() + 3600, 2, 0o600);
    let g = MockGateway::start(Options::default()).await;
    let found = native_auth::refresh_held(&config(g.base()), &store, &old)
        .await
        .unwrap();
    assert_eq!(found.saved_at, 2);
    assert_eq!(g.counts().refresh, 0);
}

#[test]
fn token_store_modes_and_atomic_replace() {
    let home = tempfile::tempdir().unwrap();
    let store = store(home.path());
    store.save(&tokens(now() + 100, 1)).unwrap();
    assert_eq!(
        std::fs::metadata(store.path().parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(store.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let before = std::fs::read(store.path()).unwrap();
    store.fail_before_rename(true);
    assert!(store.save(&tokens(now() + 200, 2)).is_err());
    assert!(std::fs::read(store.path()).unwrap() == before);
    store.fail_before_rename(false);
    std::fs::write(store.path(), r#"{"version":999}"#).unwrap();
    assert!(
        store.load().unwrap().is_none(),
        "unknown token versions are signed out"
    );
}

#[test]
fn token_file_readable_by_others_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let store = store(home.path());
    seed(&store, now() + 100, 1, 0o644);
    let error = store.load().err().expect("open permissions refused");
    assert_eq!(
        error.to_string(),
        "token file for work is readable by others; fix its permissions or sign in again"
    );
}

#[test]
fn secrets_never_formatted() {
    let tokens = tokens(now() + 100, 1);
    let ticket = WsTicket::for_test("SENTINEL-ticket");
    let pair = pkce::PkcePair {
        verifier: "SENTINEL-verifier".into(),
        challenge: "challenge".into(),
        state: "SENTINEL-state".into(),
    };
    assert!(
        !format!("{pair:?}").contains("SENTINEL-"),
        "proof key leaked through debug"
    );
    let errors = [
        SignInError::Insecure,
        SignInError::NotGated,
        SignInError::NoNativeFlow,
        SignInError::StateMismatch,
        SignInError::Denied("SENTINEL-code".into()),
        SignInError::Cancelled,
        SignInError::TimedOut,
        SignInError::CodeRejected,
        SignInError::RequestRejected,
        SignInError::OutcomeUnknown("SENTINEL-verifier".into()),
        SignInError::Io("SENTINEL-state".into()),
    ];
    for text in [
        format!("{tokens:?}"),
        format!("{tokens}"),
        format!("{ticket:?}"),
        format!("{ticket}"),
    ] {
        assert!(
            !text.contains("SENTINEL-"),
            "secret leaked through formatting"
        );
    }
    for error in errors {
        assert!(
            !format!("{error:?}").contains("SENTINEL-"),
            "secret leaked through debug"
        );
        assert!(
            !error.to_string().contains("SENTINEL-"),
            "secret leaked through display"
        );
    }
    let refresh = hermes_client::remote::RefreshError::Other("SENTINEL-refresh".into());
    assert!(
        !format!("{refresh:?}").contains("SENTINEL-") && !refresh.to_string().contains("SENTINEL-"),
        "refresh error leaked a sentinel"
    );
    let remote = hermes_client::remote::RemoteError::Other("SENTINEL-ticket".into());
    assert!(
        !format!("{remote:?}").contains("SENTINEL-") && !remote.to_string().contains("SENTINEL-"),
        "remote error leaked a sentinel"
    );
}

#[tokio::test]
async fn status_requires_gate_and_native_pkce() {
    for (opts, expected) in [
        (
            Options {
                ungated: true,
                ..Options::default()
            },
            Some("is not gated"),
        ),
        (
            Options {
                no_native: true,
                ..Options::default()
            },
            Some("has no interactive sign-in provider"),
        ),
        (Options::default(), None),
    ] {
        let gateway = MockGateway::start(opts).await;
        let result = status::probe_status(&gateway.base()).await;
        match expected {
            Some(text) => {
                let error = result.err().expect("gateway refused");
                assert_eq!(
                    error.to_string(),
                    format!("the gateway at {} {text}", gateway.base())
                );
            }
            None => {
                let status = result.expect("gated");
                assert!(status.auth_required);
                assert!(status.auth_flows.iter().any(|flow| flow == "native_pkce"));
            }
        }
        assert_eq!(gateway.counts().status, 1);
    }
}

#[tokio::test]
async fn insecure_bases_refused_without_request() {
    struct Spy(AtomicUsize);
    impl status::RequestSpy for Spy {
        fn request(&self, _: &Url) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let spy = Spy(AtomicUsize::new(0));
    for text in ["http://gateway.example.com", "http://localhost:1234"] {
        let url = Url::parse(text).unwrap();
        let error = status::probe_with_spy(&url, &spy)
            .await
            .err()
            .expect("insecure base refused");
        assert!(matches!(
            error,
            hermes_client::remote::RemoteError::Insecure { .. }
        ));
        assert_eq!(
            error.to_string(),
            format!("Refusing {url}: a remote must use https:// unless it is 127.0.0.1 or [::1].")
        );
    }
    assert_eq!(spy.0.load(Ordering::SeqCst), 0);
}
