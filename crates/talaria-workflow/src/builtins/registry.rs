use super::{Builtin, Role};
use serde_json::{Map, Value};

pub const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "fix-failing-tests",
        source: include_str!("../../workflows/fix-failing-tests.js"),
        summary: "Find the failing tests, fix each in its own worktree and verify each fix in another; nothing is merged or pushed.",
        max_agents_default: 11,
        roles: &[
            Role {
                role: "finder",
                default_tiers: &["survey"],
                list: false,
                toolsets: &["file", "terminal"],
                worktree: true,
            },
            Role {
                role: "fixer",
                default_tiers: &["builder"],
                list: false,
                toolsets: &["file", "terminal"],
                worktree: true,
            },
            Role {
                role: "verifier",
                default_tiers: &["survey"],
                list: false,
                toolsets: &["file", "terminal"],
                worktree: true,
            },
        ],
    },
    Builtin {
        name: "research",
        source: include_str!("../../workflows/research.js"),
        summary: "Answer a question from web, documentation and code sources, reading the most cited ones and removing claims no read source supports.",
        max_agents_default: 13,
        roles: &[
            Role {
                role: "planner",
                default_tiers: &["survey"],
                list: false,
                toolsets: &["todo"],
                worktree: false,
            },
            Role {
                role: "web-searcher",
                default_tiers: &["survey"],
                list: false,
                toolsets: &["web"],
                worktree: false,
            },
            Role {
                role: "code-searcher",
                default_tiers: &["survey"],
                list: false,
                toolsets: &["file"],
                worktree: true,
            },
            Role {
                role: "web-reader",
                default_tiers: &["survey"],
                list: false,
                toolsets: &["web"],
                worktree: false,
            },
            Role {
                role: "code-reader",
                default_tiers: &["survey"],
                list: false,
                toolsets: &["file"],
                worktree: true,
            },
            Role {
                role: "synthesizer",
                default_tiers: &["judge"],
                list: false,
                toolsets: &["todo"],
                worktree: false,
            },
        ],
    },
    Builtin {
        name: "review-branch",
        source: include_str!("../../workflows/review-branch.js"),
        summary: "Review a branch with one reviewer per lens, try to refute each finding, and return one ranked report.",
        max_agents_default: 17,
        roles: &[
            Role {
                role: "collector",
                default_tiers: &["survey"],
                list: false,
                toolsets: &["file", "terminal"],
                worktree: true,
            },
            Role {
                role: "reviewers",
                default_tiers: &["reviewer-1", "reviewer-2", "reviewer-3"],
                list: true,
                toolsets: &["file"],
                worktree: true,
            },
            Role {
                role: "verifier",
                default_tiers: &["judge"],
                list: false,
                toolsets: &["file"],
                worktree: true,
            },
            Role {
                role: "synthesizer",
                default_tiers: &["judge"],
                list: false,
                toolsets: &["todo"],
                worktree: false,
            },
        ],
    },
    Builtin {
        name: "ultracode",
        source: include_str!("../../workflows/ultracode.js"),
        summary: "Understand the code, compare independent designs, implement the chosen one test-first in a worktree, then review it with review-branch.",
        max_agents_default: 23,
        roles: &[
            Role {
                role: "scout",
                default_tiers: &["survey"],
                list: false,
                toolsets: &["file"],
                worktree: true,
            },
            Role {
                role: "designers",
                default_tiers: &["judge"],
                list: true,
                toolsets: &["file"],
                worktree: true,
            },
            Role {
                role: "chooser",
                default_tiers: &["judge"],
                list: false,
                toolsets: &["todo"],
                worktree: false,
            },
            Role {
                role: "implementer",
                default_tiers: &["builder"],
                list: false,
                toolsets: &["file", "terminal"],
                worktree: true,
            },
        ],
    },
];
pub fn get(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.name == name)
}
fn shown(value: &Value) -> String {
    let text = value.to_string();
    if text.encode_utf16().count() > 200 {
        let units: Vec<_> = text.encode_utf16().take(200).collect();
        format!("{}…", String::from_utf16_lossy(&units))
    } else {
        text
    }
}
fn object<'a>(value: &'a Value, path: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("'{path}' must be an object, got {}", shown(value)))
}
fn tier_name(value: &Value, key: &str) -> Result<String, String> {
    value
        .as_str()
        .filter(|s| {
            let bytes = s.as_bytes();
            !bytes.is_empty()
                && bytes.len() <= 32
                && bytes[0].is_ascii_lowercase()
                && bytes.iter().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-')
                })
        })
        .map(str::to_owned)
        .ok_or_else(|| format!("'{key}' must be a tier name, got {}", shown(value)))
}
fn lenses(args: &Map<String, Value>, prefix: &str) -> Result<usize, String> {
    let Some(v) = args.get("lenses") else {
        return Ok(3);
    };
    let allowed = [
        "correctness",
        "security",
        "simplicity",
        "performance",
        "tests",
    ];
    if let Some(list) = v.as_array()
        && (1..=5).contains(&list.len())
        && list
            .iter()
            .all(|l| l.as_str().is_some_and(|s| allowed.contains(&s)))
        && list.iter().enumerate().all(|(i, l)| !list[..i].contains(l))
    {
        return Ok(list.len());
    }
    Err(format!(
        "'{prefix}lenses' must be 1 to 5 distinct values from {}, got {}",
        allowed.join(", "),
        shown(v)
    ))
}
fn bool_arg(args: &Map<String, Value>, key: &str) -> Result<bool, String> {
    args.get(key).map_or(Ok(true), |v| {
        v.as_bool()
            .ok_or_else(|| format!("'{key}' must be true or false, got {}", shown(v)))
    })
}
fn designer_count(args: &Map<String, Value>) -> Result<usize, String> {
    match args.get("designers") {
        None => Ok(3),
        Some(v) => v
            .as_f64()
            .filter(|n| n.fract() == 0.0 && (1.0..=5.0).contains(n))
            .map(|n| n as usize)
            .ok_or_else(|| {
                format!(
                    "'designers' must be an integer from 1 to 5, got {}",
                    shown(v)
                )
            }),
    }
}
fn slots(
    builtin: &Builtin,
    args: &Map<String, Value>,
    prefix: &str,
) -> Result<Vec<(String, String)>, String> {
    let mut count = 1;
    let mut web = true;
    let mut code = true;
    if builtin.name == "review-branch" {
        count = lenses(args, prefix)?;
    }
    if builtin.name == "ultracode" {
        count = designer_count(args)?;
    }
    if builtin.name == "research" {
        code = bool_arg(args, "code")?;
        let docs = bool_arg(args, "docs")?;
        web = docs;
    }
    let tier_overrides = args
        .get("tiers")
        .map(|v| object(v, &format!("{prefix}tiers")))
        .transpose()?;
    if let Some(overrides) = tier_overrides {
        let mut allowed: Vec<_> = builtin.roles.iter().map(|r| r.role).collect();
        allowed.sort();
        for key in overrides.keys() {
            if !allowed.contains(&key.as_str()) {
                return Err(format!(
                    "unknown argument '{prefix}tiers.{key}'; allowed: {}",
                    allowed.join(", ")
                ));
            }
        }
        // Validate every override, including roles excluded by source switches.
        for key in &allowed {
            let Some(value) = overrides.get(*key) else {
                continue;
            };
            let role = builtin.roles.iter().find(|r| r.role == *key);
            if role.is_some_and(|r| r.list) {
                tier_list(value, &format!("{prefix}tiers.{key}"))?;
            } else {
                tier_name(value, &format!("{prefix}tiers.{key}"))?;
            }
        }
    }
    if builtin.name == "research" {
        web = bool_arg(args, "web")? || web;
        if !web && !code {
            return Err("at least one of web, code and docs must be true".into());
        }
    }
    let mut out = Vec::new();
    for role in builtin.roles {
        if builtin.name == "research"
            && ((!web && role.role.starts_with("web-"))
                || (!code && role.role.starts_with("code-")))
        {
            continue;
        }
        if builtin.name == "ultracode" && role.role == "chooser" && count < 2 {
            continue;
        }
        let tiers = if let Some(value) = tier_overrides.and_then(|m| m.get(role.role)) {
            if role.list {
                tier_list(value, &format!("{prefix}tiers.{}", role.role))?
            } else {
                vec![tier_name(value, &format!("{prefix}tiers.{}", role.role))?]
            }
        } else {
            role.default_tiers.iter().map(|s| s.to_string()).collect()
        };
        for i in 0..if role.list { count } else { 1 } {
            out.push((
                if role.list {
                    format!("{prefix}{}[{i}]", role.role)
                } else {
                    format!("{prefix}{}", role.role)
                },
                tiers[i % tiers.len()].clone(),
            ));
        }
    }
    Ok(out)
}
fn tier_list(v: &Value, key: &str) -> Result<Vec<String>, String> {
    let list = v
        .as_array()
        .filter(|l| (1..=5).contains(&l.len()))
        .ok_or_else(|| {
            format!(
                "'{key}' must be an array of 1 to 5 tier names, got {}",
                shown(v)
            )
        })?;
    list.iter()
        .enumerate()
        .map(|(i, t)| tier_name(t, &format!("{key}[{i}]")))
        .collect()
}
pub fn tiers_for(builtin: &Builtin, args: Option<&Value>) -> Result<Vec<(String, String)>, String> {
    let empty = Value::Object(Map::new());
    let compute = || {
        let args = object(args.unwrap_or(&empty), "args")?;
        if builtin.name == "ultracode" {
            designer_count(args)?;
        }
        // A review is checked in key order, before the outer tiers.
        let child = if builtin.name == "ultracode" {
            let review = object(args.get("review").unwrap_or(&empty), "review")?;
            if review.contains_key("base") || review.contains_key("head") {
                return Err("review.base and review.head are set by ultracode".into());
            }
            let allowed = ["focus", "lenses", "maxDiffChars", "maxFindings", "tiers"];
            for k in review.keys() {
                if !allowed.contains(&k.as_str()) {
                    return Err(format!(
                        "unknown argument 'review.{k}'; allowed: {}",
                        allowed.join(", ")
                    ));
                }
            }
            if let Some(review_builtin) = get("review-branch") {
                Some(slots(review_builtin, review, "review.")?)
            } else {
                None
            }
        } else {
            None
        };
        let mut result = slots(builtin, args, "")?;
        if let Some(child) = child {
            result.extend(child);
        }
        Ok(result)
    };
    compute().map_err(|e: String| format!("{}: {e}", builtin.name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn disabled_sources_still_validate_their_tier_overrides() {
        let builtin = get("research").unwrap();
        let error = tiers_for(
            builtin,
            Some(&json!({"web":false,"docs":false,"tiers":{"web-reader":"Bad"}})),
        )
        .unwrap_err();
        assert_eq!(
            error,
            "research: 'tiers.web-reader' must be a tier name, got \"Bad\""
        );
    }

    #[test]
    fn source_switches_and_child_slots_follow_execution_shapes() {
        let research = get("research").unwrap();
        let slots = tiers_for(research, Some(&json!({"web":false,"docs":false}))).unwrap();
        assert_eq!(
            slots.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            ["planner", "code-searcher", "code-reader", "synthesizer"]
        );
        let slots = tiers_for(get("ultracode").unwrap(), Some(&json!({"designers":1,"review":{"lenses":["tests"],"tiers":{"reviewers":["custom"]}}}))).unwrap();
        assert!(!slots.iter().any(|(k, _)| k == "chooser"));
        assert!(slots.contains(&("review.reviewers[0]".into(), "custom".into())));
        assert!(!slots.iter().any(|(k, _)| k == "review.reviewers[1]"));
    }
    #[cfg(feature = "mock")]
    #[tokio::test(flavor = "current_thread")]
    async fn routing_errors_match_script_validation_order() {
        use crate::{
            engine::{self, EngineConfig, MapResolver, RunOutcome},
            mock::MockBackend,
        };
        use std::sync::Arc;
        use tokio_util::sync::CancellationToken;
        let cases = [
            ("review-branch", json!({"tiers":null})),
            ("review-branch", json!({"lenses":[],"tiers":{"bogus":"x"}})),
            (
                "research",
                json!({"question":"q","code":false,"docs":false,"web":false,"tiers":{"planner":"Bad"}}),
            ),
            (
                "research",
                json!({"question":"q","code":false,"docs":false,"web":false}),
            ),
            ("ultracode", json!({"task":"t","designers":0,"review":null})),
            (
                "ultracode",
                json!({"task":"t","review":{"tiers":{"reviewers":[]}},"tiers":null}),
            ),
        ];
        for (name, args) in cases {
            let builtin = get(name).unwrap();
            let expected = tiers_for(builtin, Some(&args)).unwrap_err();
            let mut cfg = EngineConfig::new("validation", 0, 0);
            cfg.args = Some(args);
            let backend = Arc::new(
                MockBackend::from_json(r#"{"default":{"failed":"unexpected"},"rules":[]}"#)
                    .unwrap(),
            );
            let (tx, _) = tokio::sync::mpsc::unbounded_channel();
            let outcome = engine::run(
                engine::parse_script(builtin.source).unwrap(),
                cfg,
                backend.clone(),
                Arc::new(MapResolver::new()),
                tx,
                CancellationToken::new(),
            )
            .await;
            match outcome {
                RunOutcome::Failed { error } => assert_eq!(error.message, expected),
                other => panic!("unexpected {other:?}"),
            }
            assert!(backend.requests().is_empty());
        }
    }
    #[cfg(feature = "mock")]
    #[tokio::test(flavor = "current_thread")]
    async fn dense_backticks_in_large_diffs_do_not_exceed_the_call_argument_limit() {
        use crate::{
            engine::{self, EngineConfig, MapResolver, RunOutcome},
            mock::MockBackend,
        };
        use std::sync::Arc;
        use tokio_util::sync::CancellationToken;
        let mut rules: Value = serde_json::from_str(include_str!(
            "../../tests/data/builtins/review-branch/happy.mock.json"
        ))
        .unwrap();
        rules["rules"][0]["outcome"]["object"]["diff"] = json!("`x".repeat(90000));
        let backend = Arc::new(MockBackend::from_json(&rules.to_string()).unwrap());
        let mut cfg = EngineConfig::new("fencing", 0, 0);
        cfg.args = Some(json!({"lenses":["correctness"],"maxDiffChars":200000}));
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        let outcome = engine::run(
            engine::parse_script(get("review-branch").unwrap().source).unwrap(),
            cfg,
            backend.clone(),
            Arc::new(MapResolver::new()),
            tx,
            CancellationToken::new(),
        )
        .await;
        assert!(
            matches!(outcome, RunOutcome::Completed { .. }),
            "{outcome:?}"
        );
        let requests = backend.requests();
        let review = requests
            .iter()
            .find(|r| r.label == "review: correctness")
            .unwrap();
        assert!(review.prompt.contains("````text\n`x`x"));
    }
}
