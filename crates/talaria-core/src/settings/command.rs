use super::is_secret_key;
use std::fmt;
#[derive(Clone, PartialEq)]
pub enum ConfigCommand {
    Open { filter: Option<String> },
    Set(Vec<(String, String)>),
}
#[derive(Clone, Debug, PartialEq)]
pub enum CommandError {
    Secret { key: String },
    Syntax { detail: String },
}
impl fmt::Debug for ConfigCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open { .. } => f.write_str("Open"),
            Self::Set(pairs) => f.debug_tuple("Set").field(&pairs.len()).finish(),
        }
    }
}
/// Runs before quote parsing, so malformed input cannot bypass history protection.
pub fn secret_key_in_args(args: &str) -> Option<String> {
    let tokens: Vec<_> = args.split_ascii_whitespace().collect();
    tokens.iter().enumerate().find_map(|(index, token)| {
        let token = token.trim_start_matches('"');
        let key = token.split('=').next().unwrap_or(token);
        if is_secret_key(key) && (token.contains('=') || index + 1 < tokens.len()) {
            Some(key.into())
        } else {
            None
        }
    })
}
pub fn is_secret_config_line(text: &str) -> bool {
    let text = text.trim_start();
    let Some(rest) = text.strip_prefix("/config") else {
        return false;
    };
    (rest.is_empty() || rest.starts_with(char::is_whitespace)) && secret_key_in_args(rest).is_some()
}
pub fn parse_config_command(args: &str) -> Result<ConfigCommand, CommandError> {
    if let Some(key) = secret_key_in_args(args) {
        return Err(CommandError::Secret { key });
    }
    if !args.contains('=') {
        return Ok(ConfigCommand::Open {
            filter: (!args.trim().is_empty()).then(|| args.trim().into()),
        });
    }
    let syntax = |detail: &str| CommandError::Syntax {
        detail: detail.into(),
    };
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    let mut chars = args.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => quoted = !quoted,
            '\\' if quoted => match chars.next() {
                Some('"') => token.push('"'),
                Some('\\') => token.push('\\'),
                _ => return Err(syntax("use only escaped quotes or backslashes")),
            },
            c if c.is_whitespace() && !quoted => {
                if !token.is_empty() {
                    tokens.push(std::mem::take(&mut token));
                }
            }
            c => token.push(c),
        }
    }
    if quoted {
        return Err(syntax("close the double quote"));
    }
    if !token.is_empty() {
        tokens.push(token)
    }
    let mut pairs = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for token in tokens {
        let Some((key, value)) = token.split_once('=') else {
            return Err(syntax("every item must be key=value"));
        };
        if key.is_empty() {
            return Err(syntax("a key is missing"));
        }
        if !seen.insert(key.to_owned()) {
            return Err(syntax("a key appears more than once"));
        }
        pairs.push((key.into(), value.into()));
    }
    Ok(ConfigCommand::Set(pairs))
}
