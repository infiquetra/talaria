//! Remote command parsing and the remotes configuration file.
use clap::{Parser, Subcommand};
use hermes_client::remote::{RemoteConfig, RemoteError, RemoteName, TokenStore};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct Remotes {
    pub entries: Vec<RemoteConfig>,
}
#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    remotes: BTreeMap<String, Entry>,
}
#[derive(Serialize, Deserialize)]
struct Entry {
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
}
impl Remotes {
    pub fn load(home: &Path) -> Result<Self, String> {
        let text = match std::fs::read_to_string(home.join("remotes.toml")) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    entries: Vec::new(),
                });
            }
            Err(_) => return Err("could not read remotes.toml".into()),
        };
        let file: File = toml::from_str(&text).map_err(|_| "could not parse remotes.toml")?;
        let entries = file
            .remotes
            .into_iter()
            .map(|(name, entry)| {
                let name = RemoteName::parse(&name).map_err(|e| e.to_string())?;
                let base_url = base_url(&entry.url)?;
                Ok(RemoteConfig {
                    name,
                    base_url,
                    provider: entry.provider,
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(Self { entries })
    }
    pub fn save(&self, home: &Path) -> Result<(), String> {
        std::fs::create_dir_all(home).map_err(|_| "could not create the configuration folder")?;
        let file = File {
            remotes: self
                .entries
                .iter()
                .map(|cfg| {
                    (
                        cfg.name.as_str().to_owned(),
                        Entry {
                            url: cfg.base_url.as_str().trim_end_matches('/').to_owned(),
                            provider: cfg.provider.clone(),
                        },
                    )
                })
                .collect(),
        };
        let text = toml::to_string(&file).map_err(|_| "could not encode remotes.toml")?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(home).map_err(|_| "could not write remotes.toml")?;
        temporary
            .write_all(text.as_bytes())
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|_| "could not write remotes.toml")?;
        temporary
            .persist(home.join("remotes.toml"))
            .map_err(|_| "could not replace remotes.toml")?;
        Ok(())
    }
    pub fn add(&mut self, config: RemoteConfig) -> Result<(), String> {
        if self.entries.iter().any(|c| c.name == config.name) {
            return Err(
                "a remote with that name already exists; remove it before changing its address"
                    .into(),
            );
        }
        self.entries.push(config);
        self.entries
            .sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        Ok(())
    }
    pub fn remove(&mut self, name: &str) -> Result<(), String> {
        let index = self
            .entries
            .iter()
            .position(|c| c.name.as_str() == name)
            .ok_or("no remote with that name")?;
        self.entries.remove(index);
        Ok(())
    }
    pub fn get(&self, name: &str) -> Result<RemoteConfig, String> {
        self.entries
            .iter()
            .find(|c| c.name.as_str() == name)
            .cloned()
            .ok_or_else(|| "no remote with that name".into())
    }
}
/// Use the same configuration root as the workflow staffing configuration.
pub fn talaria_home() -> Result<PathBuf, String> {
    std::env::var_os("TALARIA_CONFIG_DIR")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|s| !s.is_empty())
                .map(|h| PathBuf::from(h).join(".talaria"))
        })
        .ok_or_else(|| "could not find the configuration folder".into())
}
fn base_url(text: &str) -> Result<url::Url, String> {
    let mut url = url::Url::parse(text).map_err(|_| "invalid remote URL")?;
    hermes_client::remote::validate_base(&url).map_err(|e| e.to_string())?;
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(&path);
    Ok(url)
}
#[derive(Parser)]
#[command(name = "talaria remote")]
struct Arguments {
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    Add {
        name: String,
        url: String,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        no_check: bool,
    },
    List,
    Remove {
        name: String,
    },
    Login {
        name: String,
        #[arg(long)]
        no_browser: bool,
        #[arg(long)]
        paste: bool,
    },
    Logout {
        name: String,
    },
    Status {
        name: String,
    },
}
impl Action {
    fn name(&self) -> Option<&str> {
        match self {
            Self::List => None,
            Self::Add { name, .. }
            | Self::Remove { name }
            | Self::Login { name, .. }
            | Self::Logout { name }
            | Self::Status { name } => Some(name),
        }
    }
}
fn expiry(expires: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let seconds = expires.saturating_sub(now);
    if seconds <= 0 {
        "expired".into()
    } else if seconds < 7200 {
        format!("in {} minutes", seconds / 60)
    } else {
        format!("in {} hours", seconds / 3600)
    }
}
fn visible(text: &str) -> String {
    text.chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        .take(256)
        .collect()
}
fn auth_failure(error: RemoteError, name: &str) -> String {
    match error {
        RemoteError::Refresh(hermes_client::remote::RefreshError::SessionExpired) => {
            format!("sign-in for {name} expired.\nNext step: run talaria remote login {name}")
        }
        RemoteError::Refresh(hermes_client::remote::RefreshError::ProviderUnreachable) => format!(
            "the sign-in provider for {name} is unreachable (HTTP 503).\nNext step: try again later"
        ),
        RemoteError::NotSignedIn { .. } | RemoteError::RefusedAfterRefresh { .. } => {
            format!("{error}\nNext step: run talaria remote login {name}")
        }
        _ => format!("{error}\nNext step: check the remote gateway"),
    }
}
async fn execute(action: Action, home: &Path) -> Result<(), String> {
    let mut remotes = Remotes::load(home)?;
    match action {
        Action::Add {
            name,
            url,
            provider,
            no_check,
        } => {
            let config = RemoteConfig {
                name: RemoteName::parse(&name).map_err(|e| e.to_string())?,
                base_url: base_url(&url)?,
                provider,
            };
            if !no_check {
                hermes_client::remote::probe_status(&config.base_url)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            remotes.add(config)?;
            remotes.save(home)?;
        }
        Action::List => {
            println!("NAME  URL  STATE  EXPIRES");
            for cfg in remotes.entries {
                let tokens = TokenStore::new(home, cfg.name.clone())
                    .load()
                    .map_err(|e| e.to_string())?;
                let (state, expires) = match tokens {
                    Some(tokens) => ("signed in", expiry(tokens.expires_at)),
                    None => ("signed out", "-".into()),
                };
                println!(
                    "{}  {}  {state}  {expires}",
                    cfg.name.as_str(),
                    cfg.base_url.as_str().trim_end_matches('/')
                );
            }
        }
        Action::Remove { name } => {
            let cfg = remotes.get(&name)?;
            TokenStore::new(home, cfg.name)
                .clear()
                .map_err(|e| e.to_string())?;
            remotes.remove(&name)?;
            remotes.save(home)?;
        }
        Action::Logout { name } => {
            let cfg = remotes.get(&name)?;
            TokenStore::new(home, cfg.name)
                .clear()
                .map_err(|e| e.to_string())?;
        }
        Action::Login {
            name,
            no_browser,
            paste,
        } => {
            let cfg = remotes.get(&name)?;
            let opts = hermes_client::remote::SignInOptions {
                no_browser,
                paste,
                cancel: tokio_util::sync::CancellationToken::new(),
            };
            let tokens = hermes_client::remote::sign_in(&cfg, opts)
                .await
                .map_err(|e| e.to_string().replace("<name>", &name))?;
            TokenStore::new(home, cfg.name)
                .save(&tokens)
                .map_err(|e| e.to_string())?;
            println!("Signed in to {name} as {}", visible(&tokens.user_id));
        }
        Action::Status { name } => {
            let cfg = remotes.get(&name)?;
            let store = TokenStore::new(home, cfg.name.clone());
            let user = hermes_client::remote::native_auth::identity(&cfg, &store)
                .await
                .map_err(|e| auth_failure(e, &name))?;
            println!(
                "user: {}\nname: {}\nprovider: {}\nexpires: {}",
                visible(&user.user_id),
                visible(&user.display_name),
                visible(&user.provider),
                expiry(user.expires_at)
            );
        }
    }
    Ok(())
}
pub fn run(args: &[String]) -> u8 {
    let parsed = match Arguments::try_parse_from(
        std::iter::once("talaria remote".to_owned()).chain(args.iter().skip(1).cloned()),
    ) {
        Ok(a) => a,
        Err(e) => {
            let code = if e.use_stderr() { 2 } else { 0 };
            let _ = e.print();
            return code;
        }
    };
    if let Some(name) = parsed.command.name()
        && let Err(e) = RemoteName::parse(name)
    {
        eprintln!("talaria: {e}");
        return 2;
    }
    if let Action::Add { url, .. } = &parsed.command {
        let parsed_url = match url::Url::parse(url) {
            Ok(url) => url,
            Err(_) => {
                eprintln!("talaria: invalid remote URL");
                return 2;
            }
        };
        if let Err(e) = hermes_client::remote::validate_base(&parsed_url) {
            let code = if matches!(e, RemoteError::Insecure { .. }) {
                1
            } else {
                2
            };
            eprintln!("talaria: {e}");
            return code;
        }
    }
    let result = talaria_home().and_then(|home| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|_| "could not start the runtime".into())
            .and_then(|rt| rt.block_on(execute(parsed.command, &home)))
    });
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("talaria: {e}");
            1
        }
    }
}
pub fn attach(cli: crate::args::Cli) -> std::process::ExitCode {
    let name = cli.remote.as_deref().unwrap_or_default();
    if let Err(e) = RemoteName::parse(name) {
        eprintln!("talaria: {e}");
        return std::process::ExitCode::from(2);
    }
    let loaded = talaria_home().and_then(|home| {
        let cfg = Remotes::load(&home)?.get(name)?;
        let store = TokenStore::new(&home, cfg.name.clone());
        if store.load().map_err(|e| e.to_string())?.is_none() {
            return Err(auth_failure(
                RemoteError::NotSignedIn { name: name.into() },
                name,
            ));
        }
        Ok((cfg, store))
    });
    let (cfg, store) = match loaded {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("talaria: {e}");
            return std::process::ExitCode::from(1);
        }
    };
    if cli.print || cli.bg || cli.worktree || cli.name.is_some() {
        eprintln!(
            "talaria: --remote opens an interactive session; print and background commands are not supported"
        );
        return std::process::ExitCode::from(2);
    }
    let checked = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|_| "could not start the runtime".to_owned())
        .and_then(|runtime| {
            runtime
                .block_on(hermes_client::remote::native_auth::ensure_signed_in(
                    &cfg, &store,
                ))
                .map_err(|error| auth_failure(error, cfg.name.as_str()))
        });
    if let Err(error) = checked {
        eprintln!("talaria: {error}");
        return std::process::ExitCode::from(1);
    }
    if let Err(error) = crate::args::validate_print_options(&cli) {
        eprintln!("talaria: {}\nNext step: {}", error.what, error.next_step);
        return std::process::ExitCode::from(2);
    }
    let ultracode = cli.ultracode;
    match cli.into_launch() {
        Ok(launch) => crate::run::run_remote(launch, ultracode, cfg, store),
        Err(e) => {
            let failure = talaria_app::failure::LaunchFailure::from(e);
            eprint!("{}", failure.render());
            std::process::ExitCode::from(failure.exit_code)
        }
    }
}
