//! Connectors and messaging calls. Consent is per profile for this process.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use hermes_client::rest::RestClient;
use hermes_client::Client;
use talaria_core::connectors::{ConnectorsEffect, ConnectorsInput};
use zeroize::Zeroizing;

/// Profiles that have agreed to a connectors call in this process.
#[derive(Clone, Default)]
pub struct ConnectorsConsent(Arc<Mutex<BTreeSet<String>>>);

impl ConnectorsConsent {
    /// Whether `profile` has already agreed.
    #[must_use]
    pub fn granted(&self, profile: &str) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .contains(profile)
    }

    /// Remember that `profile` agreed. Later calls in this process do not ask again.
    pub fn grant(&self, profile: &str) {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert(profile.to_owned());
    }
}

/// Runs one overlay effect against the socket or the dashboard.
pub struct ConnectorsService {
    client: Client,
    rest: RestClient,
    profile: String,
    consent: ConnectorsConsent,
}

impl ConnectorsService {
    /// Keep the socket, the dashboard client, the profile name, and the shared consent set.
    #[must_use]
    pub fn new(
        client: Client,
        rest: RestClient,
        profile: String,
        consent: ConnectorsConsent,
    ) -> Self {
        Self {
            client,
            rest,
            profile,
            consent,
        }
    }

    /// Carry out `effect`. Secrets are present only for a platform save.
    pub async fn execute(
        &self,
        effect: ConnectorsEffect,
        secrets: Option<BTreeMap<String, Zeroizing<String>>>,
    ) -> Vec<ConnectorsInput> {
        let _ = (self, effect, secrets);
        todo!("connectors service stub")
    }
}
