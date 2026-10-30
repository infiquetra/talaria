//! Gated-listener status probing.
use super::{GateStatus, RemoteError};
use url::Url;

pub async fn probe_status(base: &Url) -> Result<GateStatus, RemoteError> {
    let (status, value) =
        super::http::request(base, "/api/status", "GET", None, None, None).await?;
    if status != 200 {
        return Err(RemoteError::Http(status));
    }
    if value["auth_required"] != true {
        return Err(RemoteError::NotGated {
            url: base.to_string(),
        });
    }
    let strings = |key: &str| {
        value[key]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let flows = strings("auth_flows");
    if !flows.iter().any(|f| f == "native_pkce") {
        return Err(RemoteError::NoNativeFlow {
            url: base.to_string(),
        });
    }
    Ok(GateStatus {
        auth_required: true,
        auth_providers: strings("auth_providers"),
        auth_flows: flows,
    })
}
#[cfg(feature = "test-support")]
pub trait RequestSpy: Send + Sync {
    fn request(&self, url: &Url);
}
#[cfg(feature = "test-support")]
pub async fn probe_with_spy(base: &Url, spy: &dyn RequestSpy) -> Result<GateStatus, RemoteError> {
    super::validate_base(base)?;
    spy.request(base);
    probe_status(base).await
}
