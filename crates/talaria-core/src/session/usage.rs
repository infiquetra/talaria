//! Field-by-field usage merge. Absent and null leave the stored value. Zero is a reading.
//! A top-level counter fills a field only while that field is still empty, and only for the
//! four names the nested view does not always carry: `cache_read`, `cache_write`,
//! `compressions` and `context_max`.

use serde_json::Value;

use super::Usage;

/// Merge one `session.usage` or `message.complete` payload into `usage`.
///
/// `include_top_level` is true only for `session.usage`. The nested object is `payload["usage"]`
/// when it is an object.
pub fn merge_usage(usage: &mut Usage, payload: &Value, include_top_level: bool) {
    let _ = (usage, payload, include_top_level);
    todo!("P1-05")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::merge_usage;
    use crate::session::Usage;

    #[test]
    fn nested_zero_is_stored_and_null_is_not() {
        let mut usage = Usage {
            input: Some(5),
            ..Usage::default()
        };
        merge_usage(
            &mut usage,
            &json!({"usage": {"input": 0, "output": null, "model": ""}}),
            false,
        );
        assert_eq!(usage.input, Some(0));
        assert_eq!(usage.output, None);
        assert_eq!(usage.model, None);
    }

    #[test]
    fn top_level_cache_read_fills_only_while_empty() {
        let mut usage = Usage::default();
        merge_usage(
            &mut usage,
            &json!({"usage": {"model": "m", "input": 1}, "cache_read_tokens": 7}),
            true,
        );
        assert_eq!(usage.cache_read, Some(7));
        merge_usage(
            &mut usage,
            &json!({"usage": {"model": "m", "input": 2}, "cache_read_tokens": 1}),
            true,
        );
        assert_eq!(usage.cache_read, Some(7));
        assert_eq!(usage.input, Some(2));
    }
}
