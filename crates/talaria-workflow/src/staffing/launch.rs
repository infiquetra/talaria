use super::StaffingResolver;
use crate::{
    engine::AgentRequest,
    hermes::{ExplicitLaunch, HermesPort, SchemaTurnPolicy},
    journal::{JournaledBackend, LaunchResolver, ResolvedLaunch, RetryPolicy, RunJournal},
    schedule::Scheduler,
};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};
pub struct StaffedLaunch {
    resolver: StaffingResolver,
    explicit: ExplicitLaunch,
    warned: Mutex<BTreeSet<String>>,
}
impl StaffedLaunch {
    pub fn new(resolver: StaffingResolver, explicit: ExplicitLaunch) -> Self {
        Self {
            resolver,
            explicit,
            warned: Mutex::new(BTreeSet::new()),
        }
    }
}
impl LaunchResolver for StaffedLaunch {
    fn resolve(&self, request: &AgentRequest) -> Result<ResolvedLaunch, String> {
        let r = self.resolver.route(request).map_err(|e| e.0)?;
        if let Some(t) = r.ignored_tier
            && self
                .warned
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(t.clone())
        {
            tracing::warn!("tier '{t}' ignored: the call names a model");
        }
        self.explicit
            .complete(request, r.model, r.provider, r.effort)
    }
}
pub fn staffed_backend(
    journal: Arc<RunJournal>,
    port: HermesPort,
    launch: StaffedLaunch,
    scheduler: Scheduler,
    retry: RetryPolicy,
) -> JournaledBackend<HermesPort, SchemaTurnPolicy, StaffedLaunch, Scheduler> {
    JournaledBackend::new(journal, port, SchemaTurnPolicy, launch, scheduler, retry)
}
