//! A short-lived link used by one fleet launch. No reconnect.

use std::path::Path;

use hermes_client::{
    Client, DiscoverOptions, EnsurePolicy, Inbound, InboundStream, Purpose, Scope, TicketError,
    connect, discover_with, handshake, mint_ticket,
};
use tokio::task::JoinHandle;

use crate::bg::machine::DescribeSummary;
use crate::bg::outcome::LaunchError;
use crate::failure::LaunchFailure;

/// What the link was allowed to reach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkScope {
    /// A host ticket, listing the profiles the gateway served.
    Host { profiles: Vec<String> },
    /// A profile ticket, after the host ticket was refused.
    ProfileOnly { profile: String },
}

/// One connection opened for a single launch.
pub struct LaunchLink {
    scope: LinkScope,
    profile: String,
    client: Client,
    describe: DescribeSummary,
    drain: JoinHandle<()>,
}

impl std::fmt::Debug for LaunchLink {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LaunchLink")
            .field("scope", &self.scope)
            .field("profile", &self.profile)
            .finish_non_exhaustive()
    }
}

impl LaunchLink {
    /// Open a link against the Hermes home this process uses, never starting a gateway.
    ///
    /// The ticket is host scoped; when the gateway refuses that binding, a profile ticket for
    /// `foreground_profile` is minted instead and the scope says so.
    pub async fn open(home: &Path, foreground_profile: &str) -> Result<Self, LaunchError> {
        Self::dial(home, foreground_profile)
            .await
            .map_err(|failure| LaunchError {
                what: failure.what,
                next_step: failure.next_step,
            })
    }

    async fn dial(home: &Path, foreground_profile: &str) -> Result<Self, LaunchFailure> {
        let options = DiscoverOptions {
            ensure: EnsurePolicy::Never,
            ..DiscoverOptions::default()
        };
        let endpoint = discover_with(home, &options).await?;
        let profile = endpoint.resolve_profile(foreground_profile)?;
        let served: Vec<String> = endpoint
            .profiles()
            .iter()
            .map(|served| served.name.clone())
            .collect();
        // Host scope with the interactive purpose always builds.
        let host = Scope::host(Purpose::Interactive).map_err(|_| LaunchFailure {
            what: "host scope is not available".into(),
            next_step: "Update talaria.".into(),
            exit_code: 4,
        })?;
        let (ticket, scope) =
            match mint_ticket(home, &endpoint, &profile, Purpose::Interactive, host).await {
                Ok(ticket) => (ticket, LinkScope::Host { profiles: served }),
                Err(TicketError::BindingRejected | TicketError::Refused(_)) => {
                    let ticket = mint_ticket(
                        home,
                        &endpoint,
                        &profile,
                        Purpose::Interactive,
                        Scope::profile(),
                    )
                    .await?;
                    let scope = LinkScope::ProfileOnly {
                        profile: foreground_profile.to_owned(),
                    };
                    (ticket, scope)
                }
                Err(other) => return Err(other.into()),
            };
        let mut connection = connect(&endpoint, ticket).await?;
        let handshake = handshake(&mut connection).await?;
        let describe = DescribeSummary {
            sources: handshake.describe.session_create.sources.clone(),
            parameters: handshake.describe.session_create.parameters.clone(),
        };
        let (client, inbound) = Client::start(connection.into_parts());
        Ok(Self {
            scope,
            profile: foreground_profile.to_owned(),
            client,
            describe,
            drain: tokio::spawn(drain(inbound)),
        })
    }

    /// The scope the ticket was minted for.
    #[must_use]
    pub fn scope(&self) -> &LinkScope {
        &self.scope
    }

    /// The profile the ticket was minted for.
    #[must_use]
    pub fn profile(&self) -> &str {
        &self.profile
    }

    pub(crate) fn client(&self) -> &Client {
        &self.client
    }

    pub(crate) fn describe(&self) -> DescribeSummary {
        self.describe.clone()
    }

    /// Close the socket. Closing detaches every subscription without an end reason.
    pub(crate) async fn close(self) {
        self.client.close().await;
        self.drain.abort();
    }
}

/// Drop events and leave every server request alone: a refusal would read as "skip" to an
/// agent waiting on the answer, and the launch is never the one to answer it.
async fn drain(mut inbound: InboundStream) {
    while let Some(item) = inbound.next().await {
        if let Inbound::ServerRequest(request) = item {
            request.responder.ignore();
        }
    }
}
