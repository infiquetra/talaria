//! Red-run test 7 (#231): export arguments, names and sizes.

use talaria_core::directory::export::{
    ExportArgError, ExportArgs, ExportFormat, REDACT_BY_DEFAULT, candidate_names, format_size,
    parse_export_args, safe_file_name,
};

#[test]
fn args_names_and_sizes() {
    assert_eq!(
        parse_export_args(""),
        Ok(ExportArgs {
            path: None,
            redact: true
        })
    );
    assert_eq!(
        parse_export_args("--no-redact"),
        Ok(ExportArgs {
            path: None,
            redact: false
        })
    );
    assert_eq!(
        parse_export_args("notes.md"),
        Ok(ExportArgs {
            path: Some("notes.md".to_owned()),
            redact: true
        })
    );
    assert_eq!(
        parse_export_args("--no-redact notes.md"),
        Ok(ExportArgs {
            path: Some("notes.md".to_owned()),
            redact: false
        })
    );
    assert_eq!(
        parse_export_args("notes.md --no-redact"),
        Ok(ExportArgs {
            path: Some("notes.md".to_owned()),
            redact: false
        })
    );
    assert_eq!(
        parse_export_args("--bogus"),
        Err(ExportArgError::UnknownOption("--bogus".to_owned()))
    );
    assert_eq!(
        parse_export_args("a b.md"),
        Err(ExportArgError::TooManyPaths)
    );
    assert_eq!(
        parse_export_args("notes.txt"),
        Err(ExportArgError::BadExtension)
    );
    // The operator's path is used as typed.
    for path in ["../notes.md", "~/notes.md", ".notes.md"] {
        assert_eq!(
            parse_export_args(path),
            Ok(ExportArgs {
                path: Some(path.to_owned()),
                redact: true
            }),
            "path {path}"
        );
    }
    assert_eq!(parse_export_args(".md"), Err(ExportArgError::BadExtension));

    assert_eq!(ExportFormat::Markdown.wire(), "md");
    assert_eq!(ExportFormat::Json.wire(), "json");
    assert_eq!(ExportFormat::Html.wire(), "html");
    assert_eq!(ExportFormat::Markdown.extension(), "md");
    assert_eq!(ExportFormat::Markdown.label(), "Markdown");
    assert_eq!(ExportFormat::Json.label(), "JSON");
    assert_eq!(ExportFormat::Html.label(), "HTML");
    assert_eq!(
        ExportFormat::from_extension("MD"),
        Some(ExportFormat::Markdown)
    );
    assert_eq!(
        ExportFormat::from_extension("json"),
        Some(ExportFormat::Json)
    );
    assert_eq!(
        ExportFormat::from_extension("html"),
        Some(ExportFormat::Html)
    );
    assert_eq!(ExportFormat::from_extension("markdown"), None);

    // Only Hermes's file name is reduced to a safe base name.
    assert_eq!(
        safe_file_name("hermes_session_abc.md", ExportFormat::Markdown, "s-1"),
        "hermes_session_abc.md"
    );
    assert_eq!(
        safe_file_name("../../x.md", ExportFormat::Markdown, "s-1"),
        "x.md"
    );
    assert_eq!(
        safe_file_name("a\\b.md", ExportFormat::Markdown, "s-1"),
        "b.md"
    );
    for bad in [".hidden.md", "x.txt", ""] {
        assert_eq!(
            safe_file_name(bad, ExportFormat::Markdown, "s-1_2"),
            "hermes_session_s-1_2.md",
            "name {bad:?}"
        );
    }

    let names: Vec<String> = candidate_names("a.md").collect();
    assert_eq!(names.len(), 99);
    assert_eq!(names[0], "a.md");
    assert_eq!(names[1], "a (2).md");
    assert_eq!(names[98], "a (99).md");

    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(1_023), "1023 B");
    assert_eq!(format_size(1_024), "1.0 KiB");
    assert_eq!(format_size(8_704), "8.5 KiB");
    assert_eq!(format_size(1_048_576), "1.0 MiB");
    assert_eq!(format_size(8_912_896), "8.5 MiB");

    const {
        assert!(REDACT_BY_DEFAULT);
    }
}
