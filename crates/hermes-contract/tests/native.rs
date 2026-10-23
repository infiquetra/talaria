//! Test 35. Canonical markers and the legacy methods the owner still serves are native.

fn needs_native<M: hermes_contract::native::NativeMethod>() {
    let _ = std::any::type_name::<M>();
}

#[test]
fn canonical_and_selected_legacy_markers_are_native() {
    needs_native::<hermes_contract::methods::canonical::prompt_submit::Marker>();
    needs_native::<hermes_contract::methods::canonical::runtime_describe::Marker>();
    needs_native::<hermes_contract::methods::legacy::session_most_recent::Marker>();
    needs_native::<hermes_contract::methods::legacy::process_stop::Marker>();
}
