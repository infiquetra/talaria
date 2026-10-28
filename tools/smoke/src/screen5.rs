//! Read-only predicates for workflow screens and command output.

/// Finds an overlay title for a particular profile.
pub fn overlay_title_found(_text: &str, _title_fmt: &str, _profile: &str) -> bool {
    panic!("not implemented")
}

macro_rules! predicate {
    ($($name:ident),* $(,)?) => {$ (
        /// Tests for the corresponding exact text in the screen or output.
        pub fn $name(_text: &str) -> bool { panic!("not implemented") }
    )*};
}
predicate!(
    workflows_empty,
    ultracode_status,
    created_folder,
    created,
    unchanged,
    installed,
    missing_tiers,
    empty_list,
    idle_screen,
);
