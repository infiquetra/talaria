use talaria_core::settings::{
    CommandError, ConfigCommand, Screen, SettingsEffect, SettingsInput, SettingsState,
    is_secret_config_line, parse_config_command as parse_command,
};

#[test]
fn parse_config_command() {
    for (input, expected) in [
        ("", ConfigCommand::Open { filter: None }),
        (
            "compact display",
            ConfigCommand::Open {
                filter: Some("compact display".into()),
            },
        ),
        (
            "api_key",
            ConfigCommand::Open {
                filter: Some("api_key".into()),
            },
        ),
        (
            "binding key",
            ConfigCommand::Open {
                filter: Some("binding key".into()),
            },
        ),
        (
            "display.compact=true",
            ConfigCommand::Set(vec![("display.compact".into(), "true".into())]),
        ),
        (
            r#"display.skin="night blue" display.compact=on"#,
            ConfigCommand::Set(vec![
                ("display.skin".into(), "night blue".into()),
                ("display.compact".into(), "on".into()),
            ]),
        ),
        (
            r#"display.skin="a\"b\\c""#,
            ConfigCommand::Set(vec![("display.skin".into(), "a\"b\\c".into())]),
        ),
    ] {
        assert_eq!(parse_command(input), Ok(expected), "input {input}");
    }

    for input in [
        "=value",
        "display.skin=one other",
        "display.skin=one display.skin=two",
        "display.skin=\"unterminated",
    ] {
        assert!(
            matches!(parse_command(input), Err(CommandError::Syntax { .. })),
            "input {input}"
        );
    }
    for (input, key) in [
        ("delegation.api_key=x", "delegation.api_key"),
        ("delegation.api_key=\"unfinished", "delegation.api_key"),
        ("delegation.api_key x", "delegation.api_key"),
        (
            "display.compact=true delegation.api_key=x",
            "delegation.api_key",
        ),
        ("key binding", "key"),
        ("token count", "token"),
    ] {
        assert_eq!(
            parse_command(input),
            Err(CommandError::Secret { key: key.into() })
        );
    }
    for (line, expected) in [
        (" /config delegation.api_key=x", true),
        ("/config key binding", true),
        ("/config api_key", false),
        ("/config binding key", false),
        ("/config display.compact=true", false),
        ("/configuration key x", false),
    ] {
        assert_eq!(is_secret_config_line(line), expected, "line {line}");
    }

    let mut state = SettingsState::new(&[]);
    let effects = state.update(SettingsInput::Command {
        profile: "fixture".into(),
        args: "delegation.api_key=x".into(),
    });
    assert_eq!(effects, vec![SettingsEffect::Notice("delegation.api_key is a secret; set it in /config, not on the command line. Nothing was sent.".into()), SettingsEffect::ForgetHistoryLine]);
    assert!(matches!(state.view().screen, Screen::Closed));
    let effects = state.update(SettingsInput::Command {
        profile: "fixture".into(),
        args: "=value".into(),
    });
    assert!(
        matches!(&effects[..], [SettingsEffect::Notice(text)] if text.starts_with("Use /config key=value [key=value …]: "))
    );
    assert!(matches!(state.view().screen, Screen::Closed));
}
