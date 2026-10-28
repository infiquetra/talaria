//! Scripted, profile-scoped gateway conversations.
#![allow(dead_code)]
use super::wire::{Cursor, Wire, WireError, WireEvent, WireFactory};
use futures::future::BoxFuture;
use serde_json::{Map, Value};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub struct OutboundFrame {
    pub profile: String,
    pub method: String,
    pub params: Map<String, Value>,
}

#[derive(Clone, Debug)]
pub struct TrackRecord {
    pub profile: String,
    pub session_id: String,
    pub cursor: Cursor,
    pub resync: bool,
}

pub struct ScriptedWire {
    outbound: Mutex<Vec<OutboundFrame>>,
    tracks: Mutex<Vec<TrackRecord>>,
}

impl ScriptedWire {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|_| "script is unreadable".to_string())?;
        let script: Value =
            serde_json::from_str(&text).map_err(|_| "script is not JSON".to_string())?;
        let parent = path.parent().unwrap_or(path);
        Self::from_value(parent, script)
    }

    pub fn from_file(path: &Path) -> Result<Self, String> {
        Self::load(path)
    }

    pub fn from_value(_root: &Path, _script: Value) -> Result<Self, String> {
        Ok(Self {
            outbound: Mutex::new(Vec::new()),
            tracks: Mutex::new(Vec::new()),
        })
    }

    pub fn outbound(&self) -> Vec<OutboundFrame> {
        self.outbound
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    pub fn log(&self) -> Vec<OutboundFrame> {
        self.outbound()
    }

    pub fn tracks(&self) -> Vec<TrackRecord> {
        self.tracks
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    pub fn emit(&self, profile: &str, event: WireEvent) {
        let _ = (profile, event);
    }
}

struct ProfileWire;

impl Wire for ProfileWire {
    fn call(
        &self,
        method: &'static str,
        params: Map<String, Value>,
        timeout: Duration,
    ) -> BoxFuture<'static, Result<Value, WireError>> {
        let _ = (method, params, timeout);
        todo!("hermes binding stub")
    }

    fn generation(&self) -> u64 {
        0
    }

    fn track(&self, session_id: &str, cursor: Cursor) {
        let _ = (session_id, cursor);
    }

    fn resync(&self, session_id: &str, cursor: Cursor) {
        let _ = (session_id, cursor);
    }

    fn take_events(&self) -> Option<mpsc::Receiver<WireEvent>> {
        None
    }
}

impl WireFactory for ScriptedWire {
    fn open(&self, profile: &str) -> BoxFuture<'static, Result<Arc<dyn Wire>, WireError>> {
        let _ = (self, profile);
        todo!("hermes binding stub")
    }
}
