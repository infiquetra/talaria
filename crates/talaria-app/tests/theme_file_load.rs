use std::fs;
use talaria_app::theme_file::{load, reserved_id, valid_id};

#[test]
fn identifiers_are_bounded_and_builtins_reserved() {
    for id in ["a", "quiet-amber", "profile_1", &"a".repeat(64)] {
        assert!(valid_id(id), "{id}");
    }
    for id in ["", "../x", "a/b", "A", "-a", &"a".repeat(65)] {
        assert!(!valid_id(id), "{id}");
    }
    for id in ["terminal", "dark", "light", "ember", "paper"] {
        assert!(reserved_id(id));
    }
    assert!(!reserved_id("quiet"));
}

#[test]
fn reserved_names_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("themes")).unwrap();
    for id in ["terminal", "dark", "light", "ember", "paper"] {
        fs::write(dir.path().join(format!("themes/{id}.toml")), "schema=1\n").unwrap();
        let error = load(&dir.path().join("config.toml"), id).unwrap_err();
        assert!(
            error.contains(&format!("themes/{id}.toml")) && error.contains("reserved"),
            "{error}"
        );
    }
}

#[cfg(unix)]
#[test]
fn the_theme_folder_cannot_be_replaced_by_an_outside_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let configuration = dir.path().join("configuration");
    let outside = dir.path().join("outside");
    fs::create_dir(&configuration).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("escape.toml"), "schema=1\n").unwrap();
    std::os::unix::fs::symlink(outside, configuration.join("themes")).unwrap();
    let error = load(&configuration.join("config.toml"), "escape").unwrap_err();
    assert!(error.contains("themes/escape.toml"));
    assert!(!error.contains(&dir.path().display().to_string()));
}

#[test]
fn load_is_confined_bounded_and_names_only_relative_files() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let themes = dir.path().join("themes");
    fs::create_dir(&themes).unwrap();
    fs::write(themes.join("good.toml"), "schema = 1\n").unwrap();
    assert!(load(&config, "good").is_ok());
    for id in ["terminal", "dark", "light", "ember", "paper", "missing"] {
        let error = load(&config, id).unwrap_err();
        assert!(error.contains(id));
        assert!(!error.contains(&dir.path().display().to_string()));
    }
    fs::write(themes.join("large.toml"), vec![b'a'; 256 * 1024 + 1]).unwrap();
    fs::write(themes.join("invalid.toml"), [0xff]).unwrap();
    for id in ["large", "invalid"] {
        let error = load(&config, id).unwrap_err();
        assert!(error.contains(&format!("themes/{id}.toml")), "{error}");
        assert!(!error.contains(&dir.path().display().to_string()));
    }
    assert!(load(&config, "../outside").is_err());
    #[cfg(unix)]
    {
        let outside = dir.path().join("outside.toml");
        fs::write(&outside, "schema = 1\n").unwrap();
        std::os::unix::fs::symlink(outside, themes.join("escape.toml")).unwrap();
        assert!(load(&config, "escape").is_err());
    }
}
