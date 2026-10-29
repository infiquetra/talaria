use super::text::clean;
use super::{RunRow, RunRowView, RunStatus, duration_text, relative};
use unicode_segmentation::UnicodeSegmentation;

pub fn run_status(row: &RunRow) -> RunStatus {
    match row.source.as_str() {
        "cron" if row.scheduler_owned || (row.ended_at.is_none() && row.is_active) => {
            RunStatus::Running
        }
        "cron" if row.ended_at.is_none() => RunStatus::Unclosed,
        "cron" => RunStatus::Finished(row.end_reason.clone()),
        "cron_output" => {
            let prefix = row
                .title
                .as_deref()
                .unwrap_or("")
                .split(" · ")
                .next()
                .unwrap_or("");
            let word = (!prefix.is_empty()
                && prefix.bytes().any(|b| b.is_ascii_uppercase())
                && prefix
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b == b' ' || b == b'_'))
            .then(|| prefix.to_ascii_lowercase());
            RunStatus::Output(word)
        }
        _ => RunStatus::Other(row.source.clone()),
    }
}

pub fn run_status_text(status: &RunStatus) -> String {
    match status {
        RunStatus::Running => "running".into(),
        RunStatus::Unclosed => "did not finish".into(),
        RunStatus::Finished(reason) => format!("finished{}", suffix(reason)),
        RunStatus::Output(reason) => format!("output{}", suffix(reason)),
        RunStatus::Other(source) => clean(source),
    }
}

fn suffix(reason: &Option<String>) -> String {
    reason
        .as_deref()
        .filter(|s| !s.is_empty())
        .map_or(String::new(), |s| format!(" · {}", clean(s)))
}

pub(super) fn row_view(row: &RunRow, wall: i64) -> RunRowView {
    RunRowView {
        start: row
            .started_at
            .map_or(String::new(), |s| relative(wall, s as i64)),
        status: run_status_text(&run_status(row)),
        duration: match (row.started_at, row.ended_at) {
            (Some(a), Some(b)) => duration_text((b - a) as i64),
            _ => String::new(),
        },
        tokens: if row.input_tokens > 0 || row.output_tokens > 0 {
            format!("{}/{} tokens", row.input_tokens, row.output_tokens)
        } else {
            String::new()
        },
        title: clean(
            row.title
                .as_deref()
                .or(row.preview.as_deref())
                .unwrap_or(&row.id),
        ),
    }
}

pub(super) fn run_card(row: &RunRow, wall: i64) -> Vec<(String, Vec<String>)> {
    let mut fields = Vec::new();
    let mut field = |label: &str, value: &str| {
        let lines = super::split_and_sanitise(value, usize::MAX)
            .into_iter()
            .flat_map(|line| {
                let clusters: Vec<&str> = line.graphemes(true).collect();
                if clusters.is_empty() {
                    vec![String::new()]
                } else {
                    clusters.chunks(40).map(|chunk| chunk.concat()).collect()
                }
            })
            .collect();
        fields.push((label.into(), lines))
    };
    field("Id", &row.id);
    field("Source", &row.source);
    for (label, epoch) in [("Started", row.started_at), ("Ended", row.ended_at)] {
        if let Some(epoch) = epoch {
            field(
                label,
                &format!(
                    "{} ({})",
                    relative(wall, epoch as i64),
                    super::time::utc_text(epoch as i64)
                ),
            );
        }
    }
    if let (Some(a), Some(b)) = (row.started_at, row.ended_at) {
        field("Duration", &duration_text((b - a) as i64));
    }
    if let Some(model) = &row.model {
        field("Model", model);
    }
    field("Messages", &row.message_count.to_string());
    field("Tool calls", &row.tool_call_count.to_string());
    field(
        "Tokens",
        &format!("{}/{}", row.input_tokens, row.output_tokens),
    );
    if let Some(reason) = &row.end_reason {
        field("End reason", reason);
    }
    if let Some(title) = &row.title {
        field("Title", title);
    }
    if let Some(preview) = &row.preview {
        field("Preview", preview);
    }
    fields
}
