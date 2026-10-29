//! Judging a settled turn and rendering text and JSON output.

use serde::Serialize;
use serde_json::Value;

use crate::headless::run::StopCause;

/// The JSON status of a finished run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Completed,
    Failed,
    Unknown,
    InputRequired,
    NotStarted,
    TimedOut,
    Interrupted,
}

/// Token and cost figures, each absent unless Hermes reported it well-typed.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub api_calls: Option<u64>,
    pub estimated_cost_usd: Option<f64>,
    pub cost_status: Option<String>,
}

/// Everything one run prints and returns.
#[derive(Debug, Clone, PartialEq)]
pub struct HeadlessResult {
    pub status: RunStatus,
    pub exit_code: u8,
    pub text: String,
    pub session_id: Option<String>,
    pub profile: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub admission_id: Option<String>,
    pub duration_ms: u64,
    pub outcome: Option<String>,
    pub turn_exit_reason: Option<String>,
    pub usage: Usage,
    pub warnings: Vec<String>,
    pub error: Option<ErrorText>,
    pub resume_hint: Option<String>,
}

/// The two failure lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorText {
    pub what: String,
    pub next_step: String,
}

/// Where a stop found the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopPoint {
    BeforeSubmit,
    WhileSending,
    Withdrawn,
    AfterStart,
}

/// The outcome to judge: the completion's, then the receipt's, then the status.
pub fn effective_outcome(
    completion_outcome: Option<&str>,
    receipt_outcome: Option<&str>,
    completion_status: Option<&str>,
) -> Option<String> {
    completion_outcome
        .or(receipt_outcome)
        .map(str::to_string)
        .or_else(|| {
            completion_status.map(|status| {
                match status {
                    "complete" => "completed",
                    "interrupted" => "cancelled",
                    _ => "failed",
                }
                .to_string()
            })
        })
}

/// Judge a settled turn, or a stop, into a status and an exit code.
pub fn judge(
    stop: Option<(StopCause, StopPoint)>,
    status: Option<&str>,
    outcome: Option<&str>,
    result: Option<&Value>,
) -> (RunStatus, u8) {
    if let Some((cause, point)) = stop
        && (point != StopPoint::AfterStart
            || matches!(outcome, None | Some("cancelled" | "interrupted")))
    {
        return match cause {
            StopCause::Timeout => (RunStatus::TimedOut, 124),
            StopCause::Signal(1) => (RunStatus::Interrupted, 129),
            StopCause::Signal(15) => (RunStatus::Interrupted, 143),
            StopCause::Signal(_) => (RunStatus::Interrupted, 130),
            StopCause::Record => (RunStatus::Interrupted, 130),
            StopCause::OutputClosed => (RunStatus::Interrupted, 141),
        };
    }
    let flag = |key: &str| {
        result
            .and_then(|value| value.get(key))
            .and_then(Value::as_bool)
    };
    if flag("interrupted") == Some(true)
        || matches!(outcome, Some("cancelled" | "interrupted"))
        || (outcome.is_none() && status == Some("interrupted"))
    {
        return (RunStatus::Interrupted, 130);
    }
    if matches!(outcome, None | Some("unknown")) {
        return (RunStatus::Unknown, 1);
    }
    if flag("failed") == Some(true)
        || flag("partial") == Some(true)
        || flag("completed") == Some(false)
        || outcome != Some("completed")
    {
        return (RunStatus::Failed, 1);
    }
    (RunStatus::Completed, 0)
}

