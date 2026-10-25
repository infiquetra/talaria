//! Saved favourites and per-profile defaults.

use serde_json::{Value, json};
use std::collections::BTreeMap;

use hermes_contract::schemas::ModelOptionsResult;

use super::effort::Effort;
use super::picker::{EffortCommand, ModelCommand};

/// One profile's saved model choice. Each field may be absent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SavedDefault {
    pub model: Option<String>,
    pub provider: Option<String>,
    pub effort: Option<Effort>,
}

/// The on-disk file. Unknown top-level keys are dropped on the next write.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SavedModels {
    pub favourites: Vec<String>,
    pub defaults: BTreeMap<String, SavedDefault>,
}

impl SavedModels {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Decode saved choices without reading or changing a file.
    pub fn from_json(text: &str) -> Result<Self, String> {
        fn decode(text: &str) -> Result<SavedModels, String> {
            let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
            let object = value.as_object().ok_or("expected an object")?;
            if object.get("version").and_then(Value::as_u64) != Some(1) {
                return Err("unsupported version".into());
            }
            let mut saved = SavedModels::empty();
            if let Some(favourites) = object.get("favourites") {
                for key in favourites.as_array().ok_or("favourites must be a list")? {
                    let key = key.as_str().ok_or("favourites must contain strings")?;
                    saved.favourites.push(key.into());
                }
            }
            if let Some(defaults) = object.get("defaults") {
                for (profile, value) in defaults.as_object().ok_or("defaults must be an object")? {
                    let fields = value.as_object().ok_or("a default must be an object")?;
                    let string = |key| -> Result<Option<String>, String> {
                        fields
                            .get(key)
                            .map(|v| {
                                v.as_str()
                                    .map(String::from)
                                    .ok_or_else(|| format!("{key} must be a string"))
                            })
                            .transpose()
                    };
                    let effort = string("effort")?
                        .map(|level| Effort::parse(&level).ok_or("invalid effort"))
                        .transpose()?;
                    saved.defaults.insert(
                        profile.clone(),
                        SavedDefault {
                            model: string("model")?,
                            provider: string("provider")?,
                            effort,
                        },
                    );
                }
            }
            Ok(saved)
        }
        decode(text).map_err(|error| super::text::notice_t21(&error))
    }

    /// Encode only the supported fields; persistence belongs to the application.
    pub fn to_json(&self) -> Result<String, String> {
        let defaults: serde_json::Map<String, Value> = self
            .defaults
            .iter()
            .map(|(profile, saved)| {
                let mut fields = serde_json::Map::new();
                if let Some(model) = &saved.model {
                    fields.insert("model".into(), json!(model));
                }
                if let Some(provider) = &saved.provider {
                    fields.insert("provider".into(), json!(provider));
                }
                if let Some(effort) = saved.effort {
                    fields.insert("effort".into(), json!(effort.as_wire()));
                }
                (profile.clone(), Value::Object(fields))
            })
            .collect();
        serde_json::to_string(
            &json!({"version": 1, "favourites": self.favourites, "defaults": defaults}),
        )
        .map_err(|e| e.to_string())
    }
}

/// Why a saved model and provider were dropped together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaleReason {
    ProviderNotSetUp,
    ModelNotListed,
    ListUnreadable,
}

/// The stale check's answer. The effort is never dropped here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavedCheck {
    Keep,
    DropModelAndProvider(StaleReason),
}

pub fn check_saved_default(
    saved: &SavedDefault,
    options: Option<&ModelOptionsResult>,
) -> SavedCheck {
    if saved.model.is_none() && saved.provider.is_none() {
        return SavedCheck::Keep;
    }
    let Some(options) = options else {
        return SavedCheck::DropModelAndProvider(StaleReason::ListUnreadable);
    };
    let Some(provider) = options
        .providers
        .iter()
        .find(|p| Some(&p.slug) == saved.provider.as_ref() && p.authenticated == Some(true))
    else {
        return SavedCheck::DropModelAndProvider(StaleReason::ProviderNotSetUp);
    };
    if let Some(model) = &saved.model
        && !provider.models.contains(model)
        && provider
            .total_models
            .is_none_or(|total| total <= provider.models.len() as i64)
    {
        return SavedCheck::DropModelAndProvider(StaleReason::ModelNotListed);
    }
    SavedCheck::Keep
}

/// Parse the words after `/model`. The error text is the usage line.
pub fn parse_model_command(arg: &str) -> Result<ModelCommand, String> {
    let words: Vec<_> = arg.split_whitespace().collect();
    let Some(model) = words.first() else {
        return Ok(ModelCommand::OpenPicker);
    };
    let fail = || super::text::usage_model().to_string();
    if model.starts_with("--") {
        return Err(fail());
    }
    let (mut provider, mut effort, mut save_default) = (None, None, false);
    let mut index = 1;
    while index < words.len() {
        match words[index] {
            "--default" if !save_default => {
                save_default = true;
                index += 1;
            }
            flag @ ("--provider" | "--effort") => {
                let value = words
                    .get(index + 1)
                    .filter(|word| !word.starts_with("--"))
                    .ok_or_else(fail)?;
                if flag == "--provider" && provider.is_none() {
                    provider = Some((*value).into());
                } else if flag == "--effort" && effort.is_none() {
                    effort = Some(Effort::parse(value).ok_or_else(fail)?);
                } else {
                    return Err(fail());
                }
                index += 2;
            }
            _ => return Err(fail()),
        }
    }
    Ok(ModelCommand::Switch {
        model: (*model).into(),
        provider,
        effort,
        save_default,
    })
}

/// Parse the words after `/effort`. The error text is the usage line.
pub fn parse_effort_command(arg: &str) -> Result<EffortCommand, String> {
    let words: Vec<_> = arg.split_whitespace().collect();
    if words.is_empty() {
        return Ok(EffortCommand::Show);
    }
    let fail = || super::text::usage_effort().to_string();
    let level = if words[0].eq_ignore_ascii_case("default") {
        None
    } else {
        Some(Effort::parse(words[0]).ok_or_else(fail)?)
    };
    if words.len() > 2 || (words.len() == 2 && words[1] != "--default") {
        return Err(fail());
    }
    Ok(EffortCommand::Set {
        level,
        save_default: words.len() == 2,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_choices_round_trip_without_retaining_unknown_fields() {
        let saved = SavedModels::from_json(r#"{"version":1,"favourites":["p::m"],"defaults":{"fixture":{"model":"m","provider":"p","effort":"high"}},"extra":true}"#).expect("valid choices");
        let text = saved.to_json().expect("encode");
        assert!(!text.contains("extra"));
        assert_eq!(SavedModels::from_json(&text), Ok(saved));
    }

    #[test]
    fn saved_choices_reject_wrong_types_and_bad_efforts() {
        for text in [
            "[1]",
            "{",
            r#"{"version":2}"#,
            r#"{"version":1,"favourites":[1]}"#,
            r#"{"version":1,"defaults":[]}"#,
            r#"{"version":1,"defaults":{"fixture":{"model":1}}}"#,
            r#"{"version":1,"defaults":{"fixture":{"effort":"off"}}}"#,
        ] {
            let error = SavedModels::from_json(text).expect_err("invalid choices");
            assert!(error.starts_with("~/.talaria/models.json could not be read ("));
            assert!(error.ends_with(
                "; favourites and saved defaults are off until it is fixed or removed."
            ));
        }
    }
}
