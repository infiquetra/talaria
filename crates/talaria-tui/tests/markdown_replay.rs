mod markdown_support;

use std::path::Path;

use markdown_support::{dump, frames_dir, options};
use serde_json::Value;
use talaria_tui::markdown::{MarkdownStream, render_markdown};

#[test]
fn recorded_frames_stream_as_whole() {
    let mut files: Vec<_> = std::fs::read_dir(frames_dir())
        .unwrap_or_else(|err| panic!("cannot read frame fixtures: {err}"))
        .map(|entry| entry.unwrap_or_else(|err| panic!("cannot read a frame entry: {err}")))
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no frame fixtures were found");

    let mut saw_delta_text = false;
    let mut pending = Vec::new();
    for path in &files {
        let (segments, saw) = read_segments(path);
        saw_delta_text |= saw;
        pending.extend(segments);
    }
    assert!(saw_delta_text, "no fixture has a message.delta with text");

    for segment in pending {
        let whole = segment.concat();
        if whole.contains("]:") {
            eprintln!("skipped one segment because it contains a link reference marker");
            continue;
        }
        let opts = options(80);
        let mut markdown = MarkdownStream::new(opts.clone());
        let mut got = Vec::new();
        for delta in &segment {
            got.extend(markdown.push(delta).committed);
        }
        let done = markdown.finish();
        assert!(done.tail.is_empty());
        got.extend(done.committed);
        assert_eq!(dump(&got), dump(&render_markdown(&whole, &opts)));
    }
}

fn read_segments(path: &Path) -> (Vec<Vec<String>>, bool) {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()));
    let mut segments = Vec::new();
    let mut current = Vec::new();
    let mut saw_delta_text = false;
    for (index, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line).unwrap_or_else(|err| {
            panic!("cannot parse {} line {}: {err}", path.display(), index + 1)
        });
        if value.get("kind").and_then(Value::as_str) != Some("frame") {
            continue;
        }
        let params = &value["frame"]["params"];
        let kind = params.get("type").and_then(Value::as_str).unwrap_or("");
        match kind {
            "message.start" | "message.complete" => {
                if !current.is_empty() {
                    segments.push(std::mem::take(&mut current));
                }
            }
            "message.delta" => match params.pointer("/payload/text") {
                Some(Value::String(delta)) => {
                    saw_delta_text = true;
                    current.push(delta.clone());
                }
                Some(Value::Null) if !current.is_empty() => {
                    segments.push(std::mem::take(&mut current));
                }
                _ => {}
            },
            _ => {}
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    (segments, saw_delta_text)
}
