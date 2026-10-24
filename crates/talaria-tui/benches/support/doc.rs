//! Deterministic Markdown documents for the stream benchmark and the demo.

pub const DOCUMENT_BYTES: usize = 65_536;

const DELTA_SIZES: [usize; 10] = [30, 100, 45, 85, 60, 70, 55, 80, 65, 65];

#[allow(dead_code)]
pub fn document_names() -> [&'static str; 4] {
    ["mixed-64k", "paragraph-64k", "code-64k", "table-64k"]
}

pub fn document(name: &str) -> Option<String> {
    let text = match name {
        "mixed-64k" => grow(mixed_block),
        "paragraph-64k" => grow(paragraph_piece),
        "code-64k" => grow_after("```rust\n", code_line),
        "table-64k" => grow_after(
            "| Name | Qty | Notes |\n|:-----|----:|-------|\n",
            table_row,
        ),
        _ => return None,
    };
    Some(text)
}

pub fn cut_deltas(document: &str, sizes: &[usize]) -> Vec<String> {
    let sizes = if sizes.is_empty() {
        &DELTA_SIZES
    } else {
        sizes
    };
    let mut out = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while start < document.len() {
        let want = sizes[index % sizes.len()];
        let mut end = start.saturating_add(want).min(document.len());
        while end < document.len() && !document.is_char_boundary(end) {
            end += 1;
        }
        if end == start {
            end = document.len();
        }
        out.push(document[start..end].to_string());
        start = end;
        index += 1;
    }
    out
}

#[allow(dead_code)]
pub fn bench_deltas(document: &str) -> Vec<String> {
    cut_deltas(document, &DELTA_SIZES)
}

fn grow(mut piece: impl FnMut(usize) -> String) -> String {
    let mut out = String::new();
    let mut i = 1;
    while out.len() < DOCUMENT_BYTES {
        out.push_str(&piece(i));
        i += 1;
    }
    cut(out)
}

fn grow_after(header: &str, mut piece: impl FnMut(usize) -> String) -> String {
    let mut out = String::from(header);
    let mut i = 1;
    while out.len() < DOCUMENT_BYTES {
        out.push_str(&piece(i));
        i += 1;
    }
    cut(out)
}

fn cut(mut text: String) -> String {
    let mut end = DOCUMENT_BYTES.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}

fn mixed_block(i: usize) -> String {
    let body = r#"## Section {i}

A paragraph of ordinary prose with **bold**, *emphasis*, `inline code` and a [link](https://example.org/{i}) that runs long enough to wrap at eighty columns.

- first bullet with some text
  - nested bullet {i}
- second bullet

1. first step
2. second step

> a quoted line that also wraps when it is long enough to pass the width of the terminal

| Name | Qty | Notes |
|:-----|----:|-------|
| apple | {i} | fresh |
| pear | 12 | ripe fruit |

```python
def f(x):
    s = "text"  # comment {i}
    return x + 42
```

```rust
fn main() {
    let n = 42; // note {i}
    println!("{}", n);
}
```

"#;
    body.replace("{i}", &i.to_string())
}

fn paragraph_piece(i: usize) -> String {
    format!("The quick brown fox jumps over the lazy dog and keeps running far away, round {i}. ")
}

fn code_line(i: usize) -> String {
    format!("    let value_{i} = compute({i}, \"text\"); // step {i}\n")
}

fn table_row(i: usize) -> String {
    format!("| item {i} | {i} | note for item {i} |\n")
}
