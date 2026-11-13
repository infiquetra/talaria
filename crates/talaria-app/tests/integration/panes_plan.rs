//! Pure pane planning: detection, direction, command lines, texts. Issue #246.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use talaria_app::panes::{
    Direction, DirectionSetting, Multiplexer, MultiplexerSetting, PaneEnv, PaneOutcome,
    PaneRefusal, attach_command_line, choose_direction, detect, opening_text, outcome_text,
    parse_new_pane, split_command, type_commands,
};
use talaria_core::directory::rows::RowKey;

fn key(session_id: &str, profile: &str) -> RowKey {
    RowKey {
        profile: profile.to_owned(),
        session_id: session_id.to_owned(),
    }
}

fn herdr_env() -> PaneEnv {
    PaneEnv {
        herdr_env: Some("1".to_owned()),
        herdr_pane_id: Some("w1:p1".to_owned()),
        herdr_bin_path: Some("/bin/herdr".to_owned()),
        tmux: None,
        tmux_pane: None,
    }
}

fn tmux_env() -> PaneEnv {
    PaneEnv {
        herdr_env: None,
        herdr_pane_id: None,
        herdr_bin_path: None,
        tmux: Some("/work/tmux-1/default,42,0".to_owned()),
        tmux_pane: Some("%1".to_owned()),
    }
}

#[test]
fn detect_rules() {
    let herdr = herdr_env();
    let expected_herdr = Multiplexer::Herdr {
        bin: PathBuf::from("/bin/herdr"),
        pane: "w1:p1".to_owned(),
    };
    assert_eq!(
        detect(&herdr, MultiplexerSetting::Auto),
        Some(expected_herdr.clone())
    );
    assert_eq!(
        detect(&herdr, MultiplexerSetting::Herdr),
        Some(expected_herdr)
    );
    assert_eq!(detect(&herdr, MultiplexerSetting::Tmux), None);
    assert_eq!(detect(&herdr, MultiplexerSetting::Off), None);

    let tmux = tmux_env();
    let expected_tmux = Multiplexer::Tmux {
        bin: PathBuf::from("tmux"),
        pane: "%1".to_owned(),
    };
    assert_eq!(
        detect(&tmux, MultiplexerSetting::Auto),
        Some(expected_tmux.clone())
    );
    assert_eq!(detect(&tmux, MultiplexerSetting::Tmux), Some(expected_tmux));
    assert_eq!(detect(&tmux, MultiplexerSetting::Herdr), None);
    assert_eq!(detect(&tmux, MultiplexerSetting::Off), None);

    // Both present: automatic picks Herdr, a forced setting wins.
    let both = PaneEnv {
        tmux: tmux.tmux.clone(),
        tmux_pane: tmux.tmux_pane.clone(),
        ..herdr
    };
    assert!(matches!(
        detect(&both, MultiplexerSetting::Auto),
        Some(Multiplexer::Herdr { .. })
    ));
    assert!(matches!(
        detect(&both, MultiplexerSetting::Tmux),
        Some(Multiplexer::Tmux { .. })
    ));

    // Nothing, or empty strings, or a Herdr flag that is not `1`: none.
    let empty = PaneEnv::default();
    assert_eq!(detect(&empty, MultiplexerSetting::Auto), None);
    let blank = PaneEnv {
        herdr_env: Some(String::new()),
        herdr_pane_id: Some(String::new()),
        herdr_bin_path: Some(String::new()),
        tmux: Some(String::new()),
        tmux_pane: Some(String::new()),
    };
    assert_eq!(detect(&blank, MultiplexerSetting::Auto), None);
    let off_flag = PaneEnv {
        herdr_env: Some("0".to_owned()),
        ..herdr_env()
    };
    assert_eq!(detect(&off_flag, MultiplexerSetting::Auto), None);
    assert_eq!(detect(&off_flag, MultiplexerSetting::Herdr), None);
}

#[test]
fn direction_auto_rule() {
    assert_eq!(
        choose_direction(DirectionSetting::Auto, 160, 48),
        Direction::Right
    );
    assert_eq!(
        choose_direction(DirectionSetting::Auto, 200, 120),
        Direction::Down
    );
    assert_eq!(
        choose_direction(DirectionSetting::Auto, 100, 30),
        Direction::Down
    );
    assert_eq!(
        choose_direction(DirectionSetting::Auto, 120, 40),
        Direction::Right
    );
    assert_eq!(
        choose_direction(DirectionSetting::Right, 80, 24),
        Direction::Right
    );
    assert_eq!(
        choose_direction(DirectionSetting::Down, 300, 60),
        Direction::Down
    );
}

