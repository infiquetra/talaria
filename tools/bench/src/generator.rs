//! Writes the three replay files from recorded templates.

use std::path::Path;

use crate::budgets::Budgets;
use crate::error::BenchError;

/// Writes `empty.jsonl`, `reference-stream.jsonl` and `long-session.jsonl` into `out`.
pub fn generate(fixtures: &Path, out: &Path, budgets: &Budgets) -> Result<(), BenchError> {
    use serde_json::Value;
    use std::collections::BTreeMap;
    let mut templates = BTreeMap::new();
    let mut header = String::new();
    let mut stamps = Value::Null;
    for (file, kinds) in [
        (
            "plain-answer.jsonl",
            &["message.start", "message.delta", "message.complete"][..],
        ),
        ("tool-diff.jsonl", &["tool.start", "tool.complete"][..]),
    ] {
        let missing = |kind: &str| BenchError::TemplateMissing {
            file: file.into(),
            frame_type: kind.into(),
        };
        let text = std::fs::read_to_string(fixtures.join(file)).map_err(|_| missing(kinds[0]))?;
        if header.is_empty() {
            header = text
                .lines()
                .next()
                .ok_or_else(|| missing(kinds[0]))?
                .to_owned();
        }
        for line in text.lines().skip(1) {
            let record: Value = serde_json::from_str(line).map_err(|_| missing(kinds[0]))?;
            let params = &record["frame"]["params"];
            if record["dir"] == "in" && record["frame"]["method"] == "event" {
                // A live recording opens with gateway events that name no session, so the
                // stamps come from the first event that does; otherwise each template keeps
                // its own recording's session and the reducer drops the other one's events.
                if stamps.is_null() && params.get("session_id").is_some() {
                    stamps = params.clone();
                }
                if let Some(kind) = params["type"].as_str()
                    && kinds.contains(&kind)
                {
                    templates.entry(kind.to_owned()).or_insert(record);
                }
            }
        }
        for kind in kinds {
            if !templates.contains_key(*kind) {
                return Err(missing(kind));
            }
        }
    }
    std::fs::create_dir_all(out).map_err(|_| BenchError::Io("scenario directory".into()))?;
    std::fs::write(out.join("empty.jsonl"), format!("{header}\n"))
        .map_err(|_| BenchError::Io("scenario write".into()))?;
    let build = |long: bool| {
        let mut records = Vec::new();
        let mut emit =
            |kind: &str, t_ms: u64, turn: u64, text: Option<&str>, tool_id: Option<&str>| {
                let mut record = templates[kind].clone();
                let seq = records.len() as u64 + 1;
                record["seq"] = seq.into();
                record["tMs"] = t_ms.into();
                let params = &mut record["frame"]["params"];
                params["seq"] = seq.into();
                for key in [
                    "session_id",
                    "replay_epoch",
                    "authority_epoch",
                    "admission_id",
                ] {
                    if stamps.get(key).is_some() {
                        params[key] = stamps[key].clone();
                    }
                }
                if let Some(generation) = stamps["execution_generation"].as_u64() {
                    params["execution_generation"] = (generation + turn).into();
                }
                if let Some(text) = text {
                    params["payload"]["text"] = text.into();
                }
                if let Some(id) = tool_id {
                    params["payload"]["tool_id"] = id.into();
                }
                records.push(record);
            };
        if long {
            for turn in 0..budgets.scenario.long_session_turns {
                emit("message.start", 0, turn, None, None);
                let mut full = String::new();
                for part in 0..3 {
                    let text = format!(
                        "Turn {turn:06} part {part}: a measured response keeps the transcript concrete and readable.\n"
                    );
                    full.push_str(&text);
                    emit("message.delta", 0, turn, Some(&text), None);
                }
                emit("message.complete", 0, turn, Some(&full), None);
                let id = format!("bench-{turn:06}");
                emit("tool.start", 0, turn, None, Some(&id));
                emit("tool.complete", 0, turn, None, Some(&id));
            }
        } else {
            let mut at = 0;
            for turn in 0..2 {
                let text = reference_text(turn + 1, budgets.scenario.turn_seconds);
                emit("message.start", at, turn, None, None);
                let bytes = text.as_bytes();
                let mut offset = 0;
                let mut delta = 0;
                let mut batch = 0;
                while offset < bytes.len() {
                    at += 33;
                    let batch_count = if batch % 2 == 0 { 1 } else { 2 };
                    for _ in 0..batch_count {
                        if offset == bytes.len() {
                            break;
                        }
                        let end = (offset + [3, 5, 7, 4, 6][delta % 5]).min(bytes.len());
                        // Generated text is ASCII, so these byte cuts are character boundaries.
                        emit("message.delta", at, turn, Some(&text[offset..end]), None);
                        offset = end;
                        delta += 1;
                    }
                    batch += 1;
                }
                at += 33;
                emit("message.complete", at, turn, Some(&text), None);
            }
        }
        let mut text = format!("{header}\n");
        for record in records {
            text.push_str(&record.to_string());
            text.push('\n');
        }
        text
    };
    for (file, long) in [
        ("reference-stream.jsonl", false),
        ("long-session.jsonl", true),
    ] {
        std::fs::write(out.join(file), build(long))
            .map_err(|_| BenchError::Io("scenario write".into()))?;
    }
    Ok(())
}

fn reference_text(turn: u64, seconds: u64) -> String {
    let batches = (seconds as f64 * 1000.0 / 33.0).round() as usize;
    let target = (batches / 2 * 3 + batches % 2) * 5;
    let title = format!("## Reference turn {turn}\n");
    let last = format!("End of reference turn {turn}.\n");
    if turn == 1 {
        let mut text = title;
        let mut index = 0;
        while text.len() + last.len() < target {
            let line = match index % 8 {
                0 => format!("### Section {index:04}"),
                1 | 2 => format!("- Item {index:04} keeps the measured transcript concrete."),
                3 => format!("1. Step {index:04} checks the next part of the response."),
                4 => "| Column one | Column two | Column three |".into(),
                5 | 6 => format!("| Value {index:04} | stable | complete |"),
                _ => format!("Paragraph {index:04} makes the streaming response readable."),
            };
            text.push_str(&format!("{line:<59}\n"));
            index += 1;
        }
        text.push_str(&last);
        text
    } else {
        let fence = char::from(96).to_string().repeat(3);
        let lines = (1.1 * seconds as f64).round() as usize;
        let blocks: Vec<_> = (0..3)
            .map(|block| {
                let mut text = format!("{fence}python\n");
                for line in 0..lines {
                    text.push_str(&format!("value_{block}_{line:03} = compute({line:03})\n"));
                }
                text.push_str(&format!("{fence}\n"));
                text
            })
            .collect();
        let mut length = title.len() + last.len() + blocks.iter().map(String::len).sum::<usize>();
        let mut paragraphs = Vec::new();
        while length < target {
            let p = format!(
                "Paragraph {:04} explains this fixed example with concrete words.\n",
                paragraphs.len()
            );
            length += p.len();
            paragraphs.push(p);
        }
        let mut text = title;
        for quarter in 0..4 {
            let start = paragraphs.len() * quarter / 4;
            let end = paragraphs.len() * (quarter + 1) / 4;
            for p in &paragraphs[start..end] {
                text.push_str(p);
            }
            if let Some(block) = blocks.get(quarter) {
                text.push_str(block);
            }
        }
        text.push_str(&last);
        text
    }
}
