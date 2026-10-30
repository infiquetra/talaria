//! A fresh authenticated attach for each reconnect attempt.
use super::{RefreshError, RemoteConfig, RemoteError, TokenStore};
use crate::liveness::{Dial, DialFailure, Dialed};

pub struct RemoteDial {
    cfg: RemoteConfig,
    store: TokenStore,
    profile: String,
    profile_checked: bool,
    recorder: Option<crate::Recorder>,
    recording_path: Option<std::path::PathBuf>,
    #[cfg(feature = "test-support")]
    root: Option<Vec<u8>>,
}
impl RemoteDial {
    pub fn new(cfg: RemoteConfig, store: TokenStore, profile: String) -> Self {
        Self {
            cfg,
            store,
            profile,
            profile_checked: false,
            recorder: None,
            recording_path: None,
            #[cfg(feature = "test-support")]
            root: None,
        }
    }
    pub fn with_recorder(mut self, recorder: crate::Recorder) -> Self {
        self.recorder = Some(recorder);
        self
    }
    pub fn with_recording(mut self, path: std::path::PathBuf) -> Self {
        self.recording_path = Some(path);
        self
    }
    #[cfg(feature = "test-support")]
    pub fn with_root_certificate(mut self, root_der: &[u8]) -> Self {
        self.root = Some(root_der.to_vec());
        self
    }
    fn root(&self) -> Option<&[u8]> {
        #[cfg(feature = "test-support")]
        {
            self.root.as_deref()
        }
        #[cfg(not(feature = "test-support"))]
        {
            None
        }
    }
    fn failure(&self, error: RemoteError) -> DialFailure {
        let name = self.cfg.name.as_str();
        match error {
            RemoteError::NotSignedIn { .. } => DialFailure::GiveUp {
                reason: format!("not signed in to {name}."),
                next_step: format!("run talaria remote login {name}"),
            },
            RemoteError::Refresh(RefreshError::SessionExpired) => DialFailure::GiveUp {
                reason: format!("sign-in for {name} expired."),
                next_step: format!("run talaria remote login {name}"),
            },
            RemoteError::Refresh(RefreshError::ProviderUnreachable) => DialFailure::Retry {
                reason: format!("the sign-in provider for {name} is unreachable (HTTP 503)."),
                next_step: "try again later".into(),
            },
            RemoteError::RefusedAfterRefresh { .. } => DialFailure::GiveUp {
                reason: error.to_string(),
                next_step: format!("run talaria remote login {name}"),
            },
            RemoteError::Insecure { .. }
            | RemoteError::InvalidUrl
            | RemoteError::Certificate
            | RemoteError::TokenPermissions { .. } => DialFailure::GiveUp {
                reason: error.to_string(),
                next_step: "check the remote address, certificate and token permissions".into(),
            },
            _ => DialFailure::Retry {
                reason: error.to_string(),
                next_step: "try again later".into(),
            },
        }
    }
}
impl Dial for RemoteDial {
    async fn dial(&mut self, generation: u64) -> Result<Dialed, DialFailure> {
        let ticket = super::ws_ticket::mint_with_root(&self.cfg, &self.store, self.root())
            .await
            .map_err(|e| self.failure(e))?;
        if self.recorder.is_none()
            && let Some(path) = self.recording_path.take()
        {
            let url = super::connect::dial_url(&self.cfg, &ticket);
            self.recorder = Some(
                crate::Recorder::create(
                    &path,
                    crate::RecorderOptions {
                        endpoint: url.to_string(),
                        subprotocols: vec!["hermes-gateway-v1".into()],
                        substitutions: self
                            .cfg
                            .base_url
                            .host_str()
                            .map(|host| vec![(host.to_owned(), "gateway.example.com".into())])
                            .unwrap_or_default(),
                        ..Default::default()
                    },
                )
                .map_err(|_| DialFailure::GiveUp {
                    reason: "cannot write the recording file".into(),
                    next_step: "check the recording destination".into(),
                })?,
            );
        }
        let mut connection = super::connect::connect_inner(&self.cfg, ticket, self.root())
            .await
            .map_err(|e| match e {
                crate::ConnectError::UpgradeRejected {
                    status: Some(status),
                } => DialFailure::Retry {
                    reason: super::connect::upgrade_error(&self.cfg.base_url, status),
                    next_step: "check the tunnel and gateway".into(),
                },
                other => other.into(),
            })?;
        if let Some(recorder) = &self.recorder {
            connection.set_recorder(recorder.clone(), generation);
        }
        let handshake = crate::handshake(&mut connection).await.map_err(|e| match e {
            crate::HandshakeError::NotSessionAuthority => DialFailure::GiveUp { reason: "the remote socket has no session authority; point the tunnel at the gateway's own listener".into(), next_step: "point the tunnel at the gateway's own listener".into() },
            other => other.into(),
        })?;
        connection.set_param_injector(super::profile_injector(self.profile.clone()));
        if !self.profile_checked {
            let listed = connection
                .request_raw("session.list", serde_json::json!({"limit":1}))
                .await?;
            if let Some(error) = listed.get("error") {
                if error["code"] == 4001
                    && (error["data"]["reason"] == "profile_mismatch"
                        || error["message"] == "profile_mismatch")
                {
                    return Err(DialFailure::GiveUp {
                        reason: RemoteError::UnservedProfile(self.profile.clone()).to_string(),
                        next_step: "choose a profile served by the remote gateway".into(),
                    });
                }
                return Err(DialFailure::GiveUp {
                    reason: "the remote gateway refused session.list".into(),
                    next_step: "check the remote profile".into(),
                });
            }
            self.profile_checked = true;
        }
        Ok(Dialed {
            parts: connection.into_parts(),
            process_epoch: handshake.ready.replay_epoch,
            heartbeat: handshake.ready.heartbeat == Some(true),
            capabilities: handshake.describe.capabilities,
        })
    }
}
