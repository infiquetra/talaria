//! Turn Markdown into text the gateway can speak.

/// Which structural rendering a session starts in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeechRender {
    /// The existing spoken form.
    Strip,
    /// A short digest.
    Digest,
}

/// The starting render; an absent or unknown value retains the existing form.
#[must_use]
pub fn speech_render(value: Option<&str>) -> SpeechRender {
    match value {
        Some("digest") => SpeechRender::Digest,
        _ => SpeechRender::Strip,
    }
}

/// One sentence ready for playback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpeechChunk {
    /// The audio epoch this sentence belongs to.
    pub audio_epoch: u64,
    /// Position in the reply, starting at zero.
    pub seq: u64,
    /// The sentence, without a trailing code body.
    pub text: String,
}

/// Extract a bounded structural digest without interpreting code or test tables.
#[must_use]
pub fn digest_text(markdown: &str, max_chars: usize) -> Option<String> {
    let mut digest = Digest::default();
    let lines: Vec<_> = markdown.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if let Some((marker, count)) = fence(container_line(line)) {
            digest.code_blocks += 1;
            let language = container_line(line)
                .trim_start()
                .chars()
                .skip(count)
                .collect::<String>();
            let is_diff = matches!(language.trim(), "diff" | "patch");
            digest.diffs += usize::from(is_diff);
            index += 1;
            while index < lines.len() && !closes_fence(lines[index], marker, count) {
                if is_diff {
                    digest.diff_path(lines[index]);
                }
                index += 1;
            }
            index += usize::from(index < lines.len());
        } else if line.starts_with("    ") || line.starts_with('\t') {
            digest.code_blocks += 1;
            index += 1;
            while index < lines.len()
                && (lines[index].starts_with("    ") || lines[index].starts_with('\t'))
            {
                index += 1;
            }
        } else if line.starts_with("diff --git ")
            || (line.starts_with("--- ")
                && lines
                    .get(index + 1)
                    .is_some_and(|line| line.starts_with("+++ ")))
        {
            digest.diffs += 1;
            while index < lines.len() && !lines[index].trim().is_empty() {
                digest.diff_path(lines[index]);
                index += 1;
            }
        } else if index + 1 < lines.len() && line.contains('|') && table_delimiter(lines[index + 1])
        {
            digest.tables += 1;
            index += 2;
            while index < lines.len()
                && !lines[index].trim().is_empty()
                && lines[index].contains('|')
            {
                digest.table_rows += 1;
                index += 1;
            }
        } else {
            digest.prose(line);
            index += 1;
        }
    }
    let mut sentences = Vec::new();
    if let Some(result) = digest.result {
        sentences.push(result);
    }
    sentences.extend(digest.headings);
    sentences.extend(digest.bullets);
    sentences.extend(digest.questions);
    for (count, singular, plural) in [
        (digest.code_blocks, "code block", "code blocks"),
        (digest.diffs, "diff", "diffs"),
        (digest.paths.len(), "diff path", "diff paths"),
        (digest.tables, "table", "tables"),
        (digest.table_rows, "table row", "table rows"),
    ] {
        if count > 0 {
            sentences.push(format!(
                "{count} {}.",
                if count == 1 { singular } else { plural }
            ));
        }
    }
    if digest.code_blocks + digest.diffs + digest.tables > 0 {
        sentences.push("Review it in the terminal.".to_owned());
    }
    let mut output = String::new();
    for sentence in sentences {
        // Every candidate is complete; an oversized candidate is never sliced.
        let length =
            output.chars().count() + usize::from(!output.is_empty()) + sentence.chars().count();
        if length > max_chars {
            break;
        }
        if !output.is_empty() {
            output.push(' ');
        }
        output.push_str(&sentence);
    }
    (!output.is_empty()).then_some(output)
}

/// Split completed prose into sentences, excluding fenced and indented code.
#[must_use]
pub fn sentence_chunks(text: &str, audio_epoch: u64) -> Vec<SpeechChunk> {
    let mut prose = String::new();
    let mut open = None;
    for line in text.lines() {
        if let Some((marker, count)) = open {
            if closes_fence(line, marker, count) {
                open = None;
            }
        } else if let Some(found) = fence(container_line(line)) {
            open = Some(found);
            prose.push('\n');
        } else if !line.starts_with("    ") && !line.starts_with('\t') {
            prose.push_str(line);
            prose.push('\n');
        }
    }
    sentences(&prose)
        .into_iter()
        .enumerate()
        .map(|(seq, text)| SpeechChunk {
            audio_epoch,
            seq: seq as u64,
            text,
        })
        .collect()
}

