use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use syntect::highlighting::{
    HighlightState, Highlighter, RangedHighlightIterator, ScopeSelectors, Theme,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};

use crate::theme::{
    legacy,
    surfaces::{ViewStyles, tokens as t},
};

static RESOURCES: OnceLock<Resources> = OnceLock::new();
static STARTED: AtomicBool = AtomicBool::new(false);
static READY: AtomicBool = AtomicBool::new(false);
static FORCED_PLAIN: AtomicBool = AtomicBool::new(false);
static LOAD_STARTS: AtomicU32 = AtomicU32::new(0);

struct Resources {
    syntaxes: SyntaxSet,
    theme: Theme,
    categories: Vec<(ScopeSelectors, SyntaxKind)>,
}

#[doc(hidden)]
pub fn is_loaded() -> bool {
    !FORCED_PLAIN.load(Ordering::Acquire) && READY.load(Ordering::Acquire)
}

#[doc(hidden)]
pub fn force_not_ready(on: bool) {
    FORCED_PLAIN.store(on, Ordering::Release);
}

#[doc(hidden)]
pub fn load_starts() -> u32 {
    LOAD_STARTS.load(Ordering::Acquire)
}

/// Prepare the bundled grammars without blocking the caller or rendering a frame.
pub fn warm_up() {
    if FORCED_PLAIN.load(Ordering::Acquire)
        || STARTED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        return;
    }
    LOAD_STARTS.fetch_add(1, Ordering::AcqRel);
    let spawned = std::thread::Builder::new()
        .name("markdown-syntaxes".into())
        .spawn(|| {
            let resources = Resources {
                syntaxes: SyntaxSet::load_defaults_newlines(),
                theme: legacy::syntax_theme(),
                categories: syntax_categories(),
            };
            for (token, text) in [
                ("rust", "let n = 42; // ready\n"),
                ("python", "n = 42 # ready\n"),
                ("sh", "echo 42 \"ready\" # ready\n"),
            ] {
                if let Some(syntax) = resources.syntaxes.find_syntax_by_token(token) {
                    let mut parser = ParseState::new(syntax);
                    let highlighter = Highlighter::new(&resources.theme);
                    let mut state = HighlightState::new(&highlighter, ScopeStack::new());
                    let _ = styled_line(&resources, &mut parser, &mut state, text);
                }
            }
            if RESOURCES.set(resources).is_ok() {
                READY.store(true, Ordering::Release);
            }
        });
    if spawned.is_err() {
        LOAD_STARTS.fetch_sub(1, Ordering::AcqRel);
        STARTED.store(false, Ordering::Release);
    }
}

/// Width-independent syntax work retained while a reply is streaming.
#[derive(Default)]
pub(crate) struct CodeCache {
    blocks: Vec<CachedBlock>,
    highlighted: u64,
}

struct CachedBlock {
    language: String,
    source: String,
    lines: Vec<SyntaxLine>,
    parser: ParseState,
    state: HighlightState,
    plain_rest: bool,
    partial: Option<(String, SyntaxLine)>,
    used: bool,
}

impl CodeCache {
    pub(crate) fn lines_highlighted(&self) -> u64 {
        self.highlighted
    }

    pub(crate) fn clear_blocks(&mut self) {
        self.blocks.clear();
    }

    pub(crate) fn begin_render(&mut self) {
        for block in &mut self.blocks {
            block.used = false;
        }
    }

    pub(crate) fn retain_rendered(&mut self) {
        self.blocks.retain(|block| block.used);
    }

    pub(crate) fn render(
        &mut self,
        info: Option<&str>,
        text: &str,
        views: &ViewStyles,
    ) -> Vec<Line<'static>> {
        let language = language_token(info);
        let resources = if is_loaded() { RESOURCES.get() } else { None };
        let Some((resources, syntax)) = resources.and_then(|resources| {
            resources
                .syntaxes
                .find_syntax_by_token(&language)
                .filter(|_| !language.is_empty())
                .map(|syntax| (resources, syntax))
        }) else {
            return text
                .split_inclusive('\n')
                .map(plain_line)
                .map(|line| line.resolve(views))
                .collect();
        };

