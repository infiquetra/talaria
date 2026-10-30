//! Turn Markdown into text the gateway can speak.

/// Spoken form of `markdown`, or nothing when no letter or digit remains.
#[must_use]
pub fn speakable_text(markdown: &str, max_chars: usize) -> Option<String> {
    let lines: Vec<_> = markdown.lines().collect();
    let mut blocks = String::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if let Some((marker, count)) = fence(line) {
            blocks.push_str(" (code omitted) ");
            index += 1;
            while index < lines.len() {
                let close = fence(lines[index]);
                index += 1;
                if close.is_some_and(|(found, length)| {
                    found == marker
                        && length >= count
                        && lines[index - 1]
                            .trim_start()
                            .chars()
                            .skip(length)
                            .all(char::is_whitespace)
                }) {
                    break;
                }
            }
        } else if index + 1 < lines.len() && line.contains('|') && table_delimiter(lines[index + 1])
        {
            blocks.push_str(" (table omitted) ");
            index += 2;
            while index < lines.len()
                && !lines[index].trim().is_empty()
                && lines[index].contains('|')
            {
                index += 1;
            }
        } else {
            if !reference_definition(line) && !horizontal_rule(line) {
                blocks.push_str(strip_line_marker(line));
                blocks.push('\n');
            }
            index += 1;
        }
    }
    let inline = inline_text(&blocks);
    let mut normalized = String::new();
    let mut space = false;
    for ch in inline.chars() {
        if ch.is_whitespace() {
            space = !normalized.is_empty();
        } else if !ch.is_control() {
            if space {
                normalized.push(' ');
            }
            space = false;
            normalized.push(ch);
        }
    }
    if !normalized.chars().any(char::is_alphanumeric) {
        return None;
    }
    if normalized.chars().count() <= max_chars {
        return Some(normalized);
    }
    if max_chars < 18 {
        return Some(
            normalized
                .chars()
                .take(max_chars)
                .collect::<String>()
                .trim()
                .to_owned(),
        );
    }
    let prefix: String = normalized.chars().take(max_chars - 18).collect();
    let cut = prefix
        .char_indices()
        .rev()
        .find(|(_, ch)| matches!(ch, '.' | '!' | '?'))
        .map(|(at, ch)| at + ch.len_utf8())
        .or_else(|| prefix.rfind(' '))
        .unwrap_or(prefix.len());
    let prefix = prefix[..cut].trim();
    Some(if prefix.is_empty() {
        "(reply truncated)".into()
    } else {
        format!("{prefix} (reply truncated)")
    })
}

fn fence(line: &str) -> Option<(char, usize)> {
    let indent = line.chars().take_while(|ch| *ch == ' ').count();
    if indent > 3 {
        return None;
    }
    let trimmed = &line[indent..];
    let marker = trimmed.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let count = trimmed.chars().take_while(|ch| *ch == marker).count();
    (count >= 3).then_some((marker, count))
}

fn table_delimiter(line: &str) -> bool {
    if !line.contains('|') {
        return false;
    }
    let body = line.trim();
    let body = body.strip_prefix('|').unwrap_or(body).trim();
    let body = body.strip_suffix('|').unwrap_or(body).trim();
    !body.is_empty()
        && body.split('|').all(|cell| {
            let cell = cell.trim();
            let cell = cell.strip_prefix(':').unwrap_or(cell);
            let cell = cell.strip_suffix(':').unwrap_or(cell);
            !cell.is_empty() && cell.chars().all(|ch| ch == '-')
        })
}

fn reference_definition(line: &str) -> bool {
    let indent = line.chars().take_while(|ch| *ch == ' ').count();
    let line = &line[indent..];
    indent <= 3 && line.starts_with('[') && line.find("]:").is_some_and(|at| at > 1)
}

fn horizontal_rule(line: &str) -> bool {
    let mut markers = line.chars().filter(|ch| !ch.is_whitespace());
    let Some(marker) = markers.next() else {
        return false;
    };
    if !matches!(marker, '-' | '*' | '_') {
        return false;
    }
    let rest: Vec<_> = markers.collect();
    rest.len() >= 2 && rest.iter().all(|ch| *ch == marker)
}

