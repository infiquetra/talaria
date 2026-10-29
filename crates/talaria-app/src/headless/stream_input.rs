//! Streaming input records and reader.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputRecord {
    User { text: String },
    Interrupt,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputError {
    TooLong,
    NotUtf8,
    NotJson,
    NotObject,
    NoType,
    UnknownType(String),
    BadKeys(&'static str),
    BadText,
    NoUserRecord,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputItem {
    Record(InputRecord),
    Error(InputError),
}

pub const MAX_LINE_BYTES: usize = crate::headless::input::MAX_STDIN_BYTES;
pub const INPUT_QUEUE: usize = 64;

impl InputError {
    pub fn what(&self, _line: Option<u64>) -> String {
        todo!("add stream-json implementation")
    }
}

pub fn parse_input_line(_line: &[u8]) -> Result<Option<InputRecord>, InputError> {
    todo!("add stream-json implementation")
}

pub struct InputReader {
    pub items: tokio::sync::mpsc::Receiver<(u64, InputItem)>,
}

pub fn spawn_input_reader<R: std::io::Read + Send + 'static>(
    _input: R,
    _queue: usize,
) -> InputReader {
    todo!("add stream-json implementation")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn parses_user_and_interrupt() {
        assert_eq!(
            parse_input_line(r#"{"type":"user","text":"  café\nnext  "}"#.as_bytes()),
            Ok(Some(InputRecord::User {
                text: "  café\nnext  ".into()
            }))
        );
        assert_eq!(
            parse_input_line(br#"{"type":"interrupt"}"#),
            Ok(Some(InputRecord::Interrupt))
        );
        assert_eq!(parse_input_line(b"  \t\r"), Ok(None));
    }

    #[test]
    fn refuses_bad_lines() {
        let long = vec![b'x'; MAX_LINE_BYTES + 1];
        let cases: Vec<(&[u8], InputError, &str)> = vec![
            (
                &long,
                InputError::TooLong,
                "line 7 is longer than 1048576 bytes.",
            ),
            (&[0xff], InputError::NotUtf8, "line 7 is not UTF-8."),
            (
                b"not json",
                InputError::NotJson,
                "line 7 is not valid JSON.",
            ),
            (b"[]", InputError::NotObject, "line 7 is not a JSON object."),
            (br#"{}"#, InputError::NoType, "line 7 has no type."),
            (br#"{"type":42}"#, InputError::NoType, "line 7 has no type."),
            (
                br#"{"type":"oops"}"#,
                InputError::UnknownType("oops".into()),
                "line 7 has unknown type \"oops\".",
            ),
            (
                br#"{"type":"user","text":"hi","extra":1}"#,
                InputError::BadKeys("user"),
                "line 7 has a key that a user record does not take.",
            ),
            (
                br#"{"type":"interrupt","text":"hi"}"#,
                InputError::BadKeys("interrupt"),
                "line 7 has a key that a interrupt record does not take.",
            ),
            (
                br#"{"type":"user"}"#,
                InputError::BadText,
                "line 7 needs a non-blank text string.",
            ),
            (
                br#"{"type":"user","text":4}"#,
                InputError::BadText,
                "line 7 needs a non-blank text string.",
            ),
            (
                br#"{"type":"user","text":"   "}"#,
                InputError::BadText,
                "line 7 needs a non-blank text string.",
            ),
        ];
        for (input, error, text) in cases {
            assert_eq!(parse_input_line(input), Err(error.clone()));
            assert_eq!(error.what(Some(7)), text);
        }
        assert_eq!(
            InputError::NoUserRecord.what(None),
            "no user record arrived before the end of standard input."
        );
        assert_eq!(
            InputError::UnknownType("a\nb".into()).what(Some(7)),
            "line 7 has unknown type \"a?b\"."
        );
        assert_eq!(
            InputError::UnknownType("x".repeat(41)).what(Some(7)),
            format!("line 7 has unknown type \"{}\".", "x".repeat(40))
        );
    }

    #[tokio::test]
    async fn line_limits_crlf_and_last_line() {
        let prefix = b"{\"type\":\"user\",\"text\":\"";
        let suffix = b"\"}";
        let mut valid = prefix.to_vec();
        valid.extend(vec![b'x'; MAX_LINE_BYTES - prefix.len() - suffix.len()]);
        valid.extend(suffix);
        assert_eq!(valid.len(), MAX_LINE_BYTES);
        assert!(matches!(
            parse_input_line(&valid),
            Ok(Some(InputRecord::User { .. }))
        ));
        let mut too_long = valid.clone();
        too_long.push(b' ');
        assert_eq!(parse_input_line(&too_long), Err(InputError::TooLong));
        let mut bounded = valid.clone();
        bounded.push(b'\n');
        bounded.extend_from_slice(&too_long);
        bounded.extend_from_slice(b"\n{\"type\":\"interrupt\"}\n");
        let mut limited = spawn_input_reader(Cursor::new(bounded), 1);
        assert!(matches!(
            limited.items.recv().await,
            Some((1, InputItem::Record(InputRecord::User { .. })))
        ));
        assert_eq!(
            limited.items.recv().await,
            Some((2, InputItem::Error(InputError::TooLong)))
        );
        assert_eq!(limited.items.recv().await, None);
        let mut reader = spawn_input_reader(
            Cursor::new(
                b"{\"type\":\"interrupt\"}\r\n{\"type\":\"user\",\"text\":\"last\"}".to_vec(),
            ),
            2,
        );
        assert_eq!(
            reader.items.recv().await,
            Some((1, InputItem::Record(InputRecord::Interrupt)))
        );
        assert_eq!(
            reader.items.recv().await,
            Some((
                2,
                InputItem::Record(InputRecord::User {
                    text: "last".into()
                })
            ))
        );
        assert_eq!(reader.items.recv().await, None);
    }

    #[tokio::test]
    async fn reader_keeps_line_order() {
        let lines = b"\n{\"type\":\"user\",\"text\":\"one\"}\n{\"type\":\"interrupt\"}\n{\"type\":\"user\",\"text\":\"two\"}\nnot json\n{\"type\":\"user\",\"text\":\"never\"}\n";
        let mut reader = spawn_input_reader(Cursor::new(lines.to_vec()), 1);
        assert_eq!(
            reader.items.recv().await,
            Some((
                2,
                InputItem::Record(InputRecord::User { text: "one".into() })
            ))
        );
        assert_eq!(
            reader.items.recv().await,
            Some((3, InputItem::Record(InputRecord::Interrupt)))
        );
        assert_eq!(
            reader.items.recv().await,
            Some((
                4,
                InputItem::Record(InputRecord::User { text: "two".into() })
            ))
        );
        assert_eq!(
            reader.items.recv().await,
            Some((5, InputItem::Error(InputError::NotJson)))
        );
        assert_eq!(reader.items.recv().await, None);
    }
}