#[derive(Default)]
struct Digest {
    result: Option<String>,
    headings: Vec<String>,
    bullets: Vec<String>,
    questions: Vec<String>,
    code_blocks: usize,
    diffs: usize,
    paths: std::collections::BTreeSet<String>,
    tables: usize,
    table_rows: usize,
}

impl Digest {
    fn diff_path(&mut self, line: &str) {
        if let Some(path) = line
            .strip_prefix("+++ ")
            .or_else(|| line.strip_prefix("--- "))
        {
            let path = path.split_whitespace().next().unwrap_or_default();
            let path = path
                .strip_prefix("a/")
                .or_else(|| path.strip_prefix("b/"))
                .unwrap_or(path);
            if !path.is_empty() && path != "/dev/null" {
                self.paths.insert(path.to_owned());
            }
        }
    }

    fn prose(&mut self, line: &str) {
        if reference_definition(line) || horizontal_rule(line) {
            return;
        }
        let trimmed = line.trim_start();
        let heading = trimmed.starts_with('#') && strip_line_marker(line) != trimmed;
        let bullet = !heading && strip_line_marker(line) != trimmed;
        let clean = digest_inline(strip_line_marker(line));
        if !clean.chars().any(char::is_alphanumeric) {
            return;
        }
        for sentence in sentences(&clean) {
            let sentence = finish_sentence(sentence);
            if sentence.ends_with('?') {
                push_unique(&mut self.questions, sentence);
            } else if heading {
                push_unique(&mut self.headings, sentence);
            } else if explicit_result(&sentence) {
                if bullet {
                    push_unique(&mut self.bullets, sentence);
                } else if self.result.is_none() {
                    self.result = Some(sentence);
                }
            }
        }
    }
}

fn push_unique(items: &mut Vec<String>, sentence: String) {
    if items.len() < 3 && !items.contains(&sentence) {
        items.push(sentence);
    }
}

fn explicit_result(sentence: &str) -> bool {
    let lower = sentence.to_lowercase();
    let first = lower
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_end_matches(|ch: char| !ch.is_alphabetic());
    matches!(
        first,
        "done"
            | "shipped"
            | "fixed"
            | "added"
            | "updated"
            | "implemented"
            | "removed"
            | "changed"
            | "completed"
            | "verified"
            | "resolved"
    ) || lower.contains(" is done")
        || lower.contains(" are done")
        || lower.contains(" is fixed")
        || lower.contains(" is complete")
        || lower.starts_with("result:")
        || lower.starts_with("test result:")
        || lower.starts_with("tests passed")
        || lower.starts_with("tests failed")
        || lower.starts_with("all tests pass")
        || lower.starts_with("validation:")
        || lower.starts_with("checks:")
}

fn closes_fence(line: &str, marker: char, count: usize) -> bool {
    let line = container_line(line);
    fence(line).is_some_and(|(found, length)| {
        found == marker
            && length >= count
            && line
                .trim_start()
                .chars()
                .skip(length)
                .all(char::is_whitespace)
    })
}

fn container_line(mut line: &str) -> &str {
    // A quoted or list-nested fence still protects the entire code body.
    while let Some(quoted) = line.trim_start().strip_prefix('>') {
        line = quoted.strip_prefix(' ').unwrap_or(quoted);
    }
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        let stripped = strip_line_marker(line);
        if stripped != trimmed {
            return stripped;
        }
    }
    line
}

