//! New profile name checks. Issue #253.

use talaria_core::profiles::names::{NameError, validate_new_name};

fn known() -> Vec<String> {
    ["default", "fixture", "fixture-a"]
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn validate_new_name_rules() {
    // Empty, in any clothing, loses first.
    for input in ["", "   ", "\t\n "] {
        assert_eq!(
            validate_new_name(input, &known()),
            Err(NameError::Empty),
            "{input:?}"
        );
    }
    assert_eq!(
        validate_new_name("", &known()).unwrap_err().to_string(),
        "Enter a name."
    );

    // Shape is checked before reserved and known.
    for input in [
        "-lead",
        "_lead",
        "Bad Name!",
        "upper with space",
        "has space",
        "dot.name",
        "slash/name",
        "colon:name",
        &"a".repeat(65),
    ] {
        assert_eq!(
            validate_new_name(input, &known()),
            Err(NameError::BadShape),
            "{input:?}"
        );
    }
    // A reserved word in bad shape still reports the shape.
    assert_eq!(
        validate_new_name("Default!", &known()),
        Err(NameError::BadShape)
    );
    assert_eq!(
        validate_new_name("-", &known()).unwrap_err().to_string(),
        "Use lower-case letters, digits, - or _, starting with a letter or digit, up to 64 characters."
    );

    // Length is counted after trimming, on the normalised name.
    assert_eq!(
        validate_new_name(&format!("  {}  ", "b".repeat(64)), &[]),
        Ok("b".repeat(64))
    );
    assert_eq!(
        validate_new_name(&"c".repeat(65), &[]),
        Err(NameError::BadShape)
    );

    // Reserved wins over known: `default` is known and still reserved.
    for word in ["hermes", "default", "test", "tmp", "root", "sudo"] {
        assert_eq!(
            validate_new_name(word, &known()),
            Err(NameError::Reserved(word.to_owned())),
            "{word}"
        );
        assert_eq!(
            validate_new_name(&format!(" {word} "), &known()),
            Err(NameError::Reserved(word.to_owned())),
            "{word} with spaces"
        );
        assert_eq!(
            validate_new_name(&word.to_uppercase(), &known()),
            Err(NameError::Reserved(word.to_owned())),
            "{word} in upper case"
        );
    }
    assert_eq!(
        validate_new_name("default", &known())
            .unwrap_err()
            .to_string(),
        "default is reserved by Hermes. Choose another name."
    );

    // Known names compare after normalisation.
    for input in ["fixture", " Fixture ", "FIXTURE", "fixture-a"] {
        assert_eq!(
            validate_new_name(input, &known()),
            Err(NameError::Exists(input.trim().to_lowercase())),
            "{input:?}"
        );
    }
    assert_eq!(
        validate_new_name(" Fixture ", &known())
            .unwrap_err()
            .to_string(),
        "A profile named fixture already exists."
    );

    // A fresh name normalises.
    assert_eq!(
        validate_new_name(" Fixture-New_1 ", &known()),
        Ok("fixture-new_1".to_owned())
    );
    assert_eq!(validate_new_name("a", &[]), Ok("a".to_owned()));
    assert_eq!(validate_new_name("0", &[]), Ok("0".to_owned()));
    assert_eq!(validate_new_name("a-b_c9", &[]), Ok("a-b_c9".to_owned()));
}