/// Read the nine usage figures from the stored turn result only.
pub fn usage_from_result(result: &Value) -> Usage {
    let counter = |key: &str| result.get(key).and_then(Value::as_u64);
    Usage {
        input_tokens: counter("input_tokens"),
        output_tokens: counter("output_tokens"),
        cache_read_tokens: counter("cache_read_tokens"),
        cache_write_tokens: counter("cache_write_tokens"),
        reasoning_tokens: counter("reasoning_tokens"),
        total_tokens: counter("total_tokens"),
        api_calls: counter("api_calls"),
        estimated_cost_usd: result
            .get("estimated_cost_usd")
            .and_then(Value::as_f64)
            .filter(|cost| cost.is_finite() && *cost >= 0.0),
        cost_status: result
            .get("cost_status")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

/// Render the one JSON line, ending with a newline.
pub fn render_json(result: &HeadlessResult) -> String {
    let status = match result.status {
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
        RunStatus::Unknown => "unknown",
        RunStatus::InputRequired => "input_required",
        RunStatus::NotStarted => "not_started",
        RunStatus::TimedOut => "timed_out",
        RunStatus::Interrupted => "interrupted",
    };
    #[derive(Serialize)]
    struct ResultLine<'a> {
        #[serde(rename = "type")]
        kind: &'static str,
        version: u8,
        status: &'static str,
        exit_code: u8,
        text: &'a str,
        session_id: &'a Option<String>,
        profile: &'a Option<String>,
        model: &'a Option<String>,
        provider: &'a Option<String>,
        admission_id: &'a Option<String>,
        duration_ms: u64,
        outcome: &'a Option<String>,
        turn_exit_reason: &'a Option<String>,
        usage: &'a Usage,
        warnings: &'a [String],
        error: &'a Option<ErrorText>,
    }
    let value = ResultLine {
        kind: "result",
        version: 1,
        status,
        exit_code: result.exit_code,
        text: &result.text,
        session_id: &result.session_id,
        profile: &result.profile,
        model: &result.model,
        provider: &result.provider,
        admission_id: &result.admission_id,
        duration_ms: result.duration_ms,
        outcome: &result.outcome,
        turn_exit_reason: &result.turn_exit_reason,
        usage: &result.usage,
        warnings: &result.warnings,
        error: &result.error,
    };
    let mut line = serde_json::to_string(&value).expect("result fields serialize to JSON");
    line.push('\n');
    line
}

/// Render the text answer, made terminal-safe only for a terminal.
pub fn render_text_stdout(result: &HeadlessResult, stdout_is_terminal: bool) -> Vec<u8> {
    let mut text = if stdout_is_terminal {
        terminal_safe(&result.text)
    } else {
        result.text.clone()
    };
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.into_bytes()
}

/// Render the failure lines and warnings for standard error.
pub fn render_stderr(result: &HeadlessResult) -> Vec<u8> {
    let mut text = String::new();
    if result.exit_code != 0 {
        if let Some(error) = &result.error {
            text.push_str(&format!(
                "talaria: {}\nNext step: {}\n",
                error.what, error.next_step
            ));
        }
        if let Some(hint) = &result.resume_hint {
            text.push_str(hint);
            if !hint.ends_with('\n') {
                text.push('\n');
            }
        }
    }
    for warning in &result.warnings {
        text.push_str(&format!("talaria: warning: {warning}\n"));
    }
    text.into_bytes()
}

/// The answer for a terminal: controls become `?`, newline and tab stay.
pub fn terminal_safe(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch != '\n' && ch != '\t' && is_control(ch) {
                '?'
            } else {
                ch
            }
        })
        .collect()
}

/// A warning or failure piece: every control becomes `?`, nothing stays.
pub fn neutralise_line(text: &str) -> String {
    text.chars()
        .map(|ch| if is_control(ch) { '?' } else { ch })
        .collect()
}

