#![cfg(feature = "mock")]
mod common;
use common::*;
use proptest::{
    prelude::*,
    test_runner::{Config, TestRunner},
};
use std::{panic::catch_unwind, sync::Arc};
use talaria_workflow::engine::{self, MapResolver, ScriptError};

#[test]
fn accepts_pure_literal_meta() {
    let script = r#"/* leading */
export const meta = {
  name: 'review-demo', description: "hello \u{1F600}",
  whenToUse: `line one
line two $`,
  phases: [/* one */ {title: 'Review', detail: "carefully", model: `fixture`,},],
};
return 1;"#;
    let parsed = engine::parse_script(script).unwrap();
    assert_eq!(parsed.meta.name, "review-demo");
    assert_eq!(parsed.meta.description, "hello 😀");
    assert_eq!(
        parsed.meta.when_to_use.as_deref(),
        Some("line one\nline two $")
    );
    assert_eq!(parsed.meta.phases.len(), 1);
    assert_eq!(parsed.meta.phases[0].title, "Review");
    assert_eq!(parsed.meta.phases[0].detail.as_deref(), Some("carefully"));
    assert_eq!(parsed.meta.phases[0].model.as_deref(), Some("fixture"));
    assert!(parsed.warnings.is_empty());
    assert_eq!(parsed.body.lines().count(), script.lines().count());
    assert!(parsed.body.ends_with("return 1;"));
}

#[test]
fn refuses_non_literal_meta() {
    let cases = [
        (
            "export const meta = {name: N, description: 'x'};",
            "N",
            "identifier",
        ),
        (
            "export const meta = {name: fn(), description: 'x'};",
            "fn",
            "call",
        ),
        (
            "export const meta = {name: 'x', ...other};",
            "...",
            "spread",
        ),
        (
            "export const meta = {name: `x${y}`, description: 'x'};",
            "${",
            "template",
        ),
        ("export const meta = {[x]: 1};", "[x]", "computed"),
        ("export const meta = {name: 'x', n: 01};", "01", "01"),
        ("export const meta = {name: 'x', n: .5};", ".5", ".5"),
        (
            "export const meta = {name: 'broken",
            "'broken",
            "unterminated",
        ),
        (
            "export const meta = {name: \"bad\nstring\"};",
            "\"bad",
            "string",
        ),
        (
            "export const meta = {name:'a',description:'b'} + x",
            "+ x",
            "expression continues",
        ),
        (
            "export const meta = {name:'a',description:'b'}\n+ x",
            "+ x",
            "expression continues",
        ),
    ];
    for (script, marker, expected) in cases {
        let offset = script.find(marker).unwrap();
        let prefix = &script[..offset];
        let line = 1 + prefix.chars().filter(|c| *c == '\n').count() as u32;
        let column = 1 + prefix.rsplit('\n').next().unwrap().chars().count() as u32;
        match engine::parse_script(script).unwrap_err() {
            ScriptError::MetaNotLiteral {
                line: actual_line,
                column: actual_col,
                found,
            } => {
                assert_eq!((actual_line, actual_col), (line, column), "{script}");
                assert!(found.contains(expected), "{found:?} in {script}");
            }
            other => panic!("wrong error for {script}: {other:?}"),
        }
    }
    for (script, line) in [
        ("return 1;", 1),
        ("\nconst x = 1;\nexport const meta = {};", 2),
        ("export let meta = {};", 1),
        ("\n/* never closed", 2),
    ] {
        assert_eq!(
            engine::parse_script(script).unwrap_err(),
            ScriptError::MetaMissing { line }
        );
    }
}

#[test]
fn required_fields_and_types() {
    let bad = [
        ("{description:'x'}".to_owned(), "name"),
        ("{name:'.bad',description:'x'}".to_owned(), "name"),
        ("{name:'bad name',description:'x'}".to_owned(), "name"),
        (
            format!("{{name:'{}',description:'x'}}", "a".repeat(65)),
            "name",
        ),
        ("{name:'x',description:' '}".to_owned(), "description"),
        ("{name:'x',description:'x',phases:1}".to_owned(), "phases"),
        (
            "{name:'x',description:'x',phases:[{title:'x'},{title:'x'}]}".to_owned(),
            "phases",
        ),
        ("{name:'x',name:'y',description:'x'}".to_owned(), "name"),
        (
            "{name:'x',description:'x',phases:[{title:'x',title:'y'}]}".to_owned(),
            "title",
        ),
    ];
    for (header, field) in bad {
        match engine::parse_script(&format!("export const meta = {header};")) {
            Err(ScriptError::MetaField { field: actual, .. }) => {
                assert!(actual.contains(field), "{actual}")
            }
            other => panic!("expected field error: {other:?}"),
        }
    }
    let parsed = engine::parse_script(
        "export const meta = {name:'x',description:'x',extra:1,phases:[{title:'A',extra:true}]};",
    )
    .unwrap();
    assert_eq!(parsed.warnings.len(), 2);
    assert!(
        parsed
            .warnings
            .iter()
            .any(|warning| warning.contains("extra"))
    );
    assert!(
        parsed
            .warnings
            .iter()
            .any(|warning| warning.contains("phases[0].extra"))
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn body_keeps_original_line_numbers() {
    let resolver = Arc::new(MapResolver::new());
    let script = "export const meta = {\n name:'a',\n description:'b',\n phases: [],\n whenToUse:'x'\n};\n\n\nthrow new Error('x');";
    let (outcome, _) = execute(script, config(), default_backend(), resolver.clone()).await;
    assert_eq!(failure(outcome).line, Some(9));
    let script = "export const meta = {\n name:'a',\n description:'b',\n phases: [],\n whenToUse:'x'\n};\n\n\n\n\nconst = ;";
    let (outcome, _) = execute(script, config(), default_backend(), resolver.clone()).await;
    let error = failure(outcome);
    assert_eq!(error.name, "SyntaxError");
    assert_eq!(error.line, Some(11));
    for script in [
        "export const meta = {name:'a',description:'b'}; throw new Error('x')",
        "export const meta = {name:'a',description:'b'}; 'é'; throw new Error('x')",
    ] {
        let (outcome, _) = execute(script, config(), default_backend(), resolver.clone()).await;
        let error = failure(outcome);
        assert_eq!(error.line, Some(1));
        if let Some(column) = error.column {
            let expected = script[..script.find("throw").unwrap()].chars().count() as u32 + 1;
            assert_eq!(column, expected);
        }
    }
}

#[test]
fn parser_never_panics() {
    let mut runner = TestRunner::new(Config {
        cases: 2_000,
        max_shrink_iters: 0,
        failure_persistence: None,
        ..Config::default()
    });
    runner
        .run(&any::<String>(), |s| {
            prop_assert!(catch_unwind(|| engine::parse_script(&s)).is_ok());
            Ok(())
        })
        .unwrap();
    let mut runner = TestRunner::new(Config {
        cases: 2_000,
        max_shrink_iters: 0,
        failure_persistence: None,
        ..Config::default()
    });
    runner
        .run(
            &(0usize..HEADER.len(), any::<String>()),
            |(index, insert)| {
                let mut script = HEADER.to_owned();
                script.insert_str(index, &insert);
                prop_assert!(catch_unwind(|| engine::parse_script(&script)).is_ok());
                Ok(())
            },
        )
        .unwrap();
}
