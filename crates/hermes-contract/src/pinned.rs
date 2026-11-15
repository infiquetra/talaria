//! The Hermes release whose published contract this crate implements.

pub const TAG: &str = "v0.23.1";
pub const COMMIT: &str = "3b421578588243f920886e6fe79a6faa87ea026e";

/// The oldest Hermes release accepted by live tests and fixture recording.
pub const OLDEST_SUPPORTED: (u64, u64, u64) = (0, 23, 0);

fn version_triple(value: &str) -> Option<(u64, u64, u64)> {
    let value = value.trim();
    let value = value
        .strip_prefix('v')
        .or_else(|| value.strip_prefix('V'))
        .unwrap_or(value);
    let value = value.split_once('+').map_or(value, |(base, _)| base);
    let mut parts = value.split('.');
    let mut component = || {
        let part = parts.next()?;
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        part.parse::<u64>().ok()
    };
    let triple = (component()?, component()?, component()?);
    parts.next().is_none().then_some(triple)
}

/// Explain why a gateway does not meet the live-test version floor.
pub fn version_skip_reason(code_version: Option<&str>, oldest: (u64, u64, u64)) -> Option<String> {
    let (major, minor, patch) = oldest;
    let Some(raw) = code_version.filter(|value| !value.trim().is_empty()) else {
        return Some(format!(
            "the gateway reported no code_version; live tests need Hermes {major}.{minor}.{patch} or later and are skipped without it"
        ));
    };
    if version_triple(raw).is_some_and(|version| version >= oldest) {
        None
    } else {
        Some(format!(
            "gateway code_version \"{raw}\" is older than the oldest supported Hermes release {major}.{minor}.{patch}; live tests are skipped"
        ))
    }
}

/// Explain why a gateway is too old for a named live-test feature.
pub fn feature_skip_reason(
    code_version: Option<&str>,
    feature: &str,
    since: &str,
) -> Option<String> {
    if let Some(since_version) = version_triple(since)
        && code_version
            .and_then(version_triple)
            .is_some_and(|version| version >= since_version)
    {
        return None;
    }
    let shown = code_version
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("unknown");
    Some(format!(
        "needs {feature} (Hermes {since} or later); this gateway runs {shown}"
    ))
}

/// Choose version-floor admission or the explicitly requested pinned revision.
pub fn live_test_skip(
    code_version: Option<&str>,
    code_sha: Option<&str>,
    exact: bool,
) -> Option<String> {
    if exact {
        live_skip_reason(code_sha)
    } else {
        version_skip_reason(code_version, OLDEST_SUPPORTED)
    }
}

/// Report the complete gateway identity on success and skip paths.
pub fn gateway_version_line(code_version: Option<&str>, code_sha: Option<&str>) -> String {
    fn shown(value: Option<&str>) -> &str {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("unknown")
    }
    format!(
        "gateway code_version={} code_sha={}",
        shown(code_version),
        shown(code_sha)
    )
}

/// Resolve the live-test profile without consulting process state.
pub fn live_profile_from(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("fixture")
        .to_owned()
}

/// The profile shared by live tests, smoke runs, and fixture recording.
pub fn live_profile() -> String {
    live_profile_from(std::env::var("TALARIA_LIVE_PROFILE").ok().as_deref())
}

/// Only the exact character `1` requests pinned-revision admission.
pub fn live_exact_from(value: Option<&str>) -> bool {
    value == Some("1")
}

/// Whether this live run explicitly requests the pinned gateway revision.
pub fn live_exact() -> bool {
    live_exact_from(std::env::var("TALARIA_LIVE_EXACT").ok().as_deref())
}

/// Explain why a gateway's code identity is unsuitable for pinned live tests.
pub fn live_skip_reason(code_sha: Option<&str>) -> Option<String> {
    match code_sha {
        Some(sha)
            if sha.len() >= 7
                && COMMIT
                    .get(..sha.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(sha)) =>
        {
            None
        }
        Some(sha) => Some(format!(
            "gateway code_sha \"{sha}\" does not match the pinned Hermes commit {COMMIT} ({TAG}); live tests are skipped"
        )),
        None => Some(format!(
            "the gateway reported no code_sha; live tests need Hermes {TAG} (commit {COMMIT}) and are skipped without it"
        )),
    }
}
