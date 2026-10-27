use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    Proposal(Proposal),
    Need(Need),
    Answer(String),
    Cannot(String),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub summary: String,
    pub profile: Option<String>,
    pub changes: Vec<Change>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Need {
    pub sections: Vec<String>,
    pub why: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplyError {
    NoBlock,
    SeveralBlocks(usize),
    TooLarge(usize),
    Json(String),
    NotAnObject,
    UnknownKind(String),
    MissingField(&'static str),
    TooManyChanges(usize),
}
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Set {
        key: String,
        value: Value,
        why: Option<String>,
    },
    McpAdd {
        name: String,
        source: McpSource,
        why: Option<String>,
    },
    McpEnable {
        name: String,
        enabled: bool,
        why: Option<String>,
    },
    McpRemove {
        name: String,
        why: Option<String>,
    },
    Toggle {
        kind: CapKind,
        name: String,
        enabled: bool,
        why: Option<String>,
    },
    CronAdd {
        name: String,
        schedule: String,
        prompt: String,
        repeat: Option<u32>,
        deliver: Option<String>,
        why: Option<String>,
    },
    CronPause {
        job_id: String,
        why: Option<String>,
    },
    CronResume {
        job_id: String,
        why: Option<String>,
    },
    CronRemove {
        job_id: String,
        why: Option<String>,
    },
    MainModel {
        provider: String,
        model: String,
        why: Option<String>,
    },
    Open {
        screen: Screen,
        target: Option<String>,
        why: Option<String>,
    },
    Malformed {
        op: String,
        field: &'static str,
    },
    Unknown {
        op: String,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum McpSource {
    Preset(String),
    Command { command: String, args: Vec<String> },
    Url { url: String, oauth: bool },
    Forbidden(&'static str),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapKind {
    Skill,
    Toolset,
    Plugin,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Providers,
    Mcp,
    Capabilities,
    Cron,
    Profiles,
    Connectors,
    Settings,
}

fn blocks(text: &str) -> Vec<(usize, usize, &str)> {
    let mut result = Vec::new();
    let mut opening = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let fence = line.trim_end_matches('\n').trim_matches([' ', '\t', '\r']);
        if let Some((start, content)) = opening {
            if fence == "```" {
                result.push((start, offset + line.len(), &text[content..offset]));
                opening = None;
            }
        } else if fence == "```talaria-configure" {
            opening = Some((offset, offset + line.len()));
        }
        offset += line.len();
    }
    result
}

pub fn parse_reply(text: &str) -> Result<Reply, ReplyError> {
    let found = blocks(text);
    if found.is_empty() {
        return Err(ReplyError::NoBlock);
    }
    if found.len() != 1 {
        return Err(ReplyError::SeveralBlocks(found.len()));
    }
    let content = found[0].2;
    if content.len() > super::REPLY_BLOCK_MAX_BYTES {
        return Err(ReplyError::TooLarge(content.len()));
    }
    let value: Value =
        serde_json::from_str(content).map_err(|error| ReplyError::Json(error.to_string()))?;
    let obj = value.as_object().ok_or(ReplyError::NotAnObject)?;
    let string = |field| {
        obj.get(field)
            .and_then(Value::as_str)
            .ok_or(ReplyError::MissingField(field))
    };
    let optional = |field| -> Result<Option<String>, ReplyError> {
        obj.get(field)
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or(ReplyError::MissingField(field))
            })
            .transpose()
    };
    Ok(match string("kind")? {
        "proposal" => {
            let summary = string("summary")?.to_owned();
            let profile = optional("profile")?;
            let changes = obj
                .get("changes")
                .and_then(Value::as_array)
                .ok_or(ReplyError::MissingField("changes"))?;
            if changes.is_empty() {
                return Err(ReplyError::MissingField("changes"));
            }
            if changes.len() > super::MAX_CHANGES {
                return Err(ReplyError::TooManyChanges(changes.len()));
            }
            Reply::Proposal(Proposal {
                summary,
                profile,
                changes: changes.iter().map(parse_change).collect(),
            })
        }
        "need" => {
            let sections = obj
                .get("sections")
                .and_then(Value::as_array)
                .and_then(|v| v.iter().map(|s| s.as_str().map(str::to_owned)).collect())
                .ok_or(ReplyError::MissingField("sections"))?;
            Reply::Need(Need {
                sections,
                why: optional("why")?,
            })
        }
        "answer" => Reply::Answer(string("text")?.into()),
        "cannot" => Reply::Cannot(string("reason")?.into()),
        other => return Err(ReplyError::UnknownKind(other.into())),
    })
}

fn parse_change(value: &Value) -> Change {
    let Some(obj) = value.as_object() else {
        return Change::Unknown { op: String::new() };
    };
    let Some(op) = obj.get("op").and_then(Value::as_str) else {
        return Change::Unknown { op: String::new() };
    };
    if ![
        "set",
        "mcp_add",
        "mcp_enable",
        "mcp_remove",
        "toggle",
        "cron_add",
        "cron_pause",
        "cron_resume",
        "cron_remove",
        "main_model",
        "open",
    ]
    .contains(&op)
    {
        return Change::Unknown { op: op.into() };
    }
    if op == "mcp_add" {
        let forbidden = ["env", "headers", "bearer_token", "api_key"]
            .into_iter()
            .find(|key| obj.contains_key(*key))
            .or_else(|| {
                obj.get("auth")
                    .filter(|v| v.as_str() != Some("oauth"))
                    .map(|_| "auth")
            });
        if let Some(field) = forbidden {
            return Change::McpAdd {
                name: obj.get("name").and_then(Value::as_str).unwrap_or("").into(),
                source: McpSource::Forbidden(field),
                why: None,
            };
        }
    }
    let parse = || -> Result<Change, &'static str> {
        let string = |field| {
            obj.get(field)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or(field)
        };
        let optional = |field| {
            obj.get(field)
                .map(|v| v.as_str().map(str::to_owned).ok_or(field))
                .transpose()
        };
        let enabled = || obj.get("enabled").and_then(Value::as_bool).ok_or("enabled");
        let why = optional("why")?;
        Ok(match op {
            "set" => Change::Set {
                key: string("key")?,
                value: obj.get("value").cloned().ok_or("value")?,
                why,
            },
            "mcp_add" => {
                let name = string("name")?;
                if ["preset", "command", "url"]
                    .iter()
                    .filter(|key| obj.contains_key(**key))
                    .count()
                    != 1
                {
                    return Err("source");
                }
                let source = if obj.contains_key("preset") {
                    McpSource::Preset(string("preset")?)
                } else if obj.contains_key("command") {
                    let command = string("command")?;
                    let args = obj
                        .get("args")
                        .and_then(Value::as_array)
                        .and_then(|args| {
                            args.iter().map(|v| v.as_str().map(str::to_owned)).collect()
                        })
                        .ok_or("args")?;
                    McpSource::Command { command, args }
                } else {
                    McpSource::Url {
                        url: string("url")?,
                        oauth: obj.contains_key("auth"),
                    }
                };
                Change::McpAdd { name, source, why }
            }
            "mcp_enable" => Change::McpEnable {
                name: string("name")?,
                enabled: enabled()?,
                why,
            },
            "mcp_remove" => Change::McpRemove {
                name: string("name")?,
                why,
            },
            "toggle" => Change::Toggle {
                kind: match string("kind")?.as_str() {
                    "skill" => CapKind::Skill,
                    "toolset" => CapKind::Toolset,
                    "plugin" => CapKind::Plugin,
                    _ => return Err("kind"),
                },
                name: string("name")?,
                enabled: enabled()?,
                why,
            },
            "cron_add" => Change::CronAdd {
                name: string("name")?,
                schedule: string("schedule")?,
                prompt: string("prompt")?,
                repeat: obj
                    .get("repeat")
                    .map(|v| {
                        v.as_u64()
                            .and_then(|n| u32::try_from(n).ok())
                            .filter(|n| *n > 0)
                            .ok_or("repeat")
                    })
                    .transpose()?,
                deliver: optional("deliver")?,
                why,
            },
            "cron_pause" => Change::CronPause {
                job_id: string("job_id")?,
                why,
            },
            "cron_resume" => Change::CronResume {
                job_id: string("job_id")?,
                why,
            },
            "cron_remove" => Change::CronRemove {
                job_id: string("job_id")?,
                why,
            },
            "main_model" => Change::MainModel {
                provider: string("provider")?,
                model: string("model")?,
                why,
            },
            "open" => Change::Open {
                screen: match string("screen")?.as_str() {
                    "providers" => Screen::Providers,
                    "mcp" => Screen::Mcp,
                    "capabilities" => Screen::Capabilities,
                    "cron" => Screen::Cron,
                    "profiles" => Screen::Profiles,
                    "connectors" => Screen::Connectors,
                    "settings" => Screen::Settings,
                    _ => return Err("screen"),
                },
                target: optional("target")?,
                why,
            },
            _ => Change::Unknown { op: op.into() },
        })
    };
    parse().unwrap_or_else(|field| Change::Malformed {
        op: op.into(),
        field,
    })
}

