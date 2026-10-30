//! Agent launch configuration.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcpAgentEntry {
    pub id: String,
    pub display_name: String,
    pub command: String,
    pub args: Vec<String>,
    pub auth_method: Option<String>,
    pub allow_fs: bool,
    pub allow_terminal: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AcpConfig {
    pub agents: Vec<AcpAgentEntry>,
}

impl AcpConfig {
    pub fn parse(text: &str) -> Result<Self, String> {
        let root: toml::Table = toml::from_str(text).map_err(|error| format!("{error}"))?;
        let mut notices = Vec::new();
        Ok(root
            .get("acp")
            .and_then(toml::Value::as_table)
            .map_or_else(Self::default, |table| parse_section(table, &mut notices)))
    }
}

pub(crate) fn parse_section(section: &toml::Table, notices: &mut Vec<String>) -> AcpConfig {
    for key in section.keys().filter(|key| key.as_str() != "agents") {
        notices.push(format!(
            "Ignoring acp.{} in ~/.talaria/config.toml: it is not a known setting.",
            talaria_core::text::sanitize(key)
        ));
    }
    let Some(value) = section.get("agents") else {
        return AcpConfig::default();
    };
    let Some(agents) = value.as_array() else {
        notices.push("acp.agents in ~/.talaria/config.toml must be an array of tables.".into());
        return AcpConfig::default();
    };
    let mut parsed = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (index, value) in agents.iter().enumerate() {
        let Some(entry) = value.as_table() else {
            notices.push(format!(
                "Ignoring acp.agents entry {}: it must be a table.",
                index + 1
            ));
            continue;
        };
        for key in entry.keys().filter(|key| {
            !matches!(
                key.as_str(),
                "id" | "display_name"
                    | "command"
                    | "args"
                    | "auth_method"
                    | "allow_fs"
                    | "allow_terminal"
            )
        }) {
            notices.push(format!(
                "Ignoring unknown key acp.agents.{}.",
                talaria_core::text::sanitize(key)
            ));
        }
        let valid = || -> Option<AcpAgentEntry> {
            let id = entry.get("id")?.as_str()?;
            if id.len() > 32
                || id.is_empty()
                || !id.bytes().next()?.is_ascii_lowercase() && !id.bytes().next()?.is_ascii_digit()
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            {
                return None;
            }
            let command = entry.get("command")?.as_str()?;
            if command.trim().is_empty() {
                return None;
            }
            let display_name = match entry.get("display_name") {
                None => id,
                Some(value) => value.as_str()?,
            };
            let display_name = if display_name.is_empty() {
                id
            } else {
                display_name
            };
            let args = match entry.get("args") {
                None => Vec::new(),
                Some(value) => value
                    .as_array()?
                    .iter()
                    .map(|arg| arg.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()?,
            };
            let auth_method = match entry.get("auth_method") {
                None => None,
                Some(value) => {
                    let text = value.as_str()?;
                    (!text.is_empty()).then(|| text.to_owned())
                }
            };
            let flag = |name: &str| match entry.get(name) {
                None => Some(false),
                Some(value) => value.as_bool(),
            };
            Some(AcpAgentEntry {
                id: id.into(),
                display_name: display_name.into(),
                command: command.into(),
                args,
                auth_method,
                allow_fs: flag("allow_fs")?,
                allow_terminal: flag("allow_terminal")?,
            })
        };
        let Some(agent) = valid() else {
            notices.push(format!("Ignoring invalid acp.agents entry {}.", index + 1));
            continue;
        };
        if seen.insert(agent.id.clone()) {
            parsed.push(agent);
        } else {
            notices.push(format!(
                "Ignoring duplicate acp.agents id {}.",
                talaria_core::text::sanitize(&agent.id)
            ));
        }
    }
    AcpConfig { agents: parsed }
}
