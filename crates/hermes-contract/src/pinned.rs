//! The Hermes release whose published contract this crate implements.

pub const TAG: &str = "v0.22.4";
pub const COMMIT: &str = "4d04303818b61bf3aa09278af48356315515965b";

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
