//! Sentences the model picker shows. Issue 230. Callers copy the same words in tests.

pub fn notice_t6(model: &str, provider: &str) -> String {
    format!(
        "Hermes could not switch to {model}: it does not know that model on {provider}, or the provider is not set up. Nothing changed."
    )
}

pub fn notice_deferred(model: &str) -> String {
    format!("Model will change to {model} when the current turn ends.")
}

pub fn notice_effort(level: &str) -> String {
    format!("Effort: {level} for this session.")
}

pub fn notice_effort_back() -> &'static str {
    "Effort: back to the profile's level for this session."
}

pub fn notice_already(model: &str, effort: Option<&str>) -> String {
    match effort {
        Some(level) => format!("Already using {model} at effort {level}; nothing changed."),
        None => format!("Already using {model}; nothing changed."),
    }
}

pub fn notice_kept(model: &str) -> String {
    format!("Kept {model}.")
}

pub fn notice_kept_pending(model: &str) -> String {
    format!(
        "Kept {model}. Hermes keeps the switch pending and will ask again the next time Talaria attaches to this session."
    )
}

pub fn notice_t13(target: &str, reason: &str, model: &str) -> String {
    format!(
        "Hermes did not switch to {target} when the turn ended ({reason}). Still using {model}."
    )
}

pub fn notice_t14(name: &str) -> String {
    format!(
        "{name} is not set up. Set it up with hermes model in a terminal, then press ctrl+r here."
    )
}

pub fn notice_t15(model: &str, name: &str) -> String {
    format!("{model} is not available on your {name} plan.")
}

pub fn notice_t16(reason: &str) -> String {
    format!("Could not load models: {reason}. Press ctrl+r to try again.")
}

pub fn notice_t18(filter: &str) -> String {
    format!("No model matches \"{filter}\".")
}

pub fn notice_t20(what: &str, reason: &str) -> String {
    format!(
        "Your saved default ({what}) was refused by Hermes ({reason}), so this session uses the profile's default. Choose a new default with /model and ctrl+g."
    )
}

pub fn notice_t21(reason: &str) -> String {
    format!(
        "~/.talaria/models.json could not be read ({reason}); favourites and saved defaults are off until it is fixed or removed."
    )
}

pub fn notice_t24(reason: &str, detail: Option<&str>) -> String {
    match detail {
        Some(detail) => format!("Hermes refused the change ({reason}). {detail}"),
        None => format!("Hermes refused the change ({reason})."),
    }
}

pub fn notice_t31_level(level: &str) -> String {
    format!("Effort will change to {level} when the current turn ends.")
}

pub fn notice_effort_held() -> &'static str {
    "Effort will change to high when the current turn ends."
}

pub fn notice_effort_held_profile() -> &'static str {
    "Effort will go back to the profile's level when the current turn ends."
}

pub fn notice_t36(model: &str, provider: &str, why: &str) -> String {
    format!(
        "Your saved default model {model} on {provider} is not available on this profile any more ({why}), so this session uses the profile's default. Choose a new default with /model and ctrl+g."
    )
}

pub fn notice_t37(model: &str, name: &str) -> String {
    format!("{model} is no longer listed by {name}. Press ctrl+f to remove it from favourites.")
}

pub fn notice_saved(profile: &str, what: &str) -> String {
    format!("Saved for new Talaria sessions on profile {profile}: {what}.")
}

pub fn notice_not_offered(option: &str) -> String {
    format!(
        "Your saved default {option} was not used: this Hermes gateway does not accept it when creating a session."
    )
}

pub fn notice_stale(model: &str, provider: &str, why: &str) -> String {
    notice_t36(model, provider, why)
}

pub fn usage_model() -> &'static str {
    "Usage: /model [<model> [--provider <id>] [--effort <level>] [--default]]"
}

pub fn usage_effort() -> &'static str {
    "Usage: /effort [none|minimal|low|medium|high|xhigh|max|ultra|default] [--default]"
}