        // Content prefixes survive rendering a committed slice separately from its tail.
        let index = self
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| {
                !block.used && block.language == language && text.starts_with(&block.source)
            })
            .max_by_key(|(_, block)| block.source.len())
            .map(|(index, _)| index)
            .unwrap_or_else(|| {
                let highlighter = Highlighter::new(&resources.theme);
                self.blocks.push(CachedBlock {
                    language,
                    source: String::new(),
                    lines: Vec::new(),
                    parser: ParseState::new(syntax),
                    state: HighlightState::new(&highlighter, ScopeStack::new()),
                    plain_rest: false,
                    partial: None,
                    used: false,
                });
                self.blocks.len() - 1
            });
        let block = &mut self.blocks[index];
        block.used = true;
        let complete_end = text.rfind('\n').map_or(0, |index| index + 1);
        if block.source.len() != complete_end {
            block.partial = None;
        }
        for source in text[block.source.len()..complete_end].split_inclusive('\n') {
            let line = if block.plain_rest || source.trim_end_matches('\n').chars().count() > 1_000
            {
                block.plain_rest = true;
                plain_line(source)
            } else {
                self.highlighted += 1;
                match styled_line(resources, &mut block.parser, &mut block.state, source) {
                    Some(line) => line,
                    None => {
                        block.plain_rest = true;
                        plain_line(source)
                    }
                }
            };
            block.source.push_str(source);
            block.lines.push(line);
        }
        let mut lines: Vec<_> = block.lines.iter().map(|line| line.resolve(views)).collect();
        let partial = &text[complete_end..];
        if !partial.is_empty() {
            if let Some((previous, line)) = &block.partial
                && previous == partial
            {
                lines.push(line.resolve(views));
                return lines;
            }
            let line = if block.plain_rest || partial.chars().count() > 1_000 {
                plain_line(partial)
            } else {
                let mut parser = block.parser.clone();
                let mut state = block.state.clone();
                let source = format!("{partial}\n");
                self.highlighted += 1;
                styled_line(resources, &mut parser, &mut state, &source)
                    .unwrap_or_else(|| plain_line(partial))
            };
            block.partial = Some((partial.to_owned(), line.clone()));
            lines.push(line.resolve(views));
        } else {
            block.partial = None;
        }
        lines
    }
}

fn language_token(info: Option<&str>) -> String {
    let token = info
        .and_then(|info| info.split_whitespace().next())
        .unwrap_or_default()
        .to_lowercase();
    match token.as_str() {
        "shell" | "zsh" => "sh".into(),
        _ => token,
    }
}

fn plain_line(text: &str) -> SyntaxLine {
    SyntaxLine(vec![SyntaxSpan {
        text: text.strip_suffix('\n').unwrap_or(text).to_owned(),
        legacy: Style::default(),
        kind: SyntaxKind::Variable,
    }])
}