fn strip_line_marker(line: &str) -> &str {
    let trimmed = line.trim_start();
    let bytes = trimmed.as_bytes();
    let mut n = 0;
    if bytes.first() == Some(&b'#') {
        while bytes.get(n) == Some(&b'#') {
            n += 1;
        }
    } else if matches!(bytes.first(), Some(b'>' | b'-' | b'*' | b'+')) {
        n = 1;
    } else {
        while bytes.get(n).is_some_and(u8::is_ascii_digit) {
            n += 1;
        }
        if n == 0 || bytes.get(n) != Some(&b'.') {
            return trimmed;
        }
        n += 1;
    }
    if bytes.get(n) == Some(&b' ') {
        &trimmed[n + 1..]
    } else {
        trimmed
    }
}

fn inline_text(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '`' {
            let run = chars[i..].iter().take_while(|ch| **ch == '`').count();
            let mut end = i + run;
            while end < chars.len() && chars[end] != '\n' {
                if chars[end] == '`' {
                    let length = chars[end..].iter().take_while(|ch| **ch == '`').count();
                    if length == run {
                        break;
                    }
                    end += length;
                } else {
                    end += 1;
                }
            }
            if end < chars.len() && chars[end] == '`' {
                let mut inner: String = chars[i + run..end].iter().collect();
                if inner.starts_with(' ') && inner.ends_with(' ') && inner.len() >= 2 {
                    inner.remove(0);
                    inner.pop();
                }
                out.push_str(&inline_text(&inner));
                i = end + run;
                continue;
            }
        }
        let image = chars[i] == '!' && chars.get(i + 1) == Some(&'[');
        if chars[i] == '[' || image {
            let start = i + usize::from(image);
            if let Some(close) = (start + 1..chars.len()).find(|&at| chars[at] == ']')
                && let Some(open @ ('(' | '[')) = chars.get(close + 1).copied()
                && let Some(end) = matching_close(&chars, close + 1, open)
            {
                let label: String = chars[start + 1..close].iter().collect();
                out.push_str(&inline_text(&label));
                i = end + 1;
                continue;
            }
        }
        if (i == 0 || chars[i - 1].is_whitespace() || matches!(chars[i - 1], '(' | '[' | '<' | '"'))
            && (chars[i..].starts_with(&['h', 't', 't', 'p', ':', '/', '/'])
                || chars[i..].starts_with(&['h', 't', 't', 'p', 's', ':', '/', '/']))
        {
            let end = (i..chars.len())
                .find(|&at| chars[at].is_whitespace())
                .unwrap_or(chars.len());
            let mut suffix = end;
            while suffix > i && matches!(chars[suffix - 1], '.' | ',' | ';' | ':' | '!' | '?' | ')')
            {
                suffix -= 1;
            }
            out.push_str("(link)");
            out.extend(chars[suffix..end].iter());
            i = end;
            continue;
        }
        if matches!(chars[i], '*' | '_' | '~') {
            let marker = chars[i];
            let run = if chars.get(i + 1) == Some(&marker) {
                2
            } else {
                1
            };
            if marker != '~' || run == 2 {
                let mut end = i + run;
                while end + run <= chars.len() {
                    if chars[end..end + run].iter().all(|ch| *ch == marker) {
                        break;
                    }
                    end += 1;
                }
                if end > i + run && end + run <= chars.len() {
                    let inner: String = chars[i + run..end].iter().collect();
                    out.push_str(&inline_text(&inner));
                    i = end + run;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn matching_close(chars: &[char], start: usize, open: char) -> Option<usize> {
    let close = if open == '(' { ')' } else { ']' };
    let mut depth = 0usize;
    for (at, ch) in chars.iter().enumerate().skip(start) {
        if *ch == open {
            depth += 1;
        }
        if *ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(at);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::speakable_text;

    #[test]
    fn urls_in_inline_code_and_delimited_prose_are_not_spoken() {
        assert_eq!(
            speakable_text("See `https://example.org/private`.", 1500).as_deref(),
            Some("See (link).")
        );
        assert_eq!(
            speakable_text("See (https://example.org/private).", 1500).as_deref(),
            Some("See ((link)).")
        );
        assert_eq!(
            speakable_text("```rust\n```not-a-close\nsecret\n```\nSafe", 1500).as_deref(),
            Some("(code omitted) Safe")
        );
        assert_eq!(
            speakable_text("    ```\n    visible\n    ```", 1500).as_deref(),
            Some("``` visible ```")
        );
        assert_eq!(
            speakable_text("Name | Value\n::--- | ---\na | 1", 1500).as_deref(),
            Some("Name | Value ::--- | --- a | 1")
        );
    }

    #[test]
    fn markdown_cases() {
        let word = "Word. ".repeat(300);
        let thirteen: String = std::iter::repeat_n("Word.", 13)
            .collect::<Vec<_>>()
            .join(" ");
        let truncated = format!("{thirteen} (reply truncated)");
        assert_eq!(
            truncated.chars().count(),
            95,
            "the pinned cut is 95 characters"
        );
        let letters = "a".repeat(101);
        let cut_letters = format!("{} (reply truncated)", "a".repeat(82));
        assert_eq!(cut_letters.chars().count(), 100);
        let prose_100 = format!("A fine day. {}", "x".repeat(88));
        assert_eq!(prose_100.chars().count(), 100);

        let cases: &[(&str, usize, Option<&str>)] = &[
            (
                "# Done\n\nAll **tests** pass.",
                1500,
                Some("Done All tests pass."),
            ),
            (
                "Run:\n\n```bash\ncargo test\n```\nThen commit.",
                1500,
                Some("Run: (code omitted) Then commit."),
            ),
            (
                "See [the docs](https://x.example/a).",
                1500,
                Some("See the docs."),
            ),
            (
                "Visit https://x.example now.",
                1500,
                Some("Visit (link) now."),
            ),
            ("Use `cargo fmt`.", 1500, Some("Use cargo fmt.")),
            (
                "| a | b |\n|---|---|\n| 1 | 2 |",
                1500,
                Some("(table omitted)"),
            ),
            (
                "Name | Value\n--- | ---\na | 1",
                1500,
                Some("(table omitted)"),
            ),
            ("Pick a | b carefully.", 1500, Some("Pick a | b carefully.")),
            ("Use the http crate.", 1500, Some("Use the http crate.")),
            ("a ` b", 1500, Some("a ` b")),
            ("x](y", 1500, Some("x](y")),
            ("![chart](c.png)", 1500, Some("chart")),
            ("- one\n- two", 1500, Some("one two")),
            ("> quoted", 1500, Some("quoted")),
            ("***", 1500, None),
            ("```\nfn main() {}\n```", 1500, Some("(code omitted)")),
            ("Grüße, café. Fin.", 1500, Some("Grüße, café. Fin.")),
            ("Visit https://x.example.", 1500, Some("Visit (link).")),
            (
                "See [docs][1].\n\n[1]: https://x.example",
                1500,
                Some("See docs."),
            ),
            ("![](c.png) Done.", 1500, Some("Done.")),
            ("Up\n\n---\n\nDown", 1500, Some("Up Down")),
            ("    let x = 1;", 1500, Some("let x = 1;")),
        ];
        for (input, max_chars, expected) in cases {
            assert_eq!(
                speakable_text(input, *max_chars).as_deref(),
                *expected,
                "{input:?}"
            );
        }
        assert_eq!(
            speakable_text(&word, 100).as_deref(),
            Some(truncated.as_str()),
            "a long run of sentences cuts at the limit"
        );
        assert_eq!(
            speakable_text(&letters, 100).as_deref(),
            Some(cut_letters.as_str()),
            "a run with no break still fits the limit"
        );
        assert_eq!(speakable_text(&"a".repeat(100), 100), Some("a".repeat(100)));
        assert_eq!(
            speakable_text(&prose_100, 100).as_deref(),
            Some(prose_100.as_str()),
            "prose that already fits is unchanged"
        );
    }
}
