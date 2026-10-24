use std::process::ExitCode;

mod bench_replay;

struct Response {
    stdout: String,
    stderr: String,
    code: u8,
}

fn respond(args: &[String]) -> Response {
    let version = env!("CARGO_PKG_VERSION");
    let (stdout, stderr, code) = match args.first().map(String::as_str) {
        Some("--version" | "-V") => (format!("talaria {version}\n"), String::new(), 0),
        Some("--help" | "-h") => (
            format!(
                "talaria {version}\n\
                 This build is a placeholder while Talaria is rewritten in Rust; it does not connect to Hermes yet.\n\
                 Options: --version, --help\n"
            ),
            String::new(),
            0,
        ),
        None => (
            String::new(),
            "talaria: this build is a placeholder and does nothing yet. Run talaria --help.\n"
                .to_string(),
            2,
        ),
        Some(argument) => {
            let shown: String = argument
                .chars()
                .take(64)
                .map(|character| {
                    if character.is_control()
                        || matches!(
                            character,
                            '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}'
                                | '\u{2066}'..='\u{2069}'
                        )
                    {
                        '?'
                    } else {
                        character
                    }
                })
                .collect();
            (
                String::new(),
                format!("talaria: unknown argument `{shown}`. Run talaria --help.\n"),
                2,
            )
        }
    };
    Response {
        stdout,
        stderr,
        code,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    match bench_replay::parse_args(&args) {
        Ok(Some(parsed)) => return bench_replay::run(parsed),
        Ok(None) => {}
        Err(error) => {
            eprintln!("talaria: {error}");
            return ExitCode::from(2);
        }
    }
    let response = respond(&args);
    print!("{}", response.stdout);
    eprint!("{}", response.stderr);
    ExitCode::from(response.code)
}

#[cfg(test)]
mod tests {
    use super::respond;

    fn call(args: &[&str]) -> super::Response {
        let owned: Vec<String> = args.iter().copied().map(str::to_string).collect();
        respond(&owned)
    }

    #[test]
    fn version_flag_prints_name_and_workspace_version() {
        let expected_stdout = format!("talaria {}\n", env!("CARGO_PKG_VERSION"));
        for args in [vec!["--version"], vec!["-V"], vec!["--version", "extra"]] {
            let got = call(&args);
            assert_eq!(got.stdout, expected_stdout, "stdout");
            assert_eq!(got.stderr, "", "stderr");
            assert_eq!(got.code, 0, "exit code");
        }
    }

    #[test]
    fn help_flag_says_this_build_is_a_placeholder() {
        let version = env!("CARGO_PKG_VERSION");
        let expected_stdout = format!(
            "talaria {version}\n\
             This build is a placeholder while Talaria is rewritten in Rust; it does not connect to Hermes yet.\n\
             Options: --version, --help\n"
        );
        for args in [vec!["--help"], vec!["-h"]] {
            let got = call(&args);
            assert_eq!(got.stdout, expected_stdout, "stdout");
            assert_eq!(got.stderr, "", "stderr");
            assert_eq!(got.code, 0, "exit code");
        }
    }

    #[test]
    fn no_arguments_exits_2_with_a_pointer_to_help() {
        let got = call(&[]);
        assert_eq!(got.code, 2, "exit code");
        assert_eq!(got.stdout, "", "stdout");
        assert_eq!(
            got.stderr,
            "talaria: this build is a placeholder and does nothing yet. Run talaria --help.\n",
            "stderr"
        );
    }

    #[test]
    fn unknown_argument_exits_2_and_neutralises_control_characters() {
        // U+001B is Unicode category Cc. U+202E is one of the twelve Bidi_Control characters.
        let got = call(&["--bo\u{1b}[31mgus\u{202e}"]);
        assert_eq!(got.code, 2, "exit code");
        assert_eq!(got.stdout, "", "stdout");
        let shown = "--bo?[31mgus?";
        let expected = format!("talaria: unknown argument `{shown}`. Run talaria --help.\n");
        assert_eq!(got.stderr, expected, "stderr");
        assert!(got.stderr.contains(shown));
        assert!(got.stderr.ends_with('\n'));
        assert_eq!(
            got.stderr
                .chars()
                .filter(|character| *character == '\n')
                .count(),
            1
        );
        assert_eq!(
            got.stderr.bytes().filter(|byte| *byte < 0x20).count(),
            1,
            "stderr has a byte below 0x20 other than the final newline"
        );

        // Exactly the twelve characters with the Unicode Bidi_Control property.
        const BIDI_CONTROL: [char; 12] = [
            '\u{061C}', '\u{200E}', '\u{200F}', '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}',
            '\u{202E}', '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
        ];
        for character in BIDI_CONTROL {
            let argument = format!("a{character}b");
            let got = call(&[argument.as_str()]);
            assert_eq!(got.code, 2, "exit code");
            assert_eq!(got.stdout, "", "stdout");
            assert_eq!(
                got.stderr, "talaria: unknown argument `a?b`. Run talaria --help.\n",
                "stderr"
            );
            assert!(
                !got.stderr.contains(character),
                "a Bidi_Control character was echoed"
            );
        }

        // Truncation is counted in Rust chars (scalar values), not bytes.
        let long = "你".repeat(200);
        let got = call(&[long.as_str()]);
        assert_eq!(got.code, 2, "exit code");
        assert_eq!(got.stdout, "", "stdout");
        let shown_long = "你".repeat(64);
        assert_eq!(
            got.stderr,
            format!("talaria: unknown argument `{shown_long}`. Run talaria --help.\n"),
            "stderr"
        );

        let got = call(&["x", "--version"]);
        assert_eq!(got.code, 2, "exit code");
        assert_eq!(got.stdout, "", "stdout");
        assert_eq!(
            got.stderr, "talaria: unknown argument `x`. Run talaria --help.\n",
            "stderr"
        );
        assert!(got.stderr.contains('x'));
    }
}