fn is_control(ch: char) -> bool {
    ch.is_control()
        || matches!(ch,
        '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

#[cfg(test)]
mod tests {
    use super::{
        ErrorText, HeadlessResult, RunStatus, StopPoint, Usage, effective_outcome, judge,
        neutralise_line, render_json, render_stderr, render_text_stdout, terminal_safe,
        usage_from_result,
    };
    use crate::headless::run::StopCause;
    use serde_json::json;

    fn result() -> HeadlessResult {
        HeadlessResult {
            status: RunStatus::Completed,
            exit_code: 0,
            text: "5555".to_string(),
            session_id: Some("local-session-1".to_string()),
            profile: Some("fixture".to_string()),
            model: Some("minimax-m3.1-flash".to_string()),
            provider: Some("ollama-cloud".to_string()),
            admission_id: Some("adm-1".to_string()),
            duration_ms: 4210,
            outcome: Some("completed".to_string()),
            turn_exit_reason: Some("text_response(stop)".to_string()),
            usage: Usage {
                input_tokens: Some(812),
                output_tokens: Some(4),
                cache_read_tokens: None,
                cache_write_tokens: None,
                reasoning_tokens: None,
                total_tokens: Some(816),
                api_calls: Some(1),
                estimated_cost_usd: Some(0.0),
                cost_status: None,
            },
            warnings: Vec::new(),
            error: None,
            resume_hint: None,
        }
    }

    #[test]
    fn judge_table() {
        // The outcome source order: completion, then receipt, then status.
        assert_eq!(
            effective_outcome(Some("completed"), Some("failed"), Some("error")).as_deref(),
            Some("completed")
        );
        assert_eq!(
            effective_outcome(None, Some("failed"), Some("complete")).as_deref(),
            Some("failed")
        );
        assert_eq!(
            effective_outcome(None, None, Some("complete")).as_deref(),
            Some("completed")
        );
        assert_eq!(
            effective_outcome(None, None, Some("interrupted")).as_deref(),
            Some("cancelled")
        );
        assert_eq!(
            effective_outcome(None, None, Some("error")).as_deref(),
            Some("failed")
        );
        assert_eq!(
            effective_outcome(None, None, Some("bogus")).as_deref(),
            Some("failed")
        );
        assert_eq!(effective_outcome(None, None, None), None);
        // A stop that withdrew the prompt wins over a later settlement.
        assert_eq!(
            judge(
                Some((StopCause::Timeout, StopPoint::Withdrawn)),
                None,
                Some("cancelled"),
                None
            ),
            (RunStatus::TimedOut, 124)
        );
        // Someone else interrupted the turn.
        assert_eq!(
            judge(None, Some("interrupted"), Some("cancelled"), None),
            (RunStatus::Interrupted, 130)
        );
        // An unreadable settlement is unknown, never a pass. Outcome wins
        // over status when they disagree.
        assert_eq!(
            judge(None, Some("error"), Some("unknown"), None),
            (RunStatus::Unknown, 1)
        );
        assert_eq!(judge(None, None, None, None), (RunStatus::Unknown, 1));
        // A failed turn, by outcome or by the stored result.
        assert_eq!(
            judge(None, Some("error"), Some("failed"), None),
            (RunStatus::Failed, 1)
        );
        assert_eq!(
            judge(
                None,
                Some("complete"),
                Some("completed"),
                Some(&json!({ "failed": true }))
            ),
            (RunStatus::Failed, 1)
        );
        assert_eq!(
            judge(
                None,
                Some("complete"),
                Some("completed"),
                Some(&json!({ "completed": false }))
            ),
            (RunStatus::Failed, 1)
        );
        // The plain pass.
        assert_eq!(
            judge(
                None,
                Some("complete"),
                Some("completed"),
                Some(&json!({ "completed": true }))
            ),
            (RunStatus::Completed, 0)
        );
    }

    #[test]
    fn json_shape_golden() {
        struct KeyOrder;
        impl<'de> serde::de::Visitor<'de> for KeyOrder {
            type Value = Vec<String>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut keys = Vec::new();
                while let Some((key, _)) = map.next_entry::<String, serde::de::IgnoredAny>()? {
                    keys.push(key);
                }
                Ok(keys)
            }
        }
        let line = render_json(&result());
        assert!(line.ends_with('\n'));
        assert_eq!(line.lines().count(), 1);
        let value: serde_json::Value = serde_json::from_str(&line).expect("one JSON object");
        let names = serde::Deserializer::deserialize_map(
            &mut serde_json::Deserializer::from_str(&line),
            KeyOrder,
        )
        .expect("an object");
        let keys: Vec<&str> = names.iter().map(String::as_str).collect();
        assert_eq!(
            keys,
            vec![
                "type",
                "version",
                "status",
                "exit_code",
                "text",
                "session_id",
                "profile",
                "model",
                "provider",
                "admission_id",
                "duration_ms",
                "outcome",
                "turn_exit_reason",
                "usage",
                "warnings",
                "error",
            ]
        );
        assert_eq!(value["type"], "result");
        assert_eq!(value["version"], 1);
        assert_eq!(value["usage"].as_object().map(|map| map.len()), Some(9));
        assert_eq!(value["error"], serde_json::Value::Null);
    }

    #[test]
    fn usage_never_defaults_to_zero() {
        fn field(usage: &Usage, key: &str) -> Option<u64> {
            match key {
                "input_tokens" => usage.input_tokens,
                "output_tokens" => usage.output_tokens,
                "cache_read_tokens" => usage.cache_read_tokens,
                "cache_write_tokens" => usage.cache_write_tokens,
                "reasoning_tokens" => usage.reasoning_tokens,
                "total_tokens" => usage.total_tokens,
                "api_calls" => usage.api_calls,
                other => panic!("no such counter: {other}"),
            }
        }
        for key in [
            "input_tokens",
            "output_tokens",
            "cache_read_tokens",
            "cache_write_tokens",
            "reasoning_tokens",
            "total_tokens",
            "api_calls",
        ] {
            for bad in [json!(null), json!("12"), json!(true), json!(-1), json!(1.5)] {
                let usage = usage_from_result(&json!({ key: bad }));
                assert_eq!(field(&usage, key), None, "{key} from {bad}");
            }
            let missing = usage_from_result(&json!({}));
            assert_eq!(field(&missing, key), None, "{key} missing");
            let good = usage_from_result(&json!({ key: 12 }));
            assert_eq!(field(&good, key), Some(12), "{key} kept");
        }
        let cost = usage_from_result(&json!({ "estimated_cost_usd": "free" }));
        assert_eq!(cost.estimated_cost_usd, None);
        let status = usage_from_result(&json!({ "cost_status": 7 }));
        assert_eq!(status.cost_status, None);
    }

    #[test]
    fn terminal_safe_only_for_terminals() {
        assert_eq!(terminal_safe("a\nb\tc\x07d\u{202e}e"), "a\nb\tc?d?e");
        let bytes = render_text_stdout(&result(), true);
        assert_eq!(bytes, b"5555\n");
        let bytes = render_text_stdout(&result(), false);
        assert_eq!(bytes, b"5555\n");
        assert_eq!(neutralise_line("a\nb\rc\td"), "a?b?c?d");
    }

    #[test]
    fn message_texts_exact() {
        // The failure lines carry the built texts unchanged.
        let mut failed = result();
        failed.status = RunStatus::NotStarted;
        failed.exit_code = 4;
        failed.text.clear();
        failed.error = Some(ErrorText {
            what: "Hermes refused the prompt (busy).".to_string(),
            next_step: "Run the command again; if it keeps failing, report the reason.".to_string(),
        });
        let stderr = render_stderr(&failed);
        assert_eq!(
            String::from_utf8_lossy(&stderr),
            "talaria: Hermes refused the prompt (busy).\n\
             Next step: Run the command again; if it keeps failing, report the reason.\n"
        );
        // A clean pass writes nothing.
        let clean = result();
        assert!(render_stderr(&clean).is_empty());
        // Warnings keep their prefix.
        let mut warned = result();
        warned
            .warnings
            .push("Hermes did not return the turn's usage figures.".to_string());
        let stderr = render_stderr(&warned);
        assert_eq!(
            String::from_utf8_lossy(&stderr),
            "talaria: warning: Hermes did not return the turn's usage figures.\n"
        );
    }
}