fn digest_inline(line: &str) -> String {
    // Inline code is content present, never a claim or prose to read aloud.
    let mut without_code = String::new();
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '`' {
            let mut width = 1;
            while chars.peek() == Some(&'`') {
                chars.next();
                width += 1;
            }
            let mut run = 0;
            for inner in chars.by_ref() {
                run = if inner == '`' { run + 1 } else { 0 };
                if run == width {
                    break;
                }
            }
            without_code.push(' ');
        } else if !ch.is_control() {
            without_code.push(ch);
        }
    }
    inline_text(&without_code)
        .split_whitespace()
        .filter(|word| {
            let lower = word.to_lowercase();
            !word.contains("://")
                && !lower.contains("www.")
                && !lower.contains("mailto:")
                && !lower.contains("tel:")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn finish_sentence(mut sentence: String) -> String {
    if !sentence.ends_with(['.', '!', '?', '。', '！', '？']) {
        sentence.push('.');
    }
    sentence
}

fn sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut result = Vec::new();
    let mut start = 0;
    for (index, ch) in chars.iter().enumerate() {
        let terminal = matches!(ch, '.' | '!' | '?' | '。' | '！' | '？');
        let boundary = chars.get(index + 1).is_none_or(|next| next.is_whitespace())
            || matches!(ch, '。' | '！' | '？');
        if terminal && boundary {
            let sentence = chars[start..=index].iter().collect::<String>();
            let sentence = sentence.split_whitespace().collect::<Vec<_>>().join(" ");
            if sentence.chars().any(char::is_alphanumeric) {
                result.push(sentence);
            }
            start = index + 1;
        }
    }
    let tail = chars[start..].iter().collect::<String>();
    let tail = tail.split_whitespace().collect::<Vec<_>>().join(" ");
    if tail.chars().any(char::is_alphanumeric) {
        result.push(tail);
    }
    result
}

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
    use super::{digest_text, sentence_chunks, speakable_text};

    #[test]
    fn digest_counts_content_without_claiming_edits_or_test_success() {
        let markdown = "I will investigate.\n\nFixed the display.\n\n\
```diff\n--- a/private.rs\n+++ b/private.rs\n-OLD_BODY\n+NEW_BODY\n```\n\
| Suite | Result |\n| --- | --- |\n| core | passed |\n| app | failed |";
        assert_eq!(
            digest_text(markdown, 1500).as_deref(),
            Some(
                "Fixed the display. 1 code block. 1 diff. 1 diff path. 1 table. 2 table rows. Review it in the terminal."
            )
        );
        assert_eq!(
            digest_text("Tests passed: 12 checks.", 1500).as_deref(),
            Some("Tests passed: 12 checks.")
        );
        assert_eq!(digest_text("I will run the tests.", 1500), None);
    }

    #[test]
    fn digest_bounds_structural_items_and_omits_inline_code_and_urls() {
        let markdown = "Updated `SECRET_COMMAND` and [the notes](https://host/secret).\n\
## First\n## Second\n## Third\n## Fourth\n\
- Fixed one.\n- Fixed two.\n- Fixed three.\n- Fixed four.\n\
Is this ready?\nShould I proceed?\nWould you review?\nCan I finish?\n\
## Visit ftp://host/private\n## Visit www.host/private";
        let spoken = digest_text(markdown, 1500).expect("structural summary");
        assert!(spoken.starts_with("Updated and the notes."), "{spoken}");
        for omitted in [
            "SECRET_COMMAND",
            "host",
            "private",
            "Fourth",
            "four",
            "Can I finish",
        ] {
            assert!(!spoken.contains(omitted), "{spoken}");
        }
        assert!(spoken.contains("Fixed three."), "{spoken}");
        assert!(spoken.contains("Would you review?"), "{spoken}");
    }

    #[test]
    fn digest_limits_count_unicode_characters_and_never_cut_sentences() {
        let markdown = "Fixed café.\n## Grüße\nShould I proceed?";
        let full = digest_text(markdown, 1500).expect("digest");
        for max_chars in 0..full.chars().count() + 2 {
            if let Some(spoken) = digest_text(markdown, max_chars) {
                assert!(spoken.chars().count() <= max_chars, "{max_chars}: {spoken}");
                assert!(full.starts_with(&spoken));
                assert!(spoken.ends_with(['.', '?']));
                assert!(!spoken.chars().any(char::is_control));
            }
        }
        assert_eq!(digest_text(markdown, 10), None);
        assert_eq!(digest_text(markdown, 11).as_deref(), Some("Fixed café."));
    }

    #[test]
    fn digest_and_chunks_treat_mismatched_fences_as_code() {
        let markdown = "Done.\n````rust\nSECRET\n```\nSTILL_SECRET\n````\nReady?";
        let digest = digest_text(markdown, 1500).expect("digest");
        assert!(!digest.contains("SECRET"), "{digest}");
        let chunks = sentence_chunks(markdown, 8);
        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.text.as_str())
                .collect::<Vec<_>>(),
            ["Done.", "Ready?"]
        );
        assert_eq!(chunks[1].audio_epoch, 8);
        assert_eq!(chunks[1].seq, 1);
        let unclosed = "Done.\n~~~diff\n--- a/private\n+++ b/private\n+SECRET";
        assert_eq!(sentence_chunks(unclosed, 1).len(), 1);
        let quoted = "> ```\n> Fixed SECRET.\n> ```\nDone.";
        assert_eq!(sentence_chunks(quoted, 1)[0].text, "Done.");
        assert!(
            !digest_text(quoted, 1500)
                .expect("digest")
                .contains("SECRET")
        );
        assert!(
            !digest_text(unclosed, 1500)
                .expect("digest")
                .contains("SECRET")
        );
    }

    #[test]
    fn chunks_handle_unicode_and_keep_completed_reply_tails() {
        let chunks = sentence_chunks("完了。次です！ Café is ready? Final note", 3);
        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.text.as_str())
                .collect::<Vec<_>>(),
            ["完了。", "次です！", "Café is ready?", "Final note"]
        );
        assert_eq!(sentence_chunks("  \n***\n", 2), []);
    }

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
