use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, LinkType, Options, Parser, Tag};
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::MarkdownOptions;
use super::highlight::CodeCache;
use super::table;
use super::wrap::{self, Piece};

enum Node<'a> {
    Branch(Tag<'a>, Vec<Node<'a>>),
    Leaf(Event<'a>),
}

#[derive(Clone, Copy)]
enum Spacing {
    Blocks,
    Tight,
    Item,
}

const MAX_NESTING: usize = 32;

fn nodes<'a>(events: &mut impl Iterator<Item = Event<'a>>, depth: usize) -> Vec<Node<'a>> {
    let mut result = Vec::new();
    while let Some(event) = events.next() {
        match event {
            Event::Start(tag) if depth < MAX_NESTING => {
                result.push(Node::Branch(tag, nodes(events, depth + 1)));
            }
            Event::Start(_) => {
                // Consume the rest of this subtree iteratively, including its closing tag.
                let mut nesting = 1usize;
                let mut flattened = vec![Node::Leaf(Event::Text("… ".into()))];
                for event in events.by_ref() {
                    match event {
                        Event::Start(_) => nesting += 1,
                        Event::End(_) => {
                            nesting -= 1;
                            if nesting == 0 {
                                break;
                            }
                        }
                        Event::Text(_)
                        | Event::Code(_)
                        | Event::Html(_)
                        | Event::InlineHtml(_)
                        | Event::SoftBreak
                        | Event::HardBreak => flattened.push(Node::Leaf(event)),
                        _ => {}
                    }
                }
                result.push(Node::Branch(Tag::Paragraph, flattened));
            }
            Event::End(_) => break,
            event => result.push(Node::Leaf(event)),
        }
    }
    result
}

pub(crate) fn render(
    text: &str,
    options: &MarkdownOptions,
    cache: &mut CodeCache,
) -> Vec<Line<'static>> {
    cache.begin_render();
    let parser = Parser::new_ext(
        text,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
    );
    let document = nodes(&mut parser.into_iter(), 0);
    Renderer { options, cache }.sequence(
        &document,
        usize::from(options.width.max(8)),
        0,
        Spacing::Blocks,
    )
}

struct Renderer<'a> {
    options: &'a MarkdownOptions,
    cache: &'a mut CodeCache,
}

