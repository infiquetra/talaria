use super::*;

pub(crate) fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.trim().to_owned(),
        Value::Null => String::new(),
        value => value.to_string(),
    }
}

pub(crate) fn shown_value(value: &Value) -> String {
    let text = value_text(value);
    if text.is_empty() {
        "(empty)".into()
    } else {
        text
    }
}

pub fn field_visible(field: &FieldView, values: &BTreeMap<String, String>) -> bool {
    field.when.as_ref().is_none_or(|when| {
        when.iter().all(|(key, expected)| {
            values.get(key).map(String::as_str).unwrap_or("") == value_text(expected)
        })
    })
}

pub fn validate_input(field: &FieldView, text: &str) -> Result<Value, FieldError> {
    let text = text.trim();
    match &field.kind {
        FieldKind::Other(kind) => Err(FieldError::ReadOnly(kind.clone())),
        FieldKind::Bool => match text.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(Value::Bool(true)),
            "false" | "0" | "no" | "off" | "" => Ok(Value::Bool(false)),
            _ => Err(FieldError::NotAnOption),
        },
        FieldKind::Select => {
            if field.options.iter().any(|option| option.value == text) {
                Ok(Value::String(text.into()))
            } else {
                Err(FieldError::NotAnOption)
            }
        }
        FieldKind::Integer | FieldKind::Number => {
            if text.is_empty() {
                return Ok(Value::String(String::new()));
            }
            let number = if field.kind == FieldKind::Integer {
                text.parse::<i64>()
                    .map(|n| n as f64)
                    .map_err(|_| FieldError::NotWholeNumber)?
            } else {
                let number = text.parse::<f64>().map_err(|_| FieldError::NotNumber)?;
                if number.abs() > f64::MAX || number.is_nan() {
                    return Err(FieldError::NotNumber);
                }
                number
            };
            if let Some(min) = field.minimum
                && number < min
            {
                return Err(FieldError::BelowMin(min));
            }
            if let Some(max) = field.maximum
                && number > max
            {
                return Err(FieldError::AboveMax(max));
            }
            Ok(Value::String(text.into()))
        }
        FieldKind::Json => {
            if text.is_empty() {
                return Ok(Value::String(String::new()));
            }
            let parsed: Value =
                serde_json::from_str(text).map_err(|_| FieldError::NotJsonContainer)?;
            if !parsed.is_object() && !parsed.is_array() {
                return Err(FieldError::NotJsonContainer);
            }
            Ok(Value::String(text.into()))
        }
        FieldKind::Text | FieldKind::Secret => Ok(Value::String(text.into())),
    }
}

pub fn classify_saved(
    field: &FieldView,
    sent: &Value,
    before: &FieldView,
    after: &FieldView,
) -> SavedOutcome {
    if field.kind == FieldKind::Secret {
        return if !after.is_set {
            SavedOutcome::SecretNotStored
        } else if before.is_set {
            SavedOutcome::SecretSetUnconfirmed
        } else {
            SavedOutcome::SecretStored
        };
    }
    let sent = value_text(sent);
    let after_value = value_text(&after.value);
    if sent.is_empty() {
        SavedOutcome::Cleared(shown_value(&after.value))
    } else if sent == after_value {
        SavedOutcome::Saved
    } else if after_value == value_text(&before.value) {
        SavedOutcome::NotSaved
    } else {
        SavedOutcome::Adjusted(shown_value(&after.value))
    }
}

impl fmt::Display for FieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotWholeNumber => f.write_str("Enter a whole number"),
            Self::NotNumber => f.write_str("Enter a number"),
            Self::BelowMin(min) => write!(f, "Must be at least {min}"),
            Self::AboveMax(max) => write!(f, "Must be at most {max}"),
            Self::NotJsonContainer => f.write_str("Enter a JSON object or array"),
            Self::NotAnOption => f.write_str("Choose one of the listed options"),
            Self::ReadOnly(kind) => write!(f, "Talaria cannot edit this kind of setting ({kind})."),
        }
    }
}
