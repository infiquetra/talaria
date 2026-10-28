use super::{Roster, RosterError};
use crate::hermes::{Wire, WireError, WireFactory};
use chrono::{DateTime, Utc};
use futures_util::future::BoxFuture;
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Mutex as AsyncMutex;
pub trait RosterSource: Send + Sync + 'static {
    fn fetch(&self, profile: &str) -> BoxFuture<'static, Result<Value, RosterError>>;
}
pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> DateTime<Utc>;
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
#[derive(Clone)]
pub struct ManualClock(Arc<Mutex<DateTime<Utc>>>);
impl ManualClock {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self(Arc::new(Mutex::new(now)))
    }
    pub fn set(&self, now: DateTime<Utc>) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = now;
    }
    pub fn advance(&self, by: chrono::Duration) {
        let mut t = self.0.lock().unwrap_or_else(|e| e.into_inner());
        *t += by;
    }
}
impl Clock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}
#[derive(Clone)]
pub struct LinkRosterSource {
    wires: Arc<dyn WireFactory>,
    profiles: Arc<AsyncMutex<BTreeMap<String, Arc<dyn Wire>>>>,
}
impl LinkRosterSource {
    pub fn new(wires: Arc<dyn WireFactory>) -> Self {
        Self {
            wires,
            profiles: Arc::new(AsyncMutex::new(BTreeMap::new())),
        }
    }
}
fn wire_error(profile: &str, e: WireError) -> RosterError {
    match e {
        WireError::Refused { code, message, .. } => RosterError::Rpc {
            code,
            message: if code == 5098 {
                "the Hermes gateway is running older code than is installed; the operator restarts it".into()
            } else {
                message
            },
        },
        WireError::NotServed(_) => RosterError::NotServed(profile.into()),
        WireError::Timeout => RosterError::Timeout,
        e => RosterError::Transport(e.to_string()),
    }
}
impl RosterSource for LinkRosterSource {
    fn fetch(&self, profile: &str) -> BoxFuture<'static, Result<Value, RosterError>> {
        let this = self.clone();
        let profile = profile.to_owned();
        Box::pin(async move {
            let wire = {
                let mut map = this.profiles.lock().await;
                if let Some(w) = map.get(&profile) {
                    w.clone()
                } else {
                    let w =
                        tokio::time::timeout(Duration::from_secs(15), this.wires.open(&profile))
                            .await
                            .map_err(|_| RosterError::Timeout)?
                            .map_err(|e| wire_error(&profile, e))?;
                    map.insert(profile.clone(), w.clone());
                    w
                }
            };
            tokio::time::timeout(
                Duration::from_secs(15),
                wire.call("model.options", Map::new(), Duration::from_secs(15)),
            )
            .await
            .map_err(|_| RosterError::Timeout)?
            .map_err(|e| wire_error(&profile, e))
        })
    }
}
type FetchLog = Arc<Mutex<Vec<(String, DateTime<Utc>)>>>;
type Answers = BTreeMap<String, VecDeque<Result<Value, RosterError>>>;
#[derive(Clone)]
pub struct FixtureRosterSource {
    answers: Arc<Mutex<Answers>>,
    fetches: FetchLog,
    clock: Arc<dyn Clock>,
}
impl Default for FixtureRosterSource {
    fn default() -> Self {
        Self::new()
    }
}
impl FixtureRosterSource {
    pub fn new() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }
    pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self {
            answers: Arc::new(Mutex::new(BTreeMap::new())),
            fetches: Arc::new(Mutex::new(vec![])),
            clock,
        }
    }
    pub fn push(&self, profile: &str, answer: Result<Value, RosterError>) {
        self.answers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(profile.into())
            .or_default()
            .push_back(answer);
    }
    pub fn fetches(&self) -> Vec<(String, DateTime<Utc>)> {
        self.fetches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}
impl RosterSource for FixtureRosterSource {
    fn fetch(&self, profile: &str) -> BoxFuture<'static, Result<Value, RosterError>> {
        self.fetches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((profile.into(), self.clock.now()));
        let answer = self
            .answers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(profile)
            .and_then(VecDeque::pop_front)
            .unwrap_or_else(|| Err(RosterError::NotServed(profile.into())));
        Box::pin(async move { answer })
    }
}
#[derive(Clone)]
pub struct RosterCache {
    inner: Arc<CacheInner>,
}
struct CacheInner {
    source: Arc<dyn RosterSource>,
    clock: Arc<dyn Clock>,
    interval: Duration,
    entries: Mutex<BTreeMap<String, Arc<Roster>>>,
    attempts: Mutex<BTreeMap<String, DateTime<Utc>>>,
    read: AsyncMutex<()>,
}
impl RosterCache {
    pub fn new(
        source: Arc<dyn RosterSource>,
        clock: Arc<dyn Clock>,
        min_interval: Duration,
    ) -> Self {
        Self {
            inner: Arc::new(CacheInner {
                source,
                clock,
                interval: min_interval,
                entries: Mutex::new(BTreeMap::new()),
                attempts: Mutex::new(BTreeMap::new()),
                read: AsyncMutex::new(()),
            }),
        }
    }
    pub async fn load(&self, profile: &str) -> Result<Arc<Roster>, RosterError> {
        let _read = self.inner.read.lock().await;
        self.read(profile).await
    }
    async fn read(&self, profile: &str) -> Result<Arc<Roster>, RosterError> {
        let now = self.inner.clock.now();
        self.inner
            .attempts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(profile.into(), now);
        let v = self.inner.source.fetch(profile).await?;
        let r = Arc::new(Roster::from_model_options(&v, profile, now)?);
        self.inner
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(profile.into(), r.clone());
        Ok(r)
    }
    pub async fn get_or_load(&self, profile: &str) -> Result<Arc<Roster>, RosterError> {
        let _read = self.inner.read.lock().await;
        if let Some(r) = self.cached(profile) {
            Ok(r)
        } else {
            self.read(profile).await
        }
    }
    pub async fn refresh(&self, profile: &str, force: bool) -> Result<Arc<Roster>, RosterError> {
        let _read = self.inner.read.lock().await;
        let due = self
            .inner
            .attempts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(profile)
            .is_none_or(|t| {
                (self.inner.clock.now() - *t).to_std().unwrap_or_default() >= self.inner.interval
            });
        if !force
            && !due
            && let Some(r) = self.cached(profile)
        {
            return Ok(r);
        }
        self.read(profile).await
    }
    pub fn cached(&self, profile: &str) -> Option<Arc<Roster>> {
        self.inner
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(profile)
            .cloned()
    }
}