impl Renderer<'_> {
    fn style(&self, local: Style) -> Style {
        let mut style = self.options.base_style.patch(local);
        if !self.options.color {
            style.fg = None;
            style.bg = None;
            style.underline_color = None;
        }
        style
    }

    fn sequence(
        &mut self,
        children: &[Node<'_>],
        width: usize,
        depth: usize,
        spacing: Spacing,
    ) -> Vec<Line<'static>> {
        let mut output = Vec::new();
        let mut index = 0;
        let mut previous_paragraph = false;
        while index < children.len() {
            let paragraph = matches!(&children[index], Node::Branch(Tag::Paragraph, _));
            let lines;
            if is_block(&children[index]) {
                lines = self.block(&children[index], width, depth);
                index += 1;
            } else {
                let end = (index..children.len())
                    .find(|&i| is_block(&children[i]))
                    .unwrap_or(children.len());
                lines = wrap::wrap(&self.inline(&children[index..end], Style::default()), width);
                index = end;
            }
            if lines.is_empty() {
                continue;
            }
            let separated = match spacing {
                Spacing::Blocks => true,
                Spacing::Tight => false,
                Spacing::Item => !(previous_paragraph && paragraph),
            };
            if separated && !output.is_empty() {
                output.push(Line::default());
            }
            output.extend(lines);
            previous_paragraph = paragraph;
        }
        output
    }

    fn block(&mut self, node: &Node<'_>, width: usize, depth: usize) -> Vec<Line<'static>> {
        match node {
            Node::Branch(Tag::Paragraph, children) => {
                wrap::wrap(&self.inline(children, Style::default()), width)
            }
            Node::Branch(Tag::Heading { level, .. }, children) => {
                let style = match level {
                    HeadingLevel::H1 => self.options.theme.heading1,
                    HeadingLevel::H2 => self.options.theme.heading2,
                    _ => self.options.theme.heading3,
                };
                wrap::wrap(&self.inline(children, style), width)
            }
            Node::Branch(Tag::BlockQuote(_), children) => {
                let lines = self.sequence(
                    children,
                    width.saturating_sub(2).max(1),
                    depth,
                    Spacing::Blocks,
                );
                prefix(
                    lines,
                    "▎ ",
                    "▎ ",
                    self.style(self.options.theme.quote_bar),
                    true,
                )
            }
            Node::Branch(Tag::List(start), children) => self.list(children, *start, width, depth),
            Node::Branch(Tag::CodeBlock(kind), children) => {
                let info = match kind {
                    CodeBlockKind::Fenced(info) => Some(info.as_ref()),
                    CodeBlockKind::Indented => None,
                };
                let text: String = children
                    .iter()
                    .filter_map(|node| match node {
                        Node::Leaf(Event::Text(text)) => Some(text.as_ref()),
                        _ => None,
                    })
                    .collect();
                let mut result = Vec::new();
                if let Some(label) = info.and_then(|info| info.split_whitespace().next()) {
                    result.extend(wrap::hard(
                        vec![Span::styled(
                            label.to_lowercase(),
                            self.style(self.options.theme.code_label),
                        )],
                        width,
                    ));
                }
                for line in self.cache.render(info, &text) {
                    let spans = line
                        .spans
                        .into_iter()
                        .map(|span| {
                            Span::styled(
                                span.content.into_owned(),
                                self.style(line.style.patch(span.style)),
                            )
                        })
                        .collect();
                    result.extend(wrap::hard(spans, width));
                }
                result
            }
            Node::Branch(Tag::Table(alignments), children) => {
                let mut headers = Vec::new();
                let mut rows = Vec::new();
                for child in children {
                    match child {
                        Node::Branch(Tag::TableHead, cells) => {
                            headers = self.cells(cells, self.options.theme.table_header)
                        }
                        Node::Branch(Tag::TableRow, cells) => {
                            rows.push(self.cells(cells, Style::default()))
                        }
                        _ => {}
                    }
                }
                table::render(
                    &headers,
                    &rows,
                    alignments,
                    width,
                    self.style(self.options.theme.table_border),
                    self.style(Style::default()),
                )
            }
            Node::Branch(Tag::HtmlBlock, children) => {
                wrap::wrap(&self.inline(children, Style::default()), width)
            }
            Node::Leaf(Event::Rule) => vec![Line::from(Span::styled(
                "─".repeat(width.min(80)),
                self.style(self.options.theme.rule),
            ))],
            Node::Branch(_, children) => self.sequence(children, width, depth, Spacing::Blocks),
            _ => wrap::wrap(
                &self.inline(std::slice::from_ref(node), Style::default()),
                width,
            ),
        }
    }

    fn cells(&self, children: &[Node<'_>], style: Style) -> Vec<table::Cell> {
        children
            .iter()
            .filter_map(|node| match node {
                Node::Branch(Tag::TableCell, children) => Some(self.inline(children, style)),
                _ => None,
            })
            .collect()
    }

    fn list(
        &mut self,
        children: &[Node<'_>],
        start: Option<u64>,
        width: usize,
        depth: usize,
    ) -> Vec<Line<'static>> {
        let items: Vec<_> = children
            .iter()
            .filter_map(|node| match node {
                Node::Branch(Tag::Item, children) => Some(children),
                _ => None,
            })
            .collect();
        let loose = items.iter().any(|children| {
            children
                .iter()
                .any(|node| matches!(node, Node::Branch(Tag::Paragraph, _)))
        });
        let marker_width = start.map_or(2, |start| {
            start
                .saturating_add(items.len().saturating_sub(1) as u64)
                .to_string()
                .len()
                + 2
        });
        let mut result = Vec::new();
        for (index, item) in items.into_iter().enumerate() {
            if loose && index > 0 {
                result.push(Line::default());
            }
            let marker = if let Some(done) = task(item) {
                if done {
                    "☑ ".to_owned()
                } else {
                    "☐ ".to_owned()
                }
            } else if let Some(start) = start {
                format!(
                    "{:<marker_width$}",
                    format!("{}.", start.saturating_add(index as u64))
                )
            } else {
                format!(
                    "{} ",
                    match depth {
                        0 => '•',
                        1 => '◦',
                        _ => '▪',
                    }
                )
            };
            let indent = " ".repeat(wrap::columns(&[Span::raw(marker.as_str())]));
            let mut lines = self.sequence(
                item,
                width.saturating_sub(indent.len()).max(1),
                depth + 1,
                if loose { Spacing::Item } else { Spacing::Tight },
            );
            if lines.is_empty() {
                lines.push(Line::default());
            }
            result.extend(prefix(
                lines,
                &marker,
                &indent,
                self.style(Style::default()),
                true,
            ));
        }
        result
    }

    fn inline(&self, children: &[Node<'_>], style: Style) -> Vec<Piece> {
        let mut result = Vec::new();
        self.inline_into(children, style, &mut result);
        result
    }

    fn inline_into(&self, children: &[Node<'_>], style: Style, output: &mut Vec<Piece>) {
        for node in children {
            match node {
                Node::Leaf(Event::Text(text) | Event::Html(text) | Event::InlineHtml(text)) => {
                    output.push(Piece::new(text.as_ref(), self.style(style), false))
                }
                Node::Leaf(Event::Code(text)) => output.push(Piece::new(
                    text.as_ref(),
                    self.style(style.patch(self.options.theme.inline_code)),
                    true,
                )),
                Node::Leaf(Event::SoftBreak | Event::HardBreak) => {
                    output.push(Piece::new("\n", self.style(style), true))
                }
                Node::Branch(
                    Tag::Link {
                        link_type,
                        dest_url,
                        ..
                    },
                    children,
                ) => {
                    self.inline_into(children, style.patch(self.options.theme.link_text), output);
                    if !matches!(link_type, LinkType::Autolink | LinkType::Email)
                        && plain_text(children) != dest_url.as_ref()
                    {
                        output.push(Piece::new(
                            format!(" ({dest_url})"),
                            self.style(style.patch(self.options.theme.link_destination)),
                            false,
                        ));
                    }
                }
                Node::Branch(Tag::Image { dest_url, .. }, children) => {
                    output.push(Piece::new(
                        format!("[image: {}]", plain_text(children)),
                        self.style(style.patch(self.options.theme.image_alt)),
                        false,
                    ));
                    output.push(Piece::new(
                        format!(" ({dest_url})"),
                        self.style(style.patch(self.options.theme.link_destination)),
                        false,
                    ));
                }
                Node::Branch(tag, children) => {
                    let added = match tag {
                        Tag::Strong => self.options.theme.strong,
                        Tag::Emphasis => self.options.theme.emphasis,
                        Tag::Strikethrough => self.options.theme.strikethrough,
                        _ => Style::default(),
                    };
                    self.inline_into(children, style.patch(added), output);
                }
                _ => {}
            }
        }
    }
}

fn is_block(node: &Node<'_>) -> bool {
    matches!(
        node,
        Node::Branch(
            Tag::Paragraph
                | Tag::Heading { .. }
                | Tag::BlockQuote(_)
                | Tag::CodeBlock(_)
                | Tag::List(_)
                | Tag::Table(_)
                | Tag::HtmlBlock,
            _
        ) | Node::Leaf(Event::Rule)
    )
}

fn task(children: &[Node<'_>]) -> Option<bool> {
    children.iter().find_map(|node| match node {
        Node::Leaf(Event::TaskListMarker(done)) => Some(*done),
        Node::Branch(Tag::Paragraph, children) => task(children),
        _ => None,
    })
}

fn plain_text(children: &[Node<'_>]) -> String {
    let mut result = String::new();
    for child in children {
        match child {
            Node::Leaf(
                Event::Text(text) | Event::Code(text) | Event::Html(text) | Event::InlineHtml(text),
            ) => result.push_str(text),
            Node::Leaf(Event::SoftBreak | Event::HardBreak) => result.push('\n'),
            Node::Branch(_, children) => result.push_str(&plain_text(children)),
            _ => {}
        }
    }
    result
}

fn prefix(
    lines: Vec<Line<'static>>,
    first: &str,
    continuation: &str,
    style: Style,
    blank_bars: bool,
) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let prefix = if index == 0 { first } else { continuation };
            let prefix = if line.spans.is_empty() {
                if blank_bars { prefix.trim_end() } else { "" }
            } else {
                prefix
            };
            let mut spans = Vec::new();
            wrap::append(&mut spans, prefix, style);
            for span in line.spans {
                wrap::append(&mut spans, &span.content, line.style.patch(span.style));
            }
            Line::from(spans)
        })
        .collect()
}