#[test]
fn attach_command_line_quotes_and_refuses() {
    let line = attach_command_line(Path::new("/t/talaria"), &key("local-ab", "fixture"))
        .expect("a plain line");
    assert_eq!(line, "'/t/talaria' attach 'local-ab' --profile 'fixture'");
    let spaced = attach_command_line(Path::new("/my dir/talaria"), &key("local-ab", "fixture"))
        .expect("a space is quoted");
    assert_eq!(
        spaced,
        "'/my dir/talaria' attach 'local-ab' --profile 'fixture'"
    );

    for bad_id in ["loc'al", "a;b", "a\nb", "a\\b", "-x"] {
        assert_eq!(
            attach_command_line(Path::new("/t/talaria"), &key(bad_id, "fixture")),
            Err(PaneRefusal::UnsafeArgument),
            "id {bad_id:?} must be refused"
        );
    }
    for bad_profile in ["..", ".", "fi'xture", "fix\\ture", "a\nb"] {
        assert_eq!(
            attach_command_line(Path::new("/t/talaria"), &key("local-ab", bad_profile)),
            Err(PaneRefusal::UnsafeArgument),
            "profile {bad_profile:?} must be refused"
        );
    }
    for bad_exe in ["/t/tal'aria", "/t\\talaria", "/t/tal\naria"] {
        assert_eq!(
            attach_command_line(Path::new(bad_exe), &key("local-ab", "fixture")),
            Err(PaneRefusal::UnsafeExePath),
            "exe {bad_exe:?} must be refused"
        );
    }

    // With session id `local-ab` and profile `fixture` the line is the exe
    // path plus 40 bytes: 960 passes at exactly 1,000, 961 does not.
    let fitting = "e".repeat(960);
    let line = attach_command_line(Path::new(&fitting), &key("local-ab", "fixture"))
        .expect("exactly 1,000 bytes");
    assert_eq!(line.len(), 1_000);
    let long = "e".repeat(961);
    assert_eq!(
        attach_command_line(Path::new(&long), &key("local-ab", "fixture")),
        Err(PaneRefusal::TooLong { bytes: 1_001 })
    );
}

fn args(words: &[&str]) -> Vec<OsString> {
    words.iter().map(OsString::from).collect()
}

#[test]
fn herdr_and_tmux_command_specs() {
    let herdr = Multiplexer::Herdr {
        bin: PathBuf::from("/bin/herdr"),
        pane: "w1:p1".to_owned(),
    };
    let split = split_command(
        &herdr,
        Direction::Right,
        Some(Path::new("/work/proj")),
        false,
    );
    assert_eq!(split.program, PathBuf::from("/bin/herdr"));
    assert_eq!(
        split.args,
        args(&[
            "pane",
            "split",
            "--current",
            "--direction",
            "right",
            "--cwd",
            "/work/proj",
            "--no-focus",
        ])
    );
    let split = split_command(&herdr, Direction::Down, None, true);
    assert_eq!(
        split.args,
        args(&[
            "pane",
            "split",
            "--current",
            "--direction",
            "down",
            "--focus"
        ])
    );
    let typed = type_commands(
        &herdr,
        "w1:p7",
        "'/t/talaria' attach 'local-ab' --profile 'fixture'",
    );
    assert_eq!(typed.len(), 1);
    assert_eq!(typed[0].program, PathBuf::from("/bin/herdr"));
    assert_eq!(
        typed[0].args,
        args(&[
            "pane",
            "run",
            "w1:p7",
            "'/t/talaria' attach 'local-ab' --profile 'fixture'"
        ])
    );

    let tmux = Multiplexer::Tmux {
        bin: PathBuf::from("tmux"),
        pane: "%1".to_owned(),
    };
    let split = split_command(
        &tmux,
        Direction::Right,
        Some(Path::new("/work/proj")),
        false,
    );
    assert_eq!(split.program, PathBuf::from("tmux"));
    assert_eq!(
        split.args,
        args(&[
            "split-window",
            "-h",
            "-d",
            "-t",
            "%1",
            "-c",
            "/work/proj",
            "-P",
            "-F",
            "#{pane_id}",
        ])
    );
    let split = split_command(&tmux, Direction::Down, None, true);
    assert_eq!(
        split.args,
        args(&["split-window", "-v", "-t", "%1", "-P", "-F", "#{pane_id}"])
    );
    let typed = type_commands(
        &tmux,
        "%12",
        "'/t/talaria' attach 'local-ab' --profile 'fixture'",
    );
    assert_eq!(typed.len(), 2);
    assert_eq!(
        typed[0].args,
        args(&[
            "send-keys",
            "-t",
            "%12",
            "-l",
            "'/t/talaria' attach 'local-ab' --profile 'fixture'",
        ])
    );
    assert_eq!(typed[1].args, args(&["send-keys", "-t", "%12", "Enter"]));
}

