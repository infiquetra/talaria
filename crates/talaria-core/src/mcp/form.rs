use super::*;

pub(super) const ENV_ERROR: &str = "Use letters, digits and underscore; do not start with a digit.";
pub(super) fn valid_env(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
pub fn validate_server_name(input: &str, existing: &[String]) -> Result<String, NameError> {
    let name = input.trim();
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err(NameError::Invalid);
    }
    if existing.iter().any(|n| n == name) {
        return Err(NameError::Taken);
    }
    Ok(name.into())
}
pub fn env_key_for_server(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("MCP_{}_API_KEY", name.trim_matches('_'))
}

#[derive(Debug, Default)]
pub(super) struct Form {
    pub name: String,
    pub local: bool,
    pub url: String,
    pub auth: usize,
    pub command: String,
    pub args: String,
    pub env: [String; 8],
    pub focus: usize,
    pub checked: bool,
    pub discard: bool,
    pub dirty: bool,
}
impl Form {
    fn row_count(&self, secrets: &BTreeSet<SecretSlot>) -> usize {
        (0..8)
            .rfind(|i| {
                !self.env[*i].is_empty() || secrets.contains(&SecretSlot::EnvValue(*i as u8))
            })
            .map_or(1, |i| (i + 2).min(8))
    }
    pub fn fields(&self, state: &McpState) -> Vec<FieldView> {
        let mut fields = vec![
            field("Name", &self.name, None),
            field("Kind", if self.local { "local" } else { "remote" }, None),
        ];
        let names: Vec<_> = state
            .servers
            .iter()
            .map(|r| r.summary.name.clone())
            .collect();
        fields[0].error = match validate_server_name(&self.name, &names) {
            Err(NameError::Empty) => Some("Name is required.".into()),
            Err(NameError::Invalid) => {
                Some("Use 1 to 64 letters, digits, dot, dash or underscore.".into())
            }
            Err(NameError::Taken) => Some(format!(
                "A server named {} already exists.",
                self.name.trim()
            )),
            Ok(_) => None,
        };
        if self.local {
            let mut command = field("Command", &self.command, None);
            if self.command.trim().is_empty() {
                command.error = Some("Command is required.".into());
            }
            command.warning = Some("This runs a program on this machine whenever Hermes connects. Add it only if you trust it.".into());
            fields.push(command);
            let mut args = field("Arguments", &self.args, None);
            if shell_words::split(&self.args).is_err() {
                args.error = Some("Arguments have an unclosed quote.".into());
            }
            fields.push(args);
            let mut seen = BTreeSet::new();
            for i in 0..self.row_count(&state.secrets) {
                let name = self.env[i].trim();
                let mut key = field("Name", &self.env[i], None);
                if !name.is_empty() {
                    if !valid_env(name) {
                        key.error = Some(ENV_ERROR.into());
                    } else if !seen.insert(name) {
                        key.error = Some(format!("{name} is already listed."));
                    }
                } else if state.secrets.contains(&SecretSlot::EnvValue(i as u8)) {
                    key.error = Some(ENV_ERROR.into());
                }
                fields.push(key);
                let slot = SecretSlot::EnvValue(i as u8);
                let mut value = field("Value", "", Some(slot));
                if !name.is_empty() && !state.secrets.contains(&slot) {
                    value.error = Some("Enter a value, or clear the name.".into());
                }
                fields.push(value);
            }
        } else {
            let mut url = field("URL", &self.url, None);
            let address = self.url.trim();
            url.error = if address.is_empty() {
                Some("URL is required.".into())
            } else if let Some(parts) = address_parts(address) {
                if !matches!(parts.scheme, "http" | "https") {
                    Some("Enter an http:// or https:// address with a host.".into())
                } else if parts.authority.contains('@') {
                    Some("Put credentials in the API key field, not the address.".into())
                } else {
                    None
                }
            } else {
                Some("Enter an http:// or https:// address with a host.".into())
            };
            if let Some(parts) = address_parts(address) {
                if address.contains('?') {
                    url.warning = Some(
                        "The address is saved in config.yaml as typed; keep keys out of it.".into(),
                    );
                } else if parts.scheme == "http" && !loopback(parts.host) {
                    url.warning = Some("This address is not encrypted.".into());
                }
            }
            fields.push(url);
            fields.push(field(
                "Authentication",
                ["none", "API key", "OAuth sign-in"][self.auth],
                None,
            ));
            if self.auth == 1 {
                let mut token = field("Token", "", Some(SecretSlot::Token));
                if !state.secrets.contains(&SecretSlot::Token) {
                    token.error = Some("Enter the token, or choose none.".into());
                }
                fields.push(token);
            }
        }
        for (i, f) in fields.iter_mut().enumerate() {
            f.focused = i == self.focus;
            if !self.checked {
                f.error = None;
            }
        }
        fields
    }
    fn count(&self, state: &McpState) -> usize {
        self.fields(state).len()
    }
    pub fn key(&mut self, key: McpKey, state: &mut McpState) -> (bool, Vec<McpEffect>) {
        if self.discard {
            return match key {
                McpKey::Char('y') => (true, vec![forget(state)]),
                McpKey::Char('n') | McpKey::Esc => {
                    self.discard = false;
                    (false, vec![])
                }
                _ => (false, vec![]),
            };
        }
        let mut effects = vec![];
        match key {
            McpKey::Tab | McpKey::Enter | McpKey::Down => {
                self.checked = true;
                self.focus = (self.focus + 1) % self.count(state);
            }
            McpKey::BackTab | McpKey::Up => {
                self.checked = true;
                self.focus = (self.focus + self.count(state) - 1) % self.count(state);
            }
            McpKey::Left | McpKey::Right => {
                let right = matches!(key, McpKey::Right);
                if self.focus == 1 {
                    self.local = !self.local;
                    self.url.clear();
                    self.command.clear();
                    self.args.clear();
                    self.auth = 0;
                    self.env = Default::default();
                    self.dirty = true;
                    effects.push(forget(state));
                } else if !self.local && self.focus == 3 {
                    let old = self.auth;
                    self.auth = (self.auth + if right { 1 } else { 2 }) % 3;
                    self.dirty = true;
                    if old == 1 {
                        effects.push(forget(state));
                    }
                }
            }
            McpKey::Action(McpAction::Save) => {
                if !state.connected || state.write.is_some() {
                    return (false, vec![]);
                }
                self.checked = true;
                if let Some(i) = self.fields(state).iter().position(|f| f.error.is_some()) {
                    self.focus = i;
                    return (false, vec![]);
                }
                let name = self.name.trim().to_string();
                let mut config = serde_json::Map::new();
                if self.local {
                    config.insert("command".into(), self.command.trim().into());
                    config.insert(
                        "args".into(),
                        serde_json::json!(shell_words::split(&self.args).unwrap_or_default()),
                    );
                    state.env_queue = self
                        .env
                        .iter()
                        .enumerate()
                        .filter(|(_, n)| !n.trim().is_empty())
                        .map(|(i, n)| (i as u8, n.trim().into()))
                        .collect();
                } else {
                    config.insert("url".into(), self.url.trim().into());
                    if self.auth == 2 {
                        config.insert("auth".into(), "oauth".into());
                    }
                }
                state.write = Some(Write::Add {
                    name: name.clone(),
                    catalogue: false,
                });
                state.notice = Some("Waiting for Hermes…".into());
                effects.push(McpEffect::AddServer {
                    name,
                    preset: None,
                    config: Some(config),
                    token: !self.local && self.auth == 1,
                });
            }
            McpKey::Esc => {
                if self.dirty {
                    self.discard = true;
                } else {
                    return (true, vec![forget(state)]);
                }
            }
            McpKey::Char(c) => {
                if let Some(text) = self.text_mut() {
                    text.push(c);
                    self.dirty = true;
                }
            }
            McpKey::Paste(s) => {
                if let Some(text) = self.text_mut() {
                    text.push_str(&s);
                    self.dirty = true;
                }
            }
            McpKey::Backspace => {
                if let Some(text) = self.text_mut() {
                    text.pop();
                    self.dirty = true;
                }
            }
            _ => {}
        }
        (false, effects)
    }
    fn text_mut(&mut self) -> Option<&mut String> {
        match self.focus {
            0 => Some(&mut self.name),
            2 if self.local => Some(&mut self.command),
            2 => Some(&mut self.url),
            3 if self.local => Some(&mut self.args),
            i if self.local && i >= 4 && i % 2 == 0 && (i - 4) / 2 < 8 => {
                Some(&mut self.env[(i - 4) / 2])
            }
            _ => None,
        }
    }
}
pub(super) fn field(label: &str, value: &str, secret: Option<SecretSlot>) -> FieldView {
    FieldView {
        label: label.into(),
        text: value.into(),
        secret,
        error: None,
        warning: None,
        focused: false,
    }
}

pub(super) struct Address<'a> {
    pub scheme: &'a str,
    pub authority: &'a str,
    pub host: &'a str,
    pub path: &'a str,
}
pub(super) fn address_parts(raw: &str) -> Option<Address<'_>> {
    if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let (scheme, rest) = raw.split_once("://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let host_port = authority.rsplit('@').next()?;
    let host = if let Some(ipv6) = host_port.strip_prefix('[') {
        let (host, port) = ipv6.split_once(']')?;
        if !port.is_empty() && (!port.starts_with(':') || port[1..].parse::<u16>().is_err()) {
            return None;
        }
        host
    } else {
        let (host, port) = host_port
            .split_once(':')
            .map_or((host_port, None), |(h, p)| (h, Some(p)));
        if port.is_some_and(|p| p.parse::<u16>().is_err()) {
            return None;
        }
        host
    };
    if host.is_empty() || host.contains(['\\', '%']) {
        return None;
    }
    let tail = &rest[end..];
    let path = tail.split(['?', '#']).next().unwrap_or("");
    Some(Address {
        scheme,
        authority,
        host,
        path,
    })
}
pub(super) fn loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}
