//! Spoken digests and sentence chunks. Strip stays the existing spoken form.

use talaria_core::voice::speakable::{
    SpeechRender, digest_text, sentence_chunks, speakable_text, speech_render,
};

#[test]
fn strip_still_omits_code() {
    assert_eq!(
        speakable_text("Hello.\n\n```\nsecret\n```", 1500).as_deref(),
        Some("Hello. (code omitted)")
    );
}

#[test]
fn digest_counts_code_and_does_not_read_it() {
    let spoken = digest_text("```rust\nfn main() {}\n```", 1500)
        .expect("a code-only reply has a spoken digest");
    assert_eq!(spoken, "1 code block. Review it in the terminal.");
}

#[test]
fn digest_keeps_the_result_and_counts_the_rest() {
    let markdown = "\
Shipped the notes.

## Layout

- Is the panel still light?

```diff
--- a/src/lib.rs
+++ b/src/lib.rs
-HUNK_LINE
+HUNK_LINE
```

| Suite | Result |
| --- | --- |
| voice | failed |

See https://example.com/secret-path for the rest.
";
    let spoken = digest_text(markdown, 1500).expect("a mixed reply has a spoken digest");
    assert!(spoken.contains("Shipped the notes."), "{spoken}");
    assert!(spoken.contains("Layout"), "{spoken}");
    assert!(spoken.contains("Is the panel still light?"), "{spoken}");
    assert!(spoken.contains("1 diff"), "{spoken}");
    assert!(spoken.contains("1 table"), "{spoken}");
    assert!(!spoken.contains("HUNK_LINE"), "{spoken}");
    assert!(!spoken.contains("https://"), "{spoken}");
    assert!(!spoken.contains("secret-path"), "{spoken}");
    assert!(!spoken.contains("failed"), "{spoken}");
    assert_eq!(
        digest_text(markdown, 1500).as_deref(),
        Some(spoken.as_str())
    );
}

#[test]
fn digest_cuts_at_a_sentence_end() {
    let spoken = digest_text("Alpha is done. Beta is done. Gamma is done.", 40)
        .expect("a long reply is cut on a sentence");
    assert!(spoken.ends_with('.'), "{spoken}");
    assert!(!spoken.contains("Gamma"), "{spoken}");
    assert!(spoken.chars().count() <= 40, "{spoken}");
}

#[test]
fn sentence_chunks_carry_the_audio_epoch() {
    let chunks = sentence_chunks("Alpha done. Beta done. Gamma done.", 4);
    assert_eq!(chunks.len(), 3, "one chunk per sentence");
    assert_eq!(chunks[0].text, "Alpha done.");
    assert_eq!(chunks[0].audio_epoch, 4);
    assert_eq!(chunks[0].seq, 0);
    assert_eq!(chunks[1].seq, 1);
    assert_eq!(chunks[2].text, "Gamma done.");
    assert_eq!(chunks[2].seq, 2);
}

#[test]
fn chunks_skip_an_unclosed_code_body() {
    let chunks = sentence_chunks("Wait.\n```\npartial code", 1);
    assert_eq!(
        chunks.first().map(|chunk| chunk.text.as_str()),
        Some("Wait.")
    );
    assert!(chunks.iter().all(|chunk| !chunk.text.contains("partial")));
}

#[test]
fn render_mode_selects_digest() {
    assert_eq!(speech_render(Some("digest")), SpeechRender::Digest);
    assert_eq!(speech_render(None), SpeechRender::Strip);
    assert_eq!(speech_render(Some("strip")), SpeechRender::Strip);
}