#[test]
fn parse_new_pane_ids() {
    let herdr = Multiplexer::Herdr {
        bin: PathBuf::from("/bin/herdr"),
        pane: "w1:p1".to_owned(),
    };
    assert_eq!(
        parse_new_pane(&herdr, "{\"result\":{\"pane\":{\"pane_id\":\"w1:p7\"}}}\n"),
        Some("w1:p7".to_owned())
    );
    for bad in [
        "",
        "{bad json",
        "{}",
        "{\"result\":{}}",
        "{\"result\":{\"pane\":{}}}",
        "{\"result\":{\"pane\":{\"pane_id\":\"\"}}}",
        "{\"result\":{\"pane\":{\"pane_id\":7}}}",
    ] {
        assert_eq!(parse_new_pane(&herdr, bad), None, "herdr {bad:?}");
    }

    let tmux = Multiplexer::Tmux {
        bin: PathBuf::from("tmux"),
        pane: "%1".to_owned(),
    };
    assert_eq!(parse_new_pane(&tmux, "%12\n"), Some("%12".to_owned()));
    for bad in ["", "12", "%", "%1x", "w1:p7", "% 12"] {
        assert_eq!(parse_new_pane(&tmux, bad), None, "tmux {bad:?}");
    }
}

#[test]
fn outcome_texts() {
    const COMMAND: &str = "'/t/talaria' attach 'local-ab' --profile 'fixture'";
    assert_eq!(
        opening_text("build api", Some("tmux")),
        Some("Opening \"build api\" in a new tmux pane…".to_owned())
    );
    assert_eq!(
        opening_text("build api", Some("Herdr")),
        Some("Opening \"build api\" in a new Herdr pane…".to_owned())
    );
    assert_eq!(opening_text("build api", None), None);

    for mux in ["Herdr", "tmux"] {
        assert_eq!(
            outcome_text("build api", &PaneOutcome::Opened { multiplexer: mux }),
            format!("Opened \"build api\" in a new {mux} pane.")
        );
        assert_eq!(
            outcome_text(
                "build api",
                &PaneOutcome::SplitFailed {
                    multiplexer: mux,
                    reason: "exit code 1".to_owned(),
                }
            ),
            format!("{mux} could not open a pane: exit code 1.")
        );
        assert_eq!(
            outcome_text(
                "build api",
                &PaneOutcome::TypeFailed {
                    multiplexer: mux,
                    reason: "timed out after 5 s".to_owned(),
                    command: COMMAND.to_owned(),
                }
            ),
            format!(
                "Opened a new {mux} pane, but could not start Talaria in it: timed out after 5 s. Run this there: {COMMAND}"
            )
        );
    }
    assert_eq!(
        outcome_text(
            "build api",
            &PaneOutcome::NoMultiplexer {
                command: COMMAND.to_owned(),
                off: false,
            }
        ),
        format!(
            "Not inside Herdr or tmux. To watch \"build api\" side by side, run this in another terminal: {COMMAND}"
        )
    );
    assert_eq!(
        outcome_text(
            "build api",
            &PaneOutcome::NoMultiplexer {
                command: COMMAND.to_owned(),
                off: true,
            }
        ),
        format!(
            "Opening in a new pane is turned off ([panes] multiplexer = \"off\"). Run this in another terminal: {COMMAND}"
        )
    );
    assert_eq!(
        outcome_text(
            "build api",
            &PaneOutcome::Refused(PaneRefusal::UnsafeArgument)
        ),
        "Cannot open this session in a pane: its id or profile has characters Talaria will not put on a command line."
    );
    assert_eq!(
        outcome_text(
            "build api",
            &PaneOutcome::Refused(PaneRefusal::UnsafeExePath)
        ),
        "Cannot open this session in a pane: the path of this Talaria binary has characters Talaria will not put on a command line."
    );
    assert_eq!(
        outcome_text(
            "build api",
            &PaneOutcome::Refused(PaneRefusal::TooLong { bytes: 1_001 })
        ),
        "Cannot open this session in a pane: the command would be longer than 1,000 bytes."
    );
    assert_eq!(
        outcome_text("build api", &PaneOutcome::Refused(PaneRefusal::BinaryGone)),
        "This Talaria binary is no longer on disk (it was rebuilt or moved). Restart Talaria, then try again."
    );
}
