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
    let nested = &payload["usage"];
    macro_rules! merge {
        ($field:ident, $read:ident) => {
            if let Some(value) = nested[stringify!($field)].$read() {
                usage.$field = Some(value);
            }
        };
    }
    macro_rules! alias {
        ($field:ident, $key:literal) => {
            if include_top_level && usage.$field.is_none() {
                usage.$field = payload[$key].as_i64();
            }
        };
    }
    for (target, key) in [
        (&mut usage.model, "model"),
        (&mut usage.cost_status, "cost_status"),
    ] {
        if let Some(value) = nested[key]
            .as_str()
            .filter(|value| key != "model" || !value.is_empty())
        {
            *target = Some(value.to_owned());
        }
    }
    merge!(input, as_i64);
    merge!(output, as_i64);
    merge!(reasoning, as_i64);
    merge!(prompt, as_i64);
    merge!(completion, as_i64);
    merge!(total, as_i64);
    merge!(calls, as_i64);
    merge!(context_used, as_i64);
    merge!(context_max, as_i64);
    merge!(cache_read, as_i64);
    merge!(cache_write, as_i64);
    merge!(compressions, as_i64);
    merge!(context_percent, as_f64);
    merge!(cache_hit_pct, as_f64);
    merge!(cost_usd, as_f64);
    alias!(input, "input_tokens");
    alias!(output, "output_tokens");
    alias!(reasoning, "reasoning_tokens");
    alias!(prompt, "prompt_tokens");
    alias!(completion, "completion_tokens");
    alias!(total, "total_tokens");
    alias!(calls, "api_calls");
    alias!(context_max, "context_length");
    alias!(cache_read, "cache_read_tokens");
    alias!(cache_write, "cache_write_tokens");
    alias!(compressions, "compression_count");
    if include_top_level && usage.model.is_none() {
        usage.model = payload["model"]
            .as_str()
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
    }
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
