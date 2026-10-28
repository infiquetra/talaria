use super::{ScriptError, meta::line_end};

pub(super) fn location(text: &[char], at: usize) -> (u32, u32) {
    let (mut line, mut column) = (1, 1);
    for (i, c) in text.iter().take(at).enumerate() {
        if line_end(*c) {
            if *c != '\n' || i == 0 || text[i - 1] != '\r' {
                line += 1;
            }
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

pub(super) fn check(source: &str) -> Result<(), ScriptError> {
    let mut scan = Scanner {
        chars: source.chars().collect(),
        pos: 0,
    };
    scan.code(false, 0)
}

struct Scanner {
    chars: Vec<char>,
    pos: usize,
}
impl Scanner {
    fn at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }
    fn space(&mut self) {
        loop {
            match (self.at(0), self.at(1)) {
                (Some(c), _) if c.is_whitespace() => self.pos += 1,
                (Some('/'), Some('/')) => {
                    self.pos += 2;
                    while self.at(0).is_some_and(|c| !line_end(c)) {
                        self.pos += 1;
                    }
                }
                (Some('/'), Some('*')) => {
                    self.pos += 2;
                    while self.at(0).is_some() && (self.at(0), self.at(1)) != (Some('*'), Some('/'))
                    {
                        self.pos += 1;
                    }
                    if self.at(0).is_some() {
                        self.pos += 2;
                    }
                }
                _ => break,
            }
        }
    }
    fn quoted(&mut self, quote: char) {
        self.pos += 1;
        while let Some(c) = self.at(0) {
            self.pos += 1;
            if c == '\\' {
                if self.at(0).is_some() {
                    self.pos += 1;
                }
            } else if c == quote {
                break;
            }
        }
    }
    fn regex(&mut self) {
        self.pos += 1;
        let mut class = false;
        while let Some(c) = self.at(0) {
            self.pos += 1;
            match c {
                '\\' => {
                    if self.at(0).is_some() {
                        self.pos += 1;
                    }
                }
                '[' => class = true,
                ']' => class = false,
                '/' if !class => break,
                c if line_end(c) => break,
                _ => {}
            }
        }
        while self.at(0).is_some_and(|c| c.is_alphabetic()) {
            self.pos += 1;
        }
    }
    fn template(&mut self, depth: usize) -> Result<(), ScriptError> {
        self.pos += 1;
        while let Some(c) = self.at(0) {
            match c {
                '\\' => {
                    self.pos += 1;
                    if self.at(0).is_some() {
                        self.pos += 1;
                    }
                }
                '`' => {
                    self.pos += 1;
                    break;
                }
                '$' if self.at(1) == Some('{') => {
                    self.pos += 2;
                    self.code(true, depth + 1)?;
                }
                _ => self.pos += 1,
            }
        }
        Ok(())
    }
    fn code(&mut self, template_expr: bool, depth: usize) -> Result<(), ScriptError> {
        // Bound recursion on arbitrary input; deeper syntax is left to the compiler.
        if depth > 256 {
            return Ok(());
        }
        let mut can_end = false;
        let mut property = false;
        let mut braces = 0;
        loop {
            self.space();
            let Some(c) = self.at(0) else {
                return Ok(());
            };
            match c {
                '\'' | '"' => {
                    self.quoted(c);
                    can_end = true;
                    property = false;
                }
                '`' => {
                    self.template(depth)?;
                    can_end = true;
                    property = false;
                }
                '/' if !can_end => {
                    self.regex();
                    can_end = true;
                    property = false;
                }
                c if c.is_alphabetic() || c == '_' || c == '$' => {
                    let at = self.pos;
                    self.pos += 1;
                    while self
                        .at(0)
                        .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$')
                    {
                        self.pos += 1;
                    }
                    let word: String = self.chars[at..self.pos].iter().collect();
                    self.space();
                    if word == "import" && !property && matches!(self.at(0), Some('(' | '.')) {
                        let (line, column) = location(&self.chars, at);
                        return Err(ScriptError::ImportNotAllowed { line, column });
                    }
                    can_end = ![
                        "return", "throw", "typeof", "void", "delete", "yield", "await", "case",
                        "new",
                    ]
                    .contains(&word.as_str());
                    property = false;
                }
                c if c.is_ascii_digit() => {
                    self.pos += 1;
                    while self
                        .at(0)
                        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
                    {
                        self.pos += 1;
                    }
                    can_end = true;
                    property = false;
                }
                '.' => {
                    self.pos += 1;
                    property = true;
                    can_end = false;
                }
                '?' if self.at(1) == Some('.') => {
                    self.pos += 2;
                    property = true;
                    can_end = false;
                }
                '{' => {
                    braces += 1;
                    self.pos += 1;
                    can_end = false;
                    property = false;
                }
                '}' if template_expr && braces == 0 => {
                    self.pos += 1;
                    return Ok(());
                }
                '}' => {
                    braces -= 1;
                    self.pos += 1;
                    can_end = true;
                    property = false;
                }
                ')' | ']' => {
                    self.pos += 1;
                    can_end = true;
                    property = false;
                }
                _ => {
                    self.pos += 1;
                    can_end = false;
                    property = false;
                }
            }
        }
    }
}
