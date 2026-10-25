//! Slash-line grammar. Issue #223, scenario "slash line matches the Hermes shape".

use talaria_core::commands::{SlashLine, looks_like_slash_command, parse_slash_line};

fn line(name: &str, arg: &str) -> Option<SlashLine> {
    Some(SlashLine {
        name: name.to_owned(),
        arg: arg.to_owned(),
    })
}

#[test]
fn parse_slash_line_matches_hermes_shape() {
    let cases = [
        ("/help", line("help", "")),
        ("/HELP  skills ", line("help", "skills")),
        (
            "/goal line one\n  line two",
            line("goal", "line one\n  line two"),
        ),
        ("/usr/local/bin", None),
        ("run /clean", None),
        ("//x", None),
        ("/", line("", "")),
        ("/ ", line("", "")),
        ("/ words", None),
        ("/naïve arg", line("naïve", "arg")),
        ("hello /x", None),
    ];
    for (text, expected) in cases {
        assert_eq!(parse_slash_line(text), expected, "{text:?}");
    }

    assert!(looks_like_slash_command("/help"));
    assert!(looks_like_slash_command("/"));
    assert!(looks_like_slash_command("/ "));
    assert!(looks_like_slash_command("/ words"));
    assert!(!looks_like_slash_command("/usr/local/bin"));
    assert!(!looks_like_slash_command("run /clean"));
    assert!(!looks_like_slash_command("//x"));
    assert!(!looks_like_slash_command("hello /x"));
}
