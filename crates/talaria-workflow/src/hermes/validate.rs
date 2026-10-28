//! Deterministic extraction and bounded JSON Schema validation errors.
use serde_json::Value;

pub fn extract_json_candidate(text: &str) -> &str {
    let mut raw = text.trim();
    if raw.starts_with("```") {
        raw = raw.split_once('\n').map_or(raw, |(_, s)| s);
        raw = raw.trim_end();
        if let Some(s) = raw.strip_suffix("```") {
            raw = s;
        }
        raw = raw.trim();
        if raw
            .get(..5)
            .is_some_and(|s| s.eq_ignore_ascii_case("json\n"))
        {
            raw = &raw[5..];
        }
    }
    let mut spans = Vec::new();
    for (open, close) in [('{', '}'), ('[', ']')] {
        if let (Some(start), Some(end)) = (raw.find(open), raw.rfind(close))
            && start < end
        {
            spans.push((start, &raw[start..=end]));
        }
    }
    let mut ordered = spans.clone();
    ordered.sort_by_key(|(start, _)| *start);
    for (_, candidate) in ordered {
        if serde_json::from_str::<Value>(candidate).is_ok() {
            return candidate;
        }
    }
    spans.first().map_or(raw, |(_, span)| *span)
}
fn validator(schema: &Value) -> Result<jsonschema::Validator, String> {
    let options = if schema.get("$schema").is_none() {
        jsonschema::options().with_draft(jsonschema::Draft::Draft202012)
    } else {
        jsonschema::options()
    };
    options
        .build(schema)
        .map_err(|e| format!("schema does not compile: {e}"))
}
pub fn check_schema(schema: &Value) -> Result<(), String> {
    validator(schema).map(|_| ())
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Segment {
    Name(String),
    Index(u64),
}
pub fn validate_output(text: &str, schema: &Value) -> Result<Value, Vec<String>> {
    let candidate = extract_json_candidate(text);
    if candidate.trim().is_empty() {
        return Err(vec![
            "Response was empty — expected a JSON object matching the schema.".into(),
        ]);
    }
    let value: Value = serde_json::from_str(candidate)
        .map_err(|e| vec![format!("Response is not valid JSON: {e}")])?;
    let validator = validator(schema).map_err(|e| vec![e])?;
    let mut errors = Vec::new();
    for error in validator.iter_errors(&value) {
        let pointer = error.instance_path().to_string();
        let mut node = &value;
        let mut path = String::from("$");
        let mut segments = Vec::new();
        for token in pointer.split('/').skip(1) {
            let key = token.replace("~1", "/").replace("~0", "~");
            if node.is_array()
                && let Ok(index) = key.parse::<usize>()
            {
                segments.push(Segment::Index(index as u64));
                path.push_str(&format!("[{index}]"));
                node = node.get(index).unwrap_or(&Value::Null);
            } else {
                segments.push(Segment::Name(key.clone()));
                path.push('.');
                path.push_str(&key);
                node = node.get(&key).unwrap_or(&Value::Null);
            }
        }
        errors.push((segments, format!("{path}: {error}")));
    }
    errors.sort_by(|a, b| a.0.cmp(&b.0));
    if errors.is_empty() {
        Ok(value)
    } else {
        Err(errors.into_iter().take(10).map(|(_, text)| text).collect())
    }
}