fn styled_line(
    resources: &Resources,
    parser: &mut ParseState,
    state: &mut HighlightState,
    text: &str,
) -> Option<SyntaxLine> {
    let changes = parser.parse_line(text, &resources.syntaxes).ok()?;
    let highlighter = Highlighter::new(&resources.theme);
    let mut scopes = state.path.clone();
    let mut spans: Vec<SyntaxSpan> = Vec::new();
    let mut operation_index = 0;
    for (syntax_style, text, range) in
        RangedHighlightIterator::new(state, &changes, text, &highlighter)
    {
        // The highlighter skips empty regions. Apply all scope operations at
        // this token's start instead of pairing iterators with different lengths.
        while let Some((offset, operation)) = changes.get(operation_index)
            && *offset <= range.start
        {
            scopes.apply(operation).ok()?;
            operation_index += 1;
        }
        let text = text.strip_suffix('\n').unwrap_or(text);
        if text.is_empty() {
            continue;
        }
        let style = legacy::syntax_style(syntax_style);
        let kind = resources
            .categories
            .iter()
            .filter_map(|(selector, kind)| {
                selector
                    .does_match(&scopes.scopes)
                    .map(|score| (score, *kind))
            })
            // Delimiters inside strings and comments belong to that lexical region.
            .max_by(|a, b| {
                let region = |kind| matches!(kind, SyntaxKind::String | SyntaxKind::Comment);
                region(a.1).cmp(&region(b.1)).then_with(|| a.0.cmp(&b.0))
            })
            .map_or(SyntaxKind::Variable, |(_, kind)| kind);
        if let Some(previous) = spans.last_mut()
            && previous.legacy == style
            && previous.kind == kind
        {
            previous.text.push_str(text);
        } else {
            spans.push(SyntaxSpan {
                text: text.to_owned(),
                legacy: style,
                kind,
            });
        }
    }
    Some(SyntaxLine(spans))
}

#[derive(Clone)]
struct SyntaxLine(Vec<SyntaxSpan>);

#[derive(Clone)]
struct SyntaxSpan {
    text: String,
    legacy: Style,
    kind: SyntaxKind,
}

impl SyntaxLine {
    fn resolve(&self, views: &ViewStyles) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        for span in &self.0 {
            let style = views.inline(
                match span.kind {
                    SyntaxKind::Keyword => t::CODE_SYNTAX_KEYWORD,
                    SyntaxKind::String => t::CODE_SYNTAX_STRING,
                    SyntaxKind::Number => t::CODE_SYNTAX_NUMBER,
                    SyntaxKind::Comment => t::CODE_SYNTAX_COMMENT,
                    SyntaxKind::Function => t::CODE_SYNTAX_FUNCTION,
                    SyntaxKind::Type => t::CODE_SYNTAX_TYPE,
                    SyntaxKind::Operator => t::CODE_SYNTAX_OPERATOR,
                    SyntaxKind::Punctuation => t::CODE_SYNTAX_PUNCTUATION,
                    SyntaxKind::Variable => t::CODE_SYNTAX_VARIABLE,
                    SyntaxKind::Constant => t::CODE_SYNTAX_CONSTANT,
                    SyntaxKind::Invalid => t::CODE_SYNTAX_INVALID,
                },
                span.legacy,
            );
            if let Some(previous) = spans.last_mut()
                && previous.style == style
            {
                previous.content.to_mut().push_str(&span.text);
            } else {
                spans.push(Span::styled(span.text.clone(), style));
            }
        }
        Line::from(spans)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SyntaxKind {
    Keyword,
    String,
    Number,
    Comment,
    Function,
    Type,
    Operator,
    Punctuation,
    Variable,
    Constant,
    Invalid,
}

fn syntax_categories() -> Vec<(ScopeSelectors, SyntaxKind)> {
    [
        (
            "keyword, storage.modifier, storage.type.function",
            SyntaxKind::Keyword,
        ),
        ("string", SyntaxKind::String),
        ("constant.numeric", SyntaxKind::Number),
        ("comment", SyntaxKind::Comment),
        (
            "entity.name.function, support.function",
            SyntaxKind::Function,
        ),
        (
            "storage.type, entity.name.type, entity.name.class, support.type, support.class",
            SyntaxKind::Type,
        ),
        ("keyword.operator", SyntaxKind::Operator),
        ("punctuation", SyntaxKind::Punctuation),
        ("variable", SyntaxKind::Variable),
        (
            "constant.language, constant.character, constant.other",
            SyntaxKind::Constant,
        ),
        ("invalid", SyntaxKind::Invalid),
    ]
    .into_iter()
    .filter_map(|(selector, kind)| selector.parse().ok().map(|selector| (selector, kind)))
    .collect()
}
