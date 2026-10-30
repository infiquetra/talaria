//! Native sign-in and serialized, persisted token refresh.
use super::{
    RefreshError, RemoteConfig, RemoteError, SignInError, SignInOptions, TokenSet, TokenStore,
};
use super::{http, loopback, pkce, token_store};
use serde_json::json;
use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::sync::Arc;
use url::Url;

pub async fn exchange(
    cfg: &RemoteConfig,
    code: &str,
    verifier: &str,
) -> Result<TokenSet, SignInError> {
    let result = http::request(
        &cfg.base_url,
        "/auth/native/token",
        "POST",
        None,
        Some(json!({"code":code,"code_verifier":verifier})),
        None,
    )
    .await;
    match result {
        Ok((200, value)) if value["token_type"] == "Bearer" => token_store::decode(&value)
            .ok_or_else(|| SignInError::OutcomeUnknown("invalid response".into())),
        Ok((400, _)) => Err(SignInError::CodeRejected),
        Ok((422, _)) => Err(SignInError::RequestRejected),
        Ok((status, _)) => Err(SignInError::OutcomeUnknown(format!("HTTP {status}"))),
        Err(RemoteError::Other(cause)) if cause == "timeout" => {
            Err(SignInError::OutcomeUnknown("timeout".into()))
        }
        Err(_) => Err(SignInError::OutcomeUnknown("connection failed".into())),
    }
}
pub async fn refresh(cfg: &RemoteConfig, store: &TokenStore) -> Result<TokenSet, RefreshError> {
    let cached = store
        .load()
        .map_err(|_| RefreshError::Other(String::new()))?
        .ok_or(RefreshError::SessionExpired)?;
    refresh_cached(cfg, store, &cached, None).await
}
pub(crate) async fn refresh_cached(
    cfg: &RemoteConfig,
    store: &TokenStore,
    cached: &TokenSet,
    root: Option<&[u8]>,
) -> Result<TokenSet, RefreshError> {
    let _guard = store.refresh_lock.lock().await;
    let current = store
        .load()
        .map_err(|_| RefreshError::Other(String::new()))?
        .ok_or(RefreshError::SessionExpired)?;
    if &current != cached && super::now() < current.expires_at.saturating_sub(60) {
        return Ok(current);
    }
    let (status, value) = http::request(
        &cfg.base_url,
        "/auth/native/refresh",
        "POST",
        None,
        Some(json!({"refresh_token":current.refresh.0,"provider":current.provider})),
        root,
    )
    .await
    .map_err(|_| RefreshError::Other(String::new()))?;
    match status {
        200 if value["token_type"] == "Bearer" => {
            let mut tokens =
                token_store::decode(&value).ok_or_else(|| RefreshError::Other(String::new()))?;
            tokens.saved_at = super::now().max(current.saved_at.saturating_add(1));
            store
                .save(&tokens)
                .map_err(|_| RefreshError::Other(String::new()))?;
            Ok(tokens)
        }
        401 if value["error"] == "session_expired" => {
            store
                .clear()
                .map_err(|_| RefreshError::Other(String::new()))?;
            Err(RefreshError::SessionExpired)
        }
        503 => Err(RefreshError::ProviderUnreachable),
        _ => Err(RefreshError::Other(String::new())),
    }
}
#[cfg(feature = "test-support")]
pub async fn refresh_held(
    cfg: &RemoteConfig,
    store: &TokenStore,
    cached: &TokenSet,
) -> Result<TokenSet, RefreshError> {
    refresh_cached(cfg, store, cached, None).await
}

pub(crate) async fn fresh(
    cfg: &RemoteConfig,
    store: &TokenStore,
    root: Option<&[u8]>,
) -> Result<TokenSet, RemoteError> {
    let tokens = store.load()?.ok_or_else(|| RemoteError::NotSignedIn {
        name: cfg.name.as_str().to_owned(),
    })?;
    if super::now() >= tokens.expires_at.saturating_sub(60) {
        refresh_cached(cfg, store, &tokens, root)
            .await
            .map_err(RemoteError::Refresh)
    } else {
        Ok(tokens)
    }
}

const PASTE_PROMPT: &str =
    "Paste the address your browser was sent to (it expires 120 seconds after the redirect):";
