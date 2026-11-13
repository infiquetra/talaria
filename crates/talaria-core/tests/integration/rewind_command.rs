//! Typed command grammar. Issue 233. Checkpoint rewind and diff.

use talaria_core::rewind::{
    DiffCommand, RollbackCommand, is_checkpoint_target, parse_diff, parse_rollback, parse_undo,
};

#[test]
fn rollback_grammar() {
    let cases = [
        ("", RollbackCommand::List),
        ("list", RollbackCommand::List),
        ("ls", RollbackCommand::List),
        ("LIST", RollbackCommand::List),
        ("  ls  ", RollbackCommand::List),
        ("list x", RollbackCommand::Usage),
        ("diff", RollbackCommand::Usage),
        ("diff 2 extra", RollbackCommand::Usage),
        ("diff 2", RollbackCommand::Diff("2".into())),
        ("diff a1b2c3d4", RollbackCommand::Diff("a1b2c3d4".into())),
        ("diff zzz", RollbackCommand::BadTarget("zzz".into())),
        ("--all", RollbackCommand::BadTarget("--all".into())),
        ("+3", RollbackCommand::BadTarget("+3".into())),
        ("3_0", RollbackCommand::BadTarget("3_0".into())),
        ("-a1b2", RollbackCommand::BadTarget("-a1b2".into())),
        (
            "3 src/a  b.rs",
            RollbackCommand::Restore {
                target: "3".into(),
                file_path: Some("src/a  b.rs".into()),
                all: false,
            },
        ),
        (
            "3 file --all",
            RollbackCommand::Restore {
                target: "3".into(),
                file_path: Some("file".into()),
                all: false,
            },
        ),
        (
            "3 --force",
            RollbackCommand::Restore {
                target: "3".into(),
                file_path: None,
                all: true,
            },
        ),
        (
            "3 --all file",
            RollbackCommand::Restore {
                target: "3".into(),
                file_path: Some("file".into()),
                all: false,
            },
        ),
        (
            "a1b2c3d4",
            RollbackCommand::Restore {
                target: "a1b2c3d4".into(),
                file_path: None,
                all: false,
            },
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(parse_rollback(input), expected, "{input}");
    }
    assert!(is_checkpoint_target("1"));
    assert!(is_checkpoint_target("999999"));
    assert!(is_checkpoint_target("a1b2"));
    assert!(!is_checkpoint_target("1234567"));
    assert!(!is_checkpoint_target("zzz"));
    assert!(!is_checkpoint_target("-a1b2"));
}

#[test]
fn undo_count_grammar() {
    assert_eq!(parse_undo(""), Ok(1));
    assert_eq!(parse_undo("3"), Ok(3));
    assert_eq!(parse_undo("999"), Ok(999));
    assert_eq!(parse_undo("1000"), Err(()));
    assert_eq!(parse_undo("0"), Err(()));
    assert_eq!(parse_undo("-1"), Err(()));
    assert_eq!(parse_undo("x"), Err(()));
    assert_eq!(parse_undo("2 3"), Err(()));
}

#[test]
fn diff_grammar() {
    assert_eq!(parse_diff(""), DiffCommand::Session { stat_only: false });
    assert_eq!(
        parse_diff("session"),
        DiffCommand::Session { stat_only: false }
    );
    assert_eq!(
        parse_diff("--stat"),
        DiffCommand::Session { stat_only: true }
    );
    assert_eq!(parse_diff("stat"), DiffCommand::Session { stat_only: true });
    assert_eq!(
        parse_diff("session --stat"),
        DiffCommand::Session { stat_only: true }
    );
    assert_eq!(
        parse_diff("--stat session"),
        DiffCommand::Session { stat_only: true }
    );
    assert_eq!(parse_diff("staged"), DiffCommand::GitView);
    assert_eq!(parse_diff("all"), DiffCommand::GitView);
    assert_eq!(parse_diff("src/x.rs"), DiffCommand::GitView);
    assert_eq!(parse_diff("session staged"), DiffCommand::GitView);
}
