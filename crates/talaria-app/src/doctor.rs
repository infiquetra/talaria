use hermes_client::{DiscoverOptions, EnsurePolicy};
use std::path::PathBuf;
use talaria_core::doctor::{
    AttachFacts, DoctorFacts, GatewayFacts, Pin, PostResult, ProbeResult, StatusFacts, WorkSummary,
};

pub struct DoctorService {
    pub home: PathBuf,
    pub options: DiscoverOptions,
}
impl DoctorService {
    pub fn new(home: PathBuf) -> Self {
        Self {
            home,
            options: DiscoverOptions {
                ensure: EnsurePolicy::Never,
                ..DiscoverOptions::default()
            },
        }
    }
    pub async fn gather(&self, profile: &str) -> DoctorFacts {
        DoctorFacts {
            talaria_version: env!("CARGO_PKG_VERSION").to_owned(),
            pin: Pin {
                tag: hermes_contract::pinned::TAG.to_owned(),
                commit: hermes_contract::pinned::COMMIT.to_owned(),
            },
            profile: profile.to_owned(),
            gateway: GatewayFacts::NotReady {
                state: "stub".to_owned(),
                drain_reason: None,
            },
            identity: None,
            attach: AttachFacts::NotTried,
            status: StatusFacts::NotTried,
        }
    }
    pub async fn active_work(&self) -> Result<WorkSummary, ()> {
        Err(())
    }
    pub async fn post_restart(&self, _old_instance: &str, _selected_profile: &str) -> PostResult {
        PostResult::Changed
    }
    pub async fn probe(&self) -> ProbeResult {
        ProbeResult::NotAnswering
    }
}
