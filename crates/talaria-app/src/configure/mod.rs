//! Application services for configuration sessions.

pub mod backends;
pub mod service;
pub mod wire;

pub use backends::{ConfigureBackends, SiblingBackends};
pub use service::ConfigureService;

pub type Service = ConfigureService<SiblingBackends>;

/// Build the existing owner adapters for one already-connected profile.
pub async fn discover_service(
    home: std::path::PathBuf,
    client: hermes_client::Client,
    profile: String,
    consent: crate::connectors::ConnectorsConsent,
) -> Option<std::sync::Arc<Service>> {
    use hermes_client::{
        DiscoverOptions, EnsurePolicy, discover_with,
        rest::{NativeHttpMinter, TicketMinter},
    };
    use std::sync::Arc;
    let endpoint = Arc::new(
        discover_with(
            &home,
            &DiscoverOptions {
                ensure: EnsurePolicy::Never,
                ..DiscoverOptions::default()
            },
        )
        .await
        .ok()?,
    );
    endpoint.resolve_profile(&profile).ok()?;
    let origin = Some(endpoint.api_origin().to_owned());
    let profiles = endpoint
        .profiles()
        .iter()
        .map(|profile| profile.name.clone())
        .collect::<Vec<_>>();
    let factory: crate::settings::MinterFactory =
        Arc::new(move |name: &str| -> Arc<dyn TicketMinter> {
            // The adapters check the immutable served-profile list before minting.
            let profile = endpoint
                .resolve_profile(name)
                .expect("served profile checked before minting");
            Arc::new(NativeHttpMinter::new(
                home.clone(),
                endpoint.clone(),
                profile,
            ))
        });
    let settings = Arc::new(crate::settings::SettingsService::new(
        origin.clone(),
        profiles.clone(),
        factory.clone(),
    ));
    let backends =
        SiblingBackends::new(settings, client.clone(), consent, origin, profiles, factory);
    Some(Arc::new(ConfigureService::new(
        Some(client),
        backends,
        profile,
    )))
}
