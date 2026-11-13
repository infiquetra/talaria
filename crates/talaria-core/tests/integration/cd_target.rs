//! Tests 13 and 14 (#237): `/cd` target parsing and resolution.

use std::path::{Component, Path, PathBuf};

use proptest::prelude::*;
use talaria_core::workspace::{
    CdCommand, CdTargetError, display_path, parse_cd_command, resolve_cd_target,
};

const HOME: &str = "/home-fixture";
const FOLDER: &str = "/work/proj-a";

fn folder() -> PathBuf {
    PathBuf::from(FOLDER)
}

fn home() -> PathBuf {
    PathBuf::from(HOME)
}

#[test]
fn parse_and_resolve_cd_targets() {
    // Parsing: empty shows, anything else moves.
    assert_eq!(parse_cd_command(""), CdCommand::Show);
    assert_eq!(parse_cd_command("   "), CdCommand::Show);
    assert_eq!(
        parse_cd_command("proj-b"),
        CdCommand::Move("proj-b".to_owned())
    );
    assert_eq!(
        parse_cd_command("  /work/proj-b  "),
        CdCommand::Move("/work/proj-b".to_owned())
    );

    // Relative paths join onto the session's folder.
    assert_eq!(
        resolve_cd_target("proj-b", Some(&folder()), Some(&home())),
        Ok(PathBuf::from("/work/proj-a/proj-b"))
    );
    assert_eq!(
        resolve_cd_target("..", Some(&folder()), Some(&home())),
        Ok(PathBuf::from("/work"))
    );
    // With no known folder a relative path cannot resolve.
    assert_eq!(
        resolve_cd_target("proj-b", None, Some(&home())),
        Err(CdTargetError::RelativeWithoutFolder)
    );
    // Absolute paths are used as is.
    assert_eq!(
        resolve_cd_target("/work/proj-b", Some(&folder()), Some(&home())),
        Ok(PathBuf::from("/work/proj-b"))
    );
    assert_eq!(
        resolve_cd_target("/work/proj-b", None, None),
        Ok(PathBuf::from("/work/proj-b"))
    );

    // The home folder.
    assert_eq!(
        resolve_cd_target("~", Some(&folder()), Some(&home())),
        Ok(PathBuf::from(HOME))
    );
    assert_eq!(
        resolve_cd_target("~/rest", Some(&folder()), Some(&home())),
        Ok(PathBuf::from("/home-fixture/rest"))
    );
    assert_eq!(
        resolve_cd_target("~name", Some(&folder()), Some(&home())),
        Err(CdTargetError::TildeUser)
    );
    assert_eq!(
        resolve_cd_target("~name/x", Some(&folder()), Some(&home())),
        Err(CdTargetError::TildeUser)
    );
    assert_eq!(
        resolve_cd_target("~", Some(&folder()), None),
        Err(CdTargetError::NoHome)
    );
    assert_eq!(
        resolve_cd_target("~/rest", Some(&folder()), None),
        Err(CdTargetError::NoHome)
    );

    // One pair of matching quotes is removed; spaces inside are kept.
    assert_eq!(
        resolve_cd_target("'/work/a b'", Some(&folder()), Some(&home())),
        Ok(PathBuf::from("/work/a b"))
    );
    assert_eq!(
        resolve_cd_target("\"/work/a b\"", Some(&folder()), Some(&home())),
        Ok(PathBuf::from("/work/a b"))
    );
    assert_eq!(
        resolve_cd_target("'a b'", Some(&folder()), Some(&home())),
        Ok(PathBuf::from("/work/proj-a/a b"))
    );

    // Lexical normalisation.
    assert_eq!(
        resolve_cd_target("/work/./proj-b", Some(&folder()), Some(&home())),
        Ok(PathBuf::from("/work/proj-b"))
    );
    assert_eq!(
        resolve_cd_target("/work/a/../proj-b", Some(&folder()), Some(&home())),
        Ok(PathBuf::from("/work/proj-b"))
    );
    assert_eq!(
        resolve_cd_target("/work//proj-b", Some(&folder()), Some(&home())),
        Ok(PathBuf::from("/work/proj-b"))
    );
    assert_eq!(
        resolve_cd_target("/work/proj-b/", Some(&folder()), Some(&home())),
        Ok(PathBuf::from("/work/proj-b"))
    );
    assert_eq!(
        resolve_cd_target("/", Some(&folder()), Some(&home())),
        Ok(PathBuf::from("/"))
    );
    assert_eq!(
        resolve_cd_target("/..", Some(&folder()), Some(&home())),
        Ok(PathBuf::from("/"))
    );
    assert_eq!(
        resolve_cd_target("a/../../b", Some(&folder()), Some(&home())),
        Ok(PathBuf::from("/work/b"))
    );

    // Shortening for display.
    assert_eq!(
        display_path(Path::new("/home-fixture/work/api"), Some(Path::new(HOME))),
        "~/work/api"
    );
    assert_eq!(display_path(Path::new(HOME), Some(Path::new(HOME))), "~");
    // Only whole components shorten.
    assert_eq!(
        display_path(Path::new("/work"), Some(Path::new("/wor"))),
        "/work"
    );
    assert_eq!(
        display_path(Path::new("/work/proj-b"), None),
        "/work/proj-b"
    );
}

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn resolved_targets_are_stable(arg: String, rest: String) {
        let folder = PathBuf::from("/base").join(rest.trim_start_matches('/'));
        let home = Path::new(HOME);
        let first = match resolve_cd_target(&arg, Some(&folder), Some(home)) {
            Ok(path) => path,
            Err(_) => return Ok(()),
        };
        prop_assert!(first.is_absolute(), "{first:?} from {arg:?}");
        prop_assert!(
            !first.components().any(|component| matches!(
                component,
                Component::CurDir | Component::ParentDir
            )),
            "{first:?} from {arg:?}"
        );
        let again = resolve_cd_target(
            &first.to_string_lossy(),
            Some(&folder),
            Some(home),
        )
        .expect("resolving a resolved target");
        prop_assert_eq!(first, again);
    }
}
