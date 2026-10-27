use proptest::prelude::*;
use talaria_core::capabilities::*;

fn grammar(input: &str) -> Option<InstallTarget> {
    let s = input.trim();
    if (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
    {
        return Some(InstallTarget::CatalogName(s.into()));
    }
    let rest = s.strip_prefix("https://")?;
    if !rest
        .bytes()
        .all(|b| (33..=126).contains(&b) && !b"@?#\\[]".contains(&b))
    {
        return None;
    }
    let (authority, path) = rest.split_once('/')?;
    if path.is_empty() {
        return None;
    }
    let host = if let Some((host, port)) = authority.split_once(':') {
        if port.is_empty() || port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let port: u32 = port.parse().ok()?;
        if !(1..=65535).contains(&port) {
            return None;
        }
        host
    } else {
        authority
    };
    if host.split('.').any(|label| {
        label.is_empty()
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    }) {
        return None;
    }
    Some(InstallTarget::Repository(s.into()))
}

#[test]
fn install_input_grammar() {
    for (input, expected) in [
        (
            "fixture-plugin",
            Some(InstallTarget::CatalogName("fixture-plugin".into())),
        ),
        (
            "  web_tools  ",
            Some(InstallTarget::CatalogName("web_tools".into())),
        ),
        ("___", Some(InstallTarget::CatalogName("___".into()))),
        ("123", Some(InstallTarget::CatalogName("123".into()))),
        (
            "https://github.com/example/plugin",
            Some(InstallTarget::Repository(
                "https://github.com/example/plugin".into(),
            )),
        ),
        (
            "https://github.com/example/plugin.git",
            Some(InstallTarget::Repository(
                "https://github.com/example/plugin.git".into(),
            )),
        ),
        (
            "https://git.example.org:8443/team/plugin",
            Some(InstallTarget::Repository(
                "https://git.example.org:8443/team/plugin".into(),
            )),
        ),
    ] {
        assert_eq!(classify_install_input(input).ok(), expected, "{input:?}");
    }
    for input in [
        "",
        "My.Plugin",
        "Fixture",
        "a.b",
        "https://github.com",
        "https://github.com/",
        "http://example.com/plugin.git",
        "HTTPS://github.com/x/y",
        "https://user@github.com/x/y",
        "https://github.com/x/y?ref=1",
        "https://github.com/x/y#main",
        "https://[::1]/x",
        "https://bücher.example/x",
        "git@github.com:x/y.git",
        "./plugin",
        "x/y",
        "https://github.com/x/y\u{7}",
        "https://git.example.org:0/x",
        "https://git.example.org:65536/x",
        "https://-host/x",
        "https://host-/x",
        "https://a..b/x",
        "https://host:123456/x",
        "https://host:2:3/x",
        "https://host/a\\b",
    ] {
        assert!(classify_install_input(input).is_err(), "{input:?}");
    }
    assert!(classify_install_input(&"a".repeat(65)).is_err());
    let mut runner = proptest::test_runner::TestRunner::new(proptest::test_runner::Config {
        failure_persistence: None,
        ..Default::default()
    });
    runner
        .run(&any::<String>(), |input| {
            prop_assert_eq!(classify_install_input(&input).ok(), grammar(&input));
            Ok(())
        })
        .unwrap();
    runner
        .run(
            &(
                "[a-z0-9_-]{1,64}",
                "[a-z][a-z0-9-]{0,10}[a-z0-9]",
                1u16..=65535,
                "[A-Za-z0-9._/-]{1,30}",
            ),
            |(name, host, port, path)| {
                prop_assert_eq!(
                    classify_install_input(&name),
                    Ok(InstallTarget::CatalogName(name))
                );
                let url = format!("https://{host}.example:{port}/{path}");
                prop_assert_eq!(
                    classify_install_input(&url),
                    Ok(InstallTarget::Repository(url))
                );
                Ok(())
            },
        )
        .unwrap();
}

#[test]
fn redact_paths_removes_every_occurrence() {
    let mut runner = proptest::test_runner::TestRunner::new(proptest::test_runner::Config {
        failure_persistence: None,
        ..Default::default()
    });
    runner
        .run(
            &("(/[a-z0-9._-]{1,8}){1,4}", "(/[a-z0-9._-]{1,8}){1,4}"),
            |(a, b)| {
                let text = format!("{a} first {b} second {a}/file {b}/more");
                let clean = redact_paths(&text, &a, &b);
                prop_assert!(!clean.contains(&a));
                prop_assert!(!clean.contains(&b));
                prop_assert_eq!(redact_paths(&text, "", ""), text.clone());
                prop_assert_eq!(redact_paths(&text, "/", "/"), text);
                Ok(())
            },
        )
        .unwrap();
}
