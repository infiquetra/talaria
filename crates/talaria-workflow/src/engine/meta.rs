use std::collections::HashSet;

use serde_json::{Map, Value};

use super::{ParsedScript, PhaseMeta, ScriptError, WorkflowMeta, scan};

pub fn parse_script(source: &str) -> Result<ParsedScript, ScriptError> {
    let mut p = Parser {
        text: source.chars().collect(),
        pos: 0,
    };
    if let Err(ScriptError::MetaNotLiteral { line, .. }) = p.space() {
        return Err(ScriptError::MetaMissing { line });
    }
    let start = p.pos;
    let missing = ScriptError::MetaMissing {
        line: p.location(start).0,
    };
    for token in ["export", "const", "meta", "="] {
        if !p.token(token) {
            return Err(missing);
        }
        p.space().map_err(|_| missing.clone())?;
    }
    if p.peek() != Some('{') {
        return Err(missing);
    }
    let value = p.value(0)?;
    let mut end = p.pos;
    let boundary_start = p.pos;
    p.space()?;
    let newline = p.text[boundary_start..p.pos].iter().any(|c| line_end(*c));
    if p.peek() == Some(';') {
        p.pos += 1;
        end = p.pos;
    } else if p.peek().is_some() {
        let continuation = p.peek().is_some_and(|c| "([.?+-*/%,=<> &|^`!".contains(c))
            || p.at_word("in")
            || p.at_word("instanceof");
        if !newline || continuation {
            return Err(p.error(p.pos, "expression continues after the meta literal"));
        }
    }
    let (meta, warnings) = fields(value)?;
    let body: String = p
        .text
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            if (start..end).contains(&i) && !line_end(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    scan::check(&body)?;
    Ok(ParsedScript {
        meta,
        body,
        warnings,
    })
}

pub(super) fn line_end(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}
fn ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$'
}
fn ident(c: char) -> bool {
    ident_start(c) || c.is_ascii_digit()
}