struct EchoGuard {
    fd: OwnedFd,
    previous: rustix::termios::Termios,
}
impl EchoGuard {
    fn new(input: &impl AsFd) -> Result<Self, SignInError> {
        let fd = rustix::io::dup(input).map_err(|_| SignInError::Io(String::new()))?;
        let previous =
            rustix::termios::tcgetattr(&fd).map_err(|_| SignInError::Io(String::new()))?;
        let mut mode = previous.clone();
        mode.local_modes
            .remove(rustix::termios::LocalModes::ECHO | rustix::termios::LocalModes::ECHONL);
        rustix::termios::tcsetattr(&fd, rustix::termios::OptionalActions::Now, &mode)
            .map_err(|_| SignInError::Io(String::new()))?;
        Ok(Self { fd, previous })
    }
}
impl Drop for EchoGuard {
    fn drop(&mut self) {
        let _ = rustix::termios::tcsetattr(
            &self.fd,
            rustix::termios::OptionalActions::Now,
            &self.previous,
        );
    }
}
fn paste_url(line: &[u8], state: &str) -> Result<Url, SignInError> {
    let url = std::str::from_utf8(line)
        .ok()
        .and_then(|s| Url::parse(s.trim()).ok())
        .ok_or_else(|| SignInError::Denied("invalid_request".into()))?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("127.0.0.1" | "[::1]"))
        || url.path() != "/callback"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(SignInError::Denied("invalid_request".into()));
    }
    loopback::parse_callback(&url, state)?;
    Ok(url)
}
/// Read a callback address with echo disabled and restore the terminal on every return.
pub fn read_paste<R: Read + AsFd, W: Write>(
    input: &mut R,
    prompt: &mut W,
    state: &str,
) -> Result<Url, SignInError> {
    let _guard = EchoGuard::new(input)?;
    writeln!(prompt, "{PASTE_PROMPT}")
        .and_then(|_| prompt.flush())
        .map_err(|_| SignInError::Io(String::new()))?;
    let mut line = zeroize::Zeroizing::new(Vec::new());
    for _ in 0..8192 {
        let mut byte = [0];
        if input
            .read(&mut byte)
            .map_err(|_| SignInError::Io(String::new()))?
            == 0
            || byte[0] == b'\n'
        {
            return paste_url(&line, state);
        }
        line.push(byte[0]);
    }
    Err(SignInError::Denied("invalid_request".into()))
}
async fn paste(state: &str) -> Result<String, SignInError> {
    use std::os::unix::fs::OpenOptionsExt;
    let tty = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
        .open("/dev/tty")
        .map_err(|_| SignInError::Io(String::new()))?;
    let _echo = EchoGuard::new(&tty)?;
    eprintln!("{PASTE_PROMPT}");
    let mut line = zeroize::Zeroizing::new(Vec::new());
    while line.len() < 8192 {
        let mut byte = [0];
        match rustix::io::read(&tty, &mut byte) {
            Ok(0) => break,
            Ok(_) if byte[0] == b'\n' => {
                return loopback::parse_callback(&paste_url(&line, state)?, state);
            }
            Ok(_) => line.push(byte[0]),
            Err(rustix::io::Errno::AGAIN) => {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await
            }
            Err(_) => return Err(SignInError::Io(String::new())),
        }
    }
    Err(SignInError::Denied("invalid_request".into()))
}
type BrowserFuture = std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>>;
fn open_browser(url: &Url) -> BrowserFuture {
    let url = url.clone();
    Box::pin(async move {
        let command = std::env::var_os("BROWSER")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if cfg!(target_os = "macos") {
                    "open".into()
                } else {
                    "xdg-open".into()
                }
            });
        tokio::process::Command::new(command)
            .arg(url.as_str())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .is_ok_and(|status| status.success())
    })
}
struct LoginIo {
    browser: Arc<dyn Fn(&Url) -> BrowserFuture + Send + Sync>,
    display: Arc<dyn Fn(&Url) + Send + Sync>,
    authorize: Option<tokio::sync::mpsc::UnboundedSender<Url>>,
}
async fn login(
    cfg: &RemoteConfig,
    opts: SignInOptions,
    io: LoginIo,
) -> Result<TokenSet, SignInError> {
    super::probe_status(&cfg.base_url)
        .await
        .map_err(|e| match e {
            RemoteError::Insecure { .. } => SignInError::Insecure,
            RemoteError::NotGated { .. } => SignInError::NotGated,
            RemoteError::NoNativeFlow { .. } => SignInError::NoNativeFlow,
            _ => SignInError::Io(String::new()),
        })?;
    let listener = loopback::CallbackListener::bind()
        .await
        .map_err(|_| SignInError::Io(String::new()))?;
    let pair = pkce::try_generate()?;
    let url = pkce::authorize_url(
        &cfg.base_url,
        &pair.challenge,
        &listener.redirect_uri(),
        &pair.state,
        cfg.provider.as_deref(),
    );
    let mut browser_waiting = !opts.no_browser && !opts.paste;
    if !browser_waiting {
        (io.display)(&url);
    }
    if let Some(sender) = io.authorize {
        let _ = sender.send(url.clone());
    }
    let should_open = browser_waiting;
    let browser = async {
        if should_open {
            (io.browser)(&url).await
        } else {
            std::future::pending().await
        }
    };
    let callback = listener.wait(&pair.state, std::time::Duration::from_secs(600));
    let pasted = async {
        if opts.paste {
            paste(&pair.state).await
        } else {
            std::future::pending().await
        }
    };
    tokio::pin!(browser, callback, pasted);
    let code = loop {
        tokio::select! {
            biased;
            _ = opts.cancel.cancelled() => return Err(SignInError::Cancelled),
            _ = tokio::signal::ctrl_c() => return Err(SignInError::Cancelled),
            result = &mut callback => break result?,
            result = &mut pasted => break result?,
            opened = &mut browser, if browser_waiting => {
                browser_waiting = false;
                if !opened { (io.display)(&url); }
            }
        }
    };
    let code = zeroize::Zeroizing::new(code);
    exchange(cfg, &code, &pair.verifier).await
}
pub async fn sign_in(cfg: &RemoteConfig, opts: SignInOptions) -> Result<TokenSet, SignInError> {
    login(
        cfg,
        opts,
        LoginIo {
            browser: Arc::new(open_browser),
            display: Arc::new(|url| eprintln!("Open this URL in a browser to sign in:\n{url}")),
            authorize: None,
        },
    )
    .await
}
#[cfg(feature = "test-support")]
pub struct SignInIo {
    pub stdout: Arc<std::sync::Mutex<Vec<u8>>>,
    pub stderr: Arc<std::sync::Mutex<Vec<u8>>>,
    pub browser: Arc<dyn Fn(&Url) + Send + Sync>,
    pub authorize: tokio::sync::mpsc::UnboundedSender<Url>,
}
#[cfg(feature = "test-support")]
pub async fn sign_in_with_io(
    cfg: &RemoteConfig,
    opts: SignInOptions,
    io: SignInIo,
) -> Result<TokenSet, SignInError> {
    login(
        cfg,
        opts,
        LoginIo {
            browser: Arc::new(move |url| {
                let browser = Arc::clone(&io.browser);
                let url = url.clone();
                Box::pin(async move {
                    browser(&url);
                    true
                })
            }),
            display: Arc::new(move |url| {
                let _ = writeln!(
                    io.stderr
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner),
                    "Open this URL in a browser to sign in:\n{url}"
                );
            }),
            authorize: Some(io.authorize),
        },
    )
    .await
}

#[derive(Debug, serde::Deserialize)]
pub struct RemoteIdentity {
    pub user_id: String,
    pub display_name: String,
    pub provider: String,
    pub expires_at: i64,
}
pub async fn identity(
    cfg: &RemoteConfig,
    store: &TokenStore,
) -> Result<RemoteIdentity, RemoteError> {
    let tokens = fresh(cfg, store, None).await?;
    let (status, value) = http::request(
        &cfg.base_url,
        "/api/auth/me",
        "GET",
        Some(&tokens.access),
        None,
        None,
    )
    .await?;
    if status != 200 {
        return Err(RemoteError::Http(status));
    }
    serde_json::from_value(value).map_err(|_| RemoteError::Other(String::new()))
}

/// Check saved credentials before starting an interactive screen, without opening a browser.
pub async fn ensure_signed_in(cfg: &RemoteConfig, store: &TokenStore) -> Result<(), RemoteError> {
    fresh(cfg, store, None).await.map(|_| ())
}
