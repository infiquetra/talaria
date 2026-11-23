//! Compile a workflow body without running it, so a syntax error is found before a run starts.

use rquickjs::{AsyncContext, AsyncRuntime, context::EvalOptions};

use super::{ParsedScript, runtime::ROOT_PREFIX};

/// A problem the JavaScript compiler found in a script body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxIssue {
    pub message: String,
    /// One based, in the script file as the author wrote it.
    pub line: Option<u32>,
    /// One based, in the script file as the author wrote it.
    pub column: Option<u32>,
}

/// Compile the script body inside the same function wrapper a run uses, without calling it.
///
/// Nothing in the script runs: the wrapper is only evaluated to a function value. Returns
/// `Ok(())` when the body compiles, and the compiler's own message and position otherwise.
pub async fn check_syntax(parsed: &ParsedScript) -> Result<(), SyntaxIssue> {
    let filename = format!("{}.js", parsed.meta.name);
    let source = format!("{ROOT_PREFIX}{}\n}})", parsed.body);
    let issue = |message: String| SyntaxIssue {
        message,
        line: None,
        column: None,
    };
    let runtime = AsyncRuntime::new().map_err(|e| issue(e.to_string()))?;
    runtime.set_memory_limit(256 * 1024 * 1024).await;
    let context = AsyncContext::full(&runtime)
        .await
        .map_err(|e| issue(e.to_string()))?;
    let name = filename.clone();
    context
        .with(move |ctx| {
            let mut options = EvalOptions::default();
            options.filename = Some(name);
            match ctx.eval_with_options::<rquickjs::Value, _>(source, options) {
                Ok(_) => Ok(()),
                Err(error) => {
                    if !error.is_exception() {
                        return Err(issue(error.to_string()));
                    }
                    let caught = ctx.catch();
                    let object = caught.as_object();
                    let message = object
                        .and_then(|o| o.get::<_, String>("message").ok())
                        .unwrap_or_else(|| "script does not compile".into());
                    let stack = object
                        .and_then(|o| o.get::<_, String>("stack").ok())
                        .unwrap_or_default();
                    let (line, column) = position(&stack, &filename);
                    // The wrapper's first line is also the script's first line, so its
                    // length shifts columns on that line only.
                    let column = match (line, column) {
                        (Some(1), Some(c)) => {
                            Some(c.saturating_sub(ROOT_PREFIX.chars().count() as u32).max(1))
                        }
                        (_, c) => c,
                    };
                    Err(SyntaxIssue {
                        message,
                        line,
                        column,
                    })
                }
            }
        })
        .await
}

/// The `file:line:column` position QuickJS prints in a stack trace for `filename`.
fn position(stack: &str, filename: &str) -> (Option<u32>, Option<u32>) {
    let marker = format!("{filename}:");
    let Some(at) = stack.find(&marker) else {
        return (None, None);
    };
    let rest = &stack[at + marker.len()..];
    let digits = |text: &str| -> (Option<u32>, usize) {
        let length = text.chars().take_while(char::is_ascii_digit).count();
        (text[..length].parse().ok(), length)
    };
    let (line, length) = digits(rest);
    match rest[length..].strip_prefix(':') {
        Some(after) => (line, digits(after).0),
        None => (line, None),
    }
}