struct Parser {
    text: Vec<char>,
    pos: usize,
}
impl Parser {
    fn peek(&self) -> Option<char> {
        self.text.get(self.pos).copied()
    }
    fn location(&self, at: usize) -> (u32, u32) {
        scan::location(&self.text, at)
    }
    fn error(&self, at: usize, found: impl Into<String>) -> ScriptError {
        let (line, column) = self.location(at);
        ScriptError::MetaNotLiteral {
            line,
            column,
            found: found.into(),
        }
    }
    fn at_word(&self, s: &str) -> bool {
        let chars: Vec<_> = s.chars().collect();
        self.text.get(self.pos..self.pos + chars.len()) == Some(chars.as_slice())
            && !self
                .text
                .get(self.pos + chars.len())
                .is_some_and(|c| ident(*c))
    }
    fn token(&mut self, s: &str) -> bool {
        if self.at_word(s) {
            self.pos += s.chars().count();
            true
        } else {
            false
        }
    }
    fn space(&mut self) -> Result<(), ScriptError> {
        loop {
            while self.peek().is_some_and(char::is_whitespace) {
                self.pos += 1;
            }
            if self.text.get(self.pos..self.pos + 2) == Some(&['/', '/']) {
                while self.peek().is_some_and(|c| !line_end(c)) {
                    self.pos += 1;
                }
            } else if self.text.get(self.pos..self.pos + 2) == Some(&['/', '*']) {
                let at = self.pos;
                self.pos += 2;
                while self.pos < self.text.len()
                    && self.text.get(self.pos..self.pos + 2) != Some(&['*', '/'])
                {
                    self.pos += 1;
                }
                if self.pos == self.text.len() {
                    return Err(self.error(at, "unterminated block comment"));
                }
                self.pos += 2;
            } else {
                return Ok(());
            }
        }
    }
    fn value(&mut self, depth: usize) -> Result<Value, ScriptError> {
        self.space()?;
        if depth > 128 {
            return Err(self.error(self.pos, "literal nesting is too deep"));
        }
        let at = self.pos;
        match self.peek() {
            Some('{') => {
                self.pos += 1;
                self.space()?;
                let mut object = Map::new();
                while self.peek() != Some('}') {
                    let key = match self.peek() {
                        Some('\'' | '"' | '`') => self.string()?,
                        Some(c) if ident_start(c) => self.identifier(),
                        Some('[') => return Err(self.error(self.pos, "computed key")),
                        Some('.') => return Err(self.error(self.pos, "spread")),
                        _ => return Err(self.error(self.pos, "object key")),
                    };
                    self.space()?;
                    if self.peek() != Some(':') {
                        return Err(self.error(self.pos, "expected ':'"));
                    }
                    self.pos += 1;
                    let value = self.value(depth + 1)?;
                    if object.insert(key.clone(), value).is_some() {
                        return Err(field(&key, "duplicate key"));
                    }
                    self.space()?;
                    if self.peek() == Some('}') {
                        break;
                    }
                    if self.peek() != Some(',') {
                        return Err(self.error(self.pos, "expected ',' or '}'"));
                    }
                    self.pos += 1;
                    self.space()?;
                }
                self.pos += 1;
                Ok(Value::Object(object))
            }
            Some('[') => {
                self.pos += 1;
                self.space()?;
                let mut array = Vec::new();
                while self.peek() != Some(']') {
                    array.push(self.value(depth + 1)?);
                    self.space()?;
                    if self.peek() == Some(']') {
                        break;
                    }
                    if self.peek() != Some(',') {
                        return Err(self.error(self.pos, "expected ',' or ']'"));
                    }
                    self.pos += 1;
                    self.space()?;
                }
                self.pos += 1;
                Ok(Value::Array(array))
            }
            Some('\'' | '"' | '`') => self.string().map(Value::String),
            Some(c) if c == '-' || c.is_ascii_digit() => self.number(),
            Some(c) if ident_start(c) => {
                let word = self.identifier();
                match word.as_str() {
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    "null" => Ok(Value::Null),
                    _ => {
                        self.space()?;
                        Err(self.error(
                            at,
                            if self.peek() == Some('(') {
                                "call".into()
                            } else {
                                format!("identifier {word}")
                            },
                        ))
                    }
                }
            }
            Some('.') if self.text.get(at + 1) == Some(&'.') => Err(self.error(at, "spread")),
            Some('.') => Err(self.error(
                at,
                format!(
                    "number .{}",
                    self.text
                        .get(at + 1)
                        .map_or_else(String::new, char::to_string)
                ),
            )),
            _ => Err(self.error(
                at,
                format!(
                    "unexpected token {}",
                    self.peek()
                        .map_or_else(|| "end of input".into(), |c| c.to_string())
                ),
            )),
        }
    }
    fn identifier(&mut self) -> String {
        let at = self.pos;
        while self.peek().is_some_and(ident) {
            self.pos += 1;
        }
        self.text[at..self.pos].iter().collect()
    }
    fn number(&mut self) -> Result<Value, ScriptError> {
        let at = self.pos;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || ".+-_".contains(c))
        {
            self.pos += 1;
        }
        let token: String = self.text[at..self.pos].iter().collect();
        serde_json::from_str::<Value>(&token)
            .ok()
            .filter(Value::is_number)
            .ok_or_else(|| self.error(at, format!("number {token}")))
    }
    fn string(&mut self) -> Result<String, ScriptError> {
        let at = self.pos;
        let quote = self.text[at];
        self.pos += 1;
        let mut out = String::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            if c == quote {
                return Ok(out);
            }
            if c == '$' && quote == '`' && self.peek() == Some('{') {
                return Err(self.error(self.pos - 1, "template ${"));
            }
            if line_end(c) {
                if quote != '`' {
                    return Err(self.error(at, "raw newline in string"));
                }
                if c == '\r' {
                    if self.peek() == Some('\n') {
                        self.pos += 1;
                    }
                    out.push('\n');
                } else {
                    out.push(c);
                }
            } else if c == '\\' {
                let escape_at = self.pos - 1;
                let Some(e) = self.peek() else {
                    return Err(self.error(at, "unterminated string"));
                };
                self.pos += 1;
                out.push(match e {
                    '\'' | '"' | '`' | '\\' | '/' => e,
                    'b' => '\u{8}',
                    'f' => '\u{c}',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'v' => '\u{b}',
                    '0' if !self.peek().is_some_and(|c| c.is_ascii_digit()) => '\0',
                    'u' => {
                        let braced = self.peek() == Some('{');
                        if braced {
                            self.pos += 1;
                        }
                        let digits = self.pos;
                        if braced {
                            while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                                self.pos += 1;
                            }
                        } else {
                            for _ in 0..4 {
                                if self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                                    self.pos += 1;
                                } else {
                                    return Err(self.error(escape_at, "invalid unicode escape"));
                                }
                            }
                        }
                        if braced && (self.pos == digits || self.peek() != Some('}')) {
                            return Err(self.error(escape_at, "invalid unicode escape"));
                        }
                        let hex: String = self.text[digits..self.pos].iter().collect();
                        if braced {
                            self.pos += 1;
                        }
                        let mut scalar = u32::from_str_radix(&hex, 16)
                            .map_err(|_| self.error(escape_at, "invalid unicode scalar"))?;
                        if !braced && (0xd800..=0xdbff).contains(&scalar) {
                            if self.text.get(self.pos..self.pos + 2) != Some(&['\\', 'u']) {
                                return Err(self.error(escape_at, "lone unicode surrogate"));
                            }
                            self.pos += 2;
                            let low_at = self.pos;
                            for _ in 0..4 {
                                if !self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                                    return Err(
                                        self.error(escape_at, "invalid unicode surrogate pair")
                                    );
                                }
                                self.pos += 1;
                            }
                            let low: String = self.text[low_at..self.pos].iter().collect();
                            let low = u32::from_str_radix(&low, 16).map_err(|_| {
                                self.error(escape_at, "invalid unicode surrogate pair")
                            })?;
                            if !(0xdc00..=0xdfff).contains(&low) {
                                return Err(self.error(escape_at, "invalid unicode surrogate pair"));
                            }
                            scalar = 0x10000 + (scalar - 0xd800) * 0x400 + low - 0xdc00;
                        }
                        char::from_u32(scalar)
                            .ok_or_else(|| self.error(escape_at, "invalid unicode scalar"))?
                    }
                    _ => return Err(self.error(escape_at, "invalid string escape")),
                });
            } else {
                out.push(c);
            }
        }
        Err(self.error(at, "unterminated string"))
    }
}