pub fn prose_without_block(text: &str) -> String {
    let mut result = String::new();
    let mut previous = 0;
    for (start, end, _) in blocks(text) {
        result.push_str(&text[previous..start]);
        previous = end;
    }
    result.push_str(&text[previous..]);
    result.trim_matches('\n').to_owned()
}

pub(crate) fn cut(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

pub(crate) fn sanitize_text(text: &str, detect: super::SecretDetector) -> String {
    if detect(text) {
        return super::WITHHELD.into();
    }
    crate::attachments::sanitize(text)
}

pub(crate) fn scrub_value(value: &mut Value, detect: super::SecretDetector) -> bool {
    match value {
        Value::String(text) => scrub_string(text, detect),
        Value::Array(items) => items
            .iter_mut()
            .fold(false, |hit, v| scrub_value(v, detect) | hit),
        Value::Object(map) => {
            let old = std::mem::take(map);
            let mut hit = false;
            for (mut key, mut value) in old {
                hit |= scrub_string(&mut key, detect);
                hit |= scrub_value(&mut value, detect);
                map.insert(key, value);
            }
            hit
        }
        _ => false,
    }
}
pub(crate) fn scrub_string(text: &mut String, detect: super::SecretDetector) -> bool {
    let hit = detect(text);
    if hit {
        *text = super::WITHHELD.into();
    }
    hit || text == super::WITHHELD
}

pub(crate) fn scrub_change(change: &mut Change, detect: super::SecretDetector) -> bool {
    let mut hit = false;
    let mut strings: Vec<&mut String> = Vec::new();
    let why = match change {
        Change::Set { key, value, why } => {
            hit |= scrub_value(value, detect);
            strings.push(key);
            why
        }
        Change::McpAdd { name, source, why } => {
            strings.push(name);
            match source {
                McpSource::Preset(p) => strings.push(p),
                McpSource::Command { command, args } => {
                    strings.push(command);
                    strings.extend(args);
                }
                McpSource::Url { url, .. } => strings.push(url),
                McpSource::Forbidden(_) => {}
            }
            why
        }
        Change::McpEnable { name, why, .. }
        | Change::McpRemove { name, why }
        | Change::Toggle { name, why, .. } => {
            strings.push(name);
            why
        }
        Change::CronAdd {
            name,
            schedule,
            prompt,
            deliver,
            why,
            ..
        } => {
            strings.extend([name, schedule, prompt]);
            if let Some(v) = deliver {
                strings.push(v);
            }
            why
        }
        Change::CronPause { job_id, why }
        | Change::CronResume { job_id, why }
        | Change::CronRemove { job_id, why } => {
            strings.push(job_id);
            why
        }
        Change::MainModel {
            provider,
            model,
            why,
        } => {
            strings.extend([provider, model]);
            why
        }
        Change::Open { target, why, .. } => {
            if let Some(v) = target {
                strings.push(v);
            }
            why
        }
        Change::Malformed { op, .. } | Change::Unknown { op } => return scrub_string(op, detect),
    };
    for text in strings {
        hit |= scrub_string(text, detect);
    }
    // A reason cannot turn an otherwise safe operation into a write refusal.
    if let Some(why) = why {
        scrub_string(why, detect);
        *why = cut(why, 300);
    }
    hit
}

pub(crate) fn scrub_reply(mut reply: Reply, detect: super::SecretDetector) -> Reply {
    match &mut reply {
        Reply::Proposal(p) => {
            scrub_string(&mut p.summary, detect);
            p.summary = cut(&p.summary, 500);
            if let Some(profile) = &mut p.profile {
                scrub_string(profile, detect);
            }
            for change in &mut p.changes {
                scrub_change(change, detect);
            }
        }
        Reply::Need(n) => {
            for section in &mut n.sections {
                scrub_string(section, detect);
            }
            if let Some(why) = &mut n.why {
                scrub_string(why, detect);
            }
        }
        Reply::Answer(text) | Reply::Cannot(text) => {
            scrub_string(text, detect);
        }
    }
    reply
}
