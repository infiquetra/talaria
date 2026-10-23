//! The native-method bound is the gate for typed calls.
//!
//! `session.compress` is a legacy marker and does not implement `NativeMethod`.
//! `session.list` does. Both checks pass while the channel bodies are still stubs.

#[test]
fn legacy_session_compress_is_not_a_native_method() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/compile_fail/legacy_session_compress.rs");
}

#[test]
fn canonical_methods_are_native() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/compile_pass/canonical_session_list.rs");
}
