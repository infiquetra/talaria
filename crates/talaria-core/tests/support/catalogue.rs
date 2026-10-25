//! Catalogue samples for the model picker tests. Issue 230.

#![allow(dead_code)]

use hermes_contract::schemas::ModelOptionsResult;
use serde_json::{Value, json};

pub fn parse(value: Value) -> ModelOptionsResult {
    serde_json::from_value(value)
        .unwrap_or_else(|error| panic!("sample is not a catalogue: {error}"))
}

pub fn section_five_plus_absent() -> ModelOptionsResult {
    parse(json!({
        "model": "minimax-m3.1-flash",
        "provider": "ollama-cloud",
        "providers": [
            {
                "slug": "ollama-cloud",
                "name": "Ollama Cloud",
                "models": ["minimax-m3.1-flash", "glm-5"],
                "total_models": 2,
                "is_current": true,
                "authenticated": true,
                "capabilities": {
                    "minimax-m3.1-flash": {"fast": false, "reasoning": true},
                    "glm-5": {"fast": true, "reasoning": true}
                },
                "pricing": {
                    "minimax-m3.1-flash": {"input": "free", "output": "free", "free": true},
                    "glm-5": {"input": "$0.60", "output": "$2.20", "free": false}
                },
                "usage": {"windows": [{"label": "session", "used_percent": 42.0}]}
            },
            {
                "slug": "anthropic",
                "name": "Anthropic",
                "models": [],
                "source": "canonical",
                "authenticated": false,
                "auth_type": "api_key",
                "key_env": "ANTHROPIC_API_KEY",
                "warning": "paste ANTHROPIC_API_KEY to activate"
            },
            {
                "slug": "openrouter",
                "name": "OpenRouter",
                "models": ["open-model"],
                "source": "canonical"
            }
        ]
    }))
}

pub fn header_segments() -> ModelOptionsResult {
    parse(json!({
        "model": "a",
        "provider": "rich",
        "providers": [
            {
                "slug": "rich",
                "name": "Rich",
                "models": ["a", "b"],
                "total_models": 30,
                "is_current": true,
                "authenticated": true,
                "pricing_pending": true,
                "free_tier": true,
                "usage": {"windows": [
                    {"label": "session", "used_percent": 42.5},
                    {"label": "weekly", "used_percent": 11.2}
                ]},
                "limit": {"scope": "account", "resets_at": "2026-10-22T18:00:00+00:00"}
            },
            {
                "slug": "pool",
                "name": "Pool",
                "models": ["p"],
                "authenticated": true,
                "usage": {"accounts": [
                    {"id": "account-1", "label": "", "state": "ready"},
                    {"id": "account-2", "label": "", "state": "ready"},
                    {"id": "account-3", "label": "", "state": "limited"}
                ]}
            }
        ]
    }))
}

pub fn badges_and_prices() -> ModelOptionsResult {
    parse(json!({
        "model": "speedy",
        "provider": "ollama-cloud",
        "providers": [{
            "slug": "ollama-cloud",
            "name": "Ollama Cloud",
            "models": ["speedy", "plain", "gone", "limited", "mystery"],
            "authenticated": true,
            "is_current": true,
            "capabilities": {
                "speedy": {"fast": true, "reasoning": true},
                "plain": {"fast": false, "reasoning": false},
                "gone": {"fast": false, "reasoning": true},
                "limited": {"fast": false, "reasoning": true},
                "mystery": {"fast": false, "reasoning": true}
            },
            "pricing": {
                "speedy": {"input": "free", "output": "free", "free": true},
                "plain": {"input": "$0.60", "output": "$2.20", "free": false},
                "mystery": {"input": "?", "output": "$1.00", "free": false}
            },
            "unavailable_models": ["gone"],
            "limit": {
                "scope": "models",
                "models": {"limited": "2026-10-22T18:00:00+00:00"}
            }
        }]
    }))
}

pub fn empty_provider_current() -> ModelOptionsResult {
    parse(json!({
        "model": "glm-5",
        "provider": "",
        "providers": [
            {
                "slug": "ollama-cloud",
                "name": "Ollama Cloud",
                "models": ["minimax-m3.1-flash", "glm-5"],
                "authenticated": true,
                "is_current": true
            },
            {
                "slug": "other",
                "name": "Other",
                "models": ["glm-5"],
                "authenticated": true,
                "is_current": false
            }
        ]
    }))
}

pub fn missing_current_model() -> ModelOptionsResult {
    parse(json!({
        "model": "not-in-the-list",
        "provider": "ollama-cloud",
        "providers": [{
            "slug": "ollama-cloud",
            "name": "Ollama Cloud",
            "models": ["glm-5"],
            "authenticated": true,
            "is_current": true
        }]
    }))
}

pub fn labelled_accounts() -> ModelOptionsResult {
    parse(json!({
        "model": "glm-5",
        "provider": "ollama-cloud",
        "providers": [{
            "slug": "ollama-cloud",
            "name": "Ollama Cloud",
            "models": ["glm-5"],
            "authenticated": true,
            "usage": {"accounts": [{
                "id": "acct-1",
                "label": "Private Label One",
                "state": "ready"
            }]}
        }]
    }))
}

pub fn reasoning_false_model() -> ModelOptionsResult {
    parse(json!({
        "model": "minimax-m3.1-flash",
        "provider": "ollama-cloud",
        "providers": [{
            "slug": "ollama-cloud",
            "name": "Ollama Cloud",
            "models": ["minimax-m3.1-flash", "no-dial"],
            "total_models": 2,
            "authenticated": true,
            "is_current": true,
            "capabilities": {
                "minimax-m3.1-flash": {"fast": false, "reasoning": true},
                "no-dial": {"fast": false, "reasoning": false}
            }
        }]
    }))
}

pub fn stale_options(
    authenticated: bool,
    models: Vec<&str>,
    total: Option<i64>,
) -> ModelOptionsResult {
    let mut provider = json!({
        "slug": "ollama-cloud",
        "name": "Ollama Cloud",
        "models": models,
        "authenticated": authenticated
    });
    if let Some(total) = total {
        provider["total_models"] = json!(total);
    }
    parse(json!({
        "model": "minimax-m3.1-flash",
        "provider": "ollama-cloud",
        "providers": [provider]
    }))
}
