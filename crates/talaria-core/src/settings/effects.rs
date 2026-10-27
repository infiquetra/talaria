#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyEffect {
    Now,
    NewSessions,
    NeedsRestart,
    MayNeedRestart,
}
impl ApplyEffect {
    pub fn label(self) -> &'static str {
        match self {
            Self::Now => "applies now",
            Self::NewSessions => "new sessions",
            Self::NeedsRestart => "needs restart",
            Self::MayNeedRestart => "may need a restart",
        }
    }
}
const APPLY_EFFECTS: &[(&str, ApplyEffect, &str)] = &[
    (
        "display.skin",
        ApplyEffect::Now,
        "Hermes v0.22.4 tui_gateway/change_watcher.py:78-85,133-151 re-reads skin and broadcasts skin.changed",
    ),
    (
        "approvals.mode",
        ApplyEffect::Now,
        "Hermes v0.22.4 tools/approval_context.py:283-302 reads the live configuration cache on every approval check",
    ),
];
pub fn apply_effect(key: &str) -> (ApplyEffect, Option<&'static str>) {
    APPLY_EFFECTS
        .iter()
        .filter(|(pattern, _, _)| {
            if pattern.ends_with('.') {
                key.starts_with(pattern)
            } else {
                key == *pattern
            }
        })
        .max_by_key(|(pattern, _, _)| pattern.len())
        .map_or(
            (ApplyEffect::MayNeedRestart, None),
            |(_, effect, evidence)| (*effect, Some(*evidence)),
        )
}
