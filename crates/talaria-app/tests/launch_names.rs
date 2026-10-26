//! Worktree names. Expected slugs are the examples from the launch rules, written by hand.

use std::collections::BTreeSet;

use talaria_app::bg::{NameError, explicit_slug, pick_names, slugify};

#[test]
fn slugify_follows_desktop_rule() {
    assert_eq!(
        slugify("Fix the flaky test in net.rs"),
        "fix-the-flaky-test-in-net-rs"
    );
    assert_eq!(slugify("   "), "work");
    assert_eq!(slugify("Ünïcode only"), "n-code-only");
    let long = format!("{} bbb", "a".repeat(39));
    assert_eq!(slugify(&long), "a".repeat(39));
    assert_eq!(slugify(&"z".repeat(200)).chars().count(), 40);
}

#[test]
fn explicit_name_needs_a_letter_or_digit() {
    assert_eq!(
        explicit_slug("Fix the flaky test in net.rs").as_deref(),
        Ok("fix-the-flaky-test-in-net-rs")
    );
    assert_eq!(explicit_slug("--"), Err(NameError::Empty));
    assert_eq!(explicit_slug("!!"), Err(NameError::Empty));
    assert_eq!(explicit_slug("é"), Err(NameError::Empty));
    assert_eq!(explicit_slug("   "), Err(NameError::Empty));
}

#[test]
fn pick_names_skips_taken_folders_and_branches() {
    let slug = "fix";
    let free = pick_names(slug, &BTreeSet::new(), &BTreeSet::new()).expect("a free pair");
    assert_eq!(free.dir_name, "talaria-fix");
    assert_eq!(free.branch, "talaria/fix");

    let mut dirs = BTreeSet::new();
    dirs.insert("talaria-fix".to_owned());
    let next = pick_names(slug, &dirs, &BTreeSet::new()).expect("the second pair");
    assert_eq!(next.dir_name, "talaria-fix-2");
    assert_eq!(next.branch, "talaria/fix-2");

    let mut branches = BTreeSet::new();
    branches.insert("talaria/fix".to_owned());
    branches.insert("talaria/fix-2".to_owned());
    let third = pick_names(slug, &BTreeSet::new(), &branches).expect("the third pair");
    assert_eq!(third.branch, "talaria/fix-3");

    let mut all_dirs = BTreeSet::new();
    let mut all_branches = BTreeSet::new();
    all_dirs.insert("talaria-fix".to_owned());
    all_branches.insert("talaria/fix".to_owned());
    for number in 2..=99 {
        all_dirs.insert(format!("talaria-fix-{number}"));
        all_branches.insert(format!("talaria/fix-{number}"));
    }
    assert!(pick_names(slug, &all_dirs, &all_branches).is_none());
}

proptest::proptest! {
    #![proptest_config(names_config())]

    #[test]
    fn names_are_valid_branch_names(prompt in proptest::collection::vec(proptest::prelude::any::<char>(), 0..80usize)) {
        let text: String = prompt.into_iter().collect();
        let slug = slugify(&text);
        let names = pick_names(&slug, &BTreeSet::new(), &BTreeSet::new()).expect("a pair");
        let rest = names.branch.strip_prefix("talaria/").unwrap_or("");
        let branch_ok = !rest.is_empty()
            && !rest.starts_with('-')
            && !rest.ends_with('-')
            && !rest.contains("--")
            && rest
                .chars()
                .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-');
        let dir_ok = names.dir_name == format!("talaria-{rest}");
        proptest::prop_assert!(branch_ok);
        proptest::prop_assert!(dir_ok);
    }
}

fn names_config() -> proptest::test_runner::Config {
    proptest::test_runner::Config {
        cases: 32,
        failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::Off)),
        ..proptest::test_runner::Config::default()
    }
}
