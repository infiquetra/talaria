use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};

pub(crate) fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

pub(crate) struct Block {
    pub range: Range<usize>,
    pub closed_fence: bool,
}

/// Locate complete top-level containers without treating a partial line as evidence.
pub(crate) fn scan(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let mut fenced = false;
    for (event, range) in Parser::new_ext(text, options()).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if depth == 0 {
                    start = range.start;
                    fenced = matches!(tag, Tag::CodeBlock(CodeBlockKind::Fenced(_)));
                }
                depth += 1;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let range = start..range.end;
                    let closed_fence = fenced && fence_closed(&text[range.clone()]);
                    blocks.push(Block {
                        range,
                        closed_fence,
                    });
                }
            }
            _ if depth == 0 => blocks.push(Block {
                range,
                closed_fence: false,
            }),
            _ => {}
        }
    }
    blocks
}

fn fence_closed(text: &str) -> bool {
    let mut lines = text.lines();
    let Some(opening) = lines.next() else {
        return false;
    };
    let Some(last) = lines.last() else {
        return false;
    };
    let opening = opening.trim_start_matches(' ');
    let Some(ch @ ('`' | '~')) = opening.chars().next() else {
        return false;
    };
    let count = opening.chars().take_while(|&c| c == ch).count();
    let leading = last.len() - last.trim_start_matches(' ').len();
    let closing = last.trim();
    leading <= 3 && closing.len() >= count && closing.chars().all(|c| c == ch)
}
