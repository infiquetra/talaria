use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::highlighting::{
    Color as SyntaxColor, FontStyle, HighlightIterator, HighlightState, Highlighter, StyleModifier,
    Theme, ThemeItem, ThemeSettings,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};

static RESOURCES: OnceLock<Resources> = OnceLock::new();
static STARTED: AtomicBool = AtomicBool::new(false);
static READY: AtomicBool = AtomicBool::new(false);
static FORCED_PLAIN: AtomicBool = AtomicBool::new(false);
static LOAD_STARTS: AtomicU32 = AtomicU32::new(0);

struct Resources {
    syntaxes: SyntaxSet,
    theme: Theme,
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
                theme: terminal_theme(),
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
    lines: Vec<Line<'static>>,
    parser: ParseState,
    state: HighlightState,
    plain_rest: bool,
    partial: Option<(String, Line<'static>)>,
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

    pub(crate) fn render(&mut self, info: Option<&str>, text: &str) -> Vec<Line<'static>> {
        let language = language_token(info);
        let resources = if is_loaded() { RESOURCES.get() } else { None };
        let Some((resources, syntax)) = resources.and_then(|resources| {
            resources
                .syntaxes
                .find_syntax_by_token(&language)
                .filter(|_| !language.is_empty())
                .map(|syntax| (resources, syntax))
        }) else {
            return plain_lines(text);
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
        let mut lines = block.lines.clone();
        let partial = &text[complete_end..];
        if !partial.is_empty() {
            if let Some((previous, line)) = &block.partial
                && previous == partial
            {
                lines.push(line.clone());
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
            lines.push(line);
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

fn plain_lines(text: &str) -> Vec<Line<'static>> {
    text.split_inclusive('\n').map(plain_line).collect()
}

fn plain_line(text: &str) -> Line<'static> {
    Line::from(text.strip_suffix('\n').unwrap_or(text).to_owned())
}

fn styled_line(
    resources: &Resources,
    parser: &mut ParseState,
    state: &mut HighlightState,
    text: &str,
) -> Option<Line<'static>> {
    let changes = parser.parse_line(text, &resources.syntaxes).ok()?;
    let highlighter = Highlighter::new(&resources.theme);
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (syntax_style, text) in HighlightIterator::new(state, &changes, text, &highlighter) {
        let text = text.strip_suffix('\n').unwrap_or(text);
        if text.is_empty() {
            continue;
        }
        let style = terminal_style(syntax_style);
        if let Some(previous) = spans.last_mut()
            && previous.style == style
        {
            previous.content.to_mut().push_str(text);
        } else {
            spans.push(Span::styled(text.to_owned(), style));
        }
    }
    Some(Line::from(spans))
}

const PALETTE: [(SyntaxColor, Color); 7] = [
    (rgb(1), Color::DarkGray),
    (rgb(2), Color::Green),
    (rgb(3), Color::Yellow),
    (rgb(4), Color::Magenta),
    (rgb(5), Color::Cyan),
    (rgb(6), Color::Blue),
    (rgb(7), Color::Red),
];

const fn rgb(value: u8) -> SyntaxColor {
    SyntaxColor {
        r: value,
        g: value,
        b: value,
        a: 255,
    }
}

fn terminal_style(syntax_style: syntect::highlighting::Style) -> Style {
    let mut style = Style::default();
    if let Some((_, color)) = PALETTE
        .iter()
        .find(|(rgb, _)| *rgb == syntax_style.foreground)
    {
        style = style.fg(*color);
    }
    for (syntax_modifier, modifier) in [
        (FontStyle::BOLD, Modifier::BOLD),
        (FontStyle::ITALIC, Modifier::ITALIC),
        (FontStyle::UNDERLINE, Modifier::UNDERLINED),
    ] {
        if syntax_style.font_style.contains(syntax_modifier) {
            style = style.add_modifier(modifier);
        }
    }
    style
}

fn terminal_theme() -> Theme {
    let mut theme = Theme {
        name: Some("Terminal palette".into()),
        author: None,
        settings: ThemeSettings::default(),
        scopes: Vec::new(),
    };
    for (selector, colour, font_style) in [
        ("comment", Some(0), None),
        ("string", Some(1), None),
        ("constant.numeric", Some(2), None),
        ("constant.language, constant.character", Some(2), None),
        ("keyword, storage.modifier", Some(3), None),
        ("storage.type", Some(4), None),
        ("entity.name.function, support.function", Some(5), None),
        (
            "entity.name.type, entity.name.class, support.type, support.class",
            Some(4),
            None,
        ),
        ("markup.heading", None, Some(FontStyle::BOLD)),
        ("markup.inserted", Some(1), None),
        ("markup.deleted, invalid", Some(6), None),
    ] {
        if let Ok(scope) = selector.parse() {
            theme.scopes.push(ThemeItem {
                scope,
                style: StyleModifier {
                    foreground: colour.map(|index| PALETTE[index].0),
                    background: None,
                    font_style,
                },
            });
        }
    }
    theme
}
