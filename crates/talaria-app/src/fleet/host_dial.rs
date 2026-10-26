//! A host-scoped dial, with a profile-only fallback when the gateway refuses host scope.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use hermes_client::liveness::{Dial, DialFailure, Dialed};
use hermes_client::{
    DiscoverOptions, EnsurePolicy, Purpose, Scope, TicketError, connect, discover_with, handshake,
    mint_ticket,
};

use crate::side_connection::SideScope;

/// Why the fallback was taken. Shown beside a profile-only listing.
pub(crate) const HOST_REFUSED: &str = "the gateway refused a host-scoped ticket";

/// What the latest successful dial learned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DialReport {
    /// Generation the dial was made for.
    pub generation: u64,
    /// The scope the ticket carried.
    pub scope: SideScope,
    /// Served profile names, in served order.
    pub served: Vec<String>,
    /// Parked profiles and the gateway's reason for each.
    pub parked: Vec<(String, String)>,
}

/// Home, foreground profile, and the scope the latest dial was granted.
pub struct HostDial {
    home: PathBuf,
    foreground_profile: String,
    report: Arc<Mutex<Option<DialReport>>>,
}

impl HostDial {
    /// Start on host scope for the foreground profile.
    #[must_use]
    pub fn new(home: PathBuf, foreground_profile: String) -> Self {
        Self {
            home,
            foreground_profile,
            report: Arc::new(Mutex::new(None)),
        }
    }

    /// The scope of the latest dial; host scope before any dial. Every dial asks for host
    /// scope first, so a profile-only fallback lasts for one connection.
    #[must_use]
    pub fn scope(&self) -> SideScope {
        match lock(&self.report).as_ref() {
            Some(report) => report.scope.clone(),
            None => SideScope::Host {
                profiles: vec![self.foreground_profile.clone()],
            },
        }
    }

    /// Where the service reads what each dial learned.
    pub(crate) fn reports(&self) -> Arc<Mutex<Option<DialReport>>> {
        Arc::clone(&self.report)
    }

    async fn attach(&self, generation: u64) -> Result<Dialed, DialFailure> {
        let options = DiscoverOptions {
            ensure: EnsurePolicy::Never,
            ..DiscoverOptions::default()
        };
        let endpoint = discover_with(&self.home, &options).await?;
        let profile = endpoint.resolve_profile(&self.foreground_profile)?;
        // Host scope with the interactive purpose always builds.
        let host = Scope::host(Purpose::Interactive).map_err(|_| DialFailure::GiveUp {
            reason: "host scope is not available".into(),
            next_step: "Update talaria.".into(),
        })?;
        let served: Vec<String> = endpoint
            .profiles()
            .iter()
            .map(|served| served.name.clone())
            .collect();
        let (ticket, scope) =
            match mint_ticket(&self.home, &endpoint, &profile, Purpose::Interactive, host).await {
                Ok(ticket) => (
                    ticket,
                    SideScope::Host {
                        profiles: served.clone(),
                    },
                ),
                Err(TicketError::BindingRejected | TicketError::Refused(_)) => {
                    let ticket = mint_ticket(
                        &self.home,
                        &endpoint,
                        &profile,
                        Purpose::Interactive,
                        Scope::profile(),
                    )
                    .await?;
                    (
                        ticket,
                        SideScope::ProfileOnly {
                            profile: self.foreground_profile.clone(),
                            why: HOST_REFUSED.into(),
                        },
                    )
                }
                Err(other) => return Err(other.into()),
            };
        let mut connection = connect(&endpoint, ticket).await?;
        let handshake = handshake(&mut connection).await?;
        *lock(&self.report) = Some(DialReport {
            generation,
            scope,
            served,
            parked: endpoint.parked().to_vec(),
        });
        Ok(Dialed {
            parts: connection.into_parts(),
            process_epoch: handshake.ready.replay_epoch,
            heartbeat: handshake.ready.heartbeat == Some(true),
            capabilities: handshake.describe.capabilities,
        })
    }
}

impl Dial for HostDial {
    async fn dial(&mut self, generation: u64) -> Result<Dialed, DialFailure> {
        self.attach(generation).await
    }
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