fn field(name: &str, problem: &str) -> ScriptError {
    ScriptError::MetaField {
        field: name.into(),
        problem: problem.into(),
    }
}
fn string_field(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
    required: bool,
) -> Result<Option<String>, ScriptError> {
    match object.get(key) {
        Some(Value::String(s)) if !required || !s.trim().is_empty() => Ok(Some(s.clone())),
        None if !required => Ok(None),
        _ => Err(field(path, "must be a non-empty string")),
    }
}
fn fields(value: Value) -> Result<(WorkflowMeta, Vec<String>), ScriptError> {
    let object = value
        .as_object()
        .ok_or_else(|| field("meta", "must be an object"))?;
    let name = string_field(object, "name", "name", true)?.unwrap_or_default();
    if name.chars().count() > 64
        || name.starts_with('.')
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    {
        return Err(field(
            "name",
            "must contain 1 to 64 letters, digits, '.', '_' or '-', and not start with '.'",
        ));
    }
    let description = string_field(object, "description", "description", true)?.unwrap_or_default();
    let when_to_use = string_field(object, "whenToUse", "whenToUse", false)?;
    let mut warnings = Vec::new();
    for key in object
        .keys()
        .filter(|k| !["name", "description", "whenToUse", "phases"].contains(&k.as_str()))
    {
        warnings.push(format!("unknown meta key `{key}` ignored"));
    }
    let mut phases = Vec::new();
    let mut titles = HashSet::new();
    if let Some(value) = object.get("phases") {
        let array = value
            .as_array()
            .ok_or_else(|| field("phases", "must be an array"))?;
        for (i, value) in array.iter().enumerate() {
            let path = format!("phases[{i}]");
            let phase = value
                .as_object()
                .ok_or_else(|| field(&path, "must be an object"))?;
            let title =
                string_field(phase, "title", &format!("{path}.title"), true)?.unwrap_or_default();
            if !titles.insert(title.clone()) {
                return Err(field(&format!("{path}.title"), "duplicate title"));
            }
            let detail = string_field(phase, "detail", &format!("{path}.detail"), false)?;
            let model = string_field(phase, "model", &format!("{path}.model"), false)?;
            for key in phase
                .keys()
                .filter(|k| !["title", "detail", "model"].contains(&k.as_str()))
            {
                warnings.push(format!("unknown meta key `{path}.{key}` ignored"));
            }
            phases.push(PhaseMeta {
                title,
                detail,
                model,
            });
        }
    }
    Ok((
        WorkflowMeta {
            name,
            description,
            when_to_use,
            phases,
        },
        warnings,
    ))
}
