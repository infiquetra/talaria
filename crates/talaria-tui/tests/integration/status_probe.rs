//! Parsing the terminal's 7501 status-record capability answer.

use talaria_tui::terminal::{StatusSupport, parse_probe_reply};

const COMPACT_BEL_ANSWER: &[u8] =
    b"\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x07";
const COMPACT_ST_ANSWER: &[u8] =
    b"\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x1b\\";
const SPACED_BEL_ANSWER: &[u8] =
    b"\x1b]7501;? : states=idle,working,done,blocked,error : kinds=permission,question,auth\x07";
const SPACED_ST_ANSWER: &[u8] =
    b"\x1b]7501;? : states=idle,working,done,blocked,error : kinds=permission,question,auth\x1b\\";
const MARKER: &[u8] = b"\x1b[?62;22c";

#[test]
fn status_answer_with_bel_and_st() {
    let all_supported = StatusSupport {
        idle: true,
        working: true,
        done: true,
        blocked: true,
        error: true,
        permission: true,
        question: true,
        auth: true,
    };

    for (name, answer) in [
        ("compact BEL", COMPACT_BEL_ANSWER),
        ("compact ST", COMPACT_ST_ANSWER),
        ("spaced BEL", SPACED_BEL_ANSWER),
        ("spaced ST", SPACED_ST_ANSWER),
    ] {
        let bytes = [answer, MARKER].concat();
        let (reply, complete) = parse_probe_reply(&bytes);
        assert!(complete, "{name} reply must be complete");
        assert_eq!(
            reply.status,
            Some(all_supported),
            "{name} status answer must set every StatusSupport flag"
        );
    }
}

#[test]
fn missing_status_answer_leaves_support_unset() {
    let (reply, complete) = parse_probe_reply(MARKER);
    assert!(complete);
    assert_eq!(reply.status, None);
}

#[test]
fn status_answer_after_marker_is_not_stored() {
    for (name, answer) in [
        ("compact BEL", COMPACT_BEL_ANSWER),
        ("spaced BEL", SPACED_BEL_ANSWER),
    ] {
        let bytes = [MARKER, answer].concat();
        let (reply, complete) = parse_probe_reply(&bytes);
        assert!(complete, "{name} reply must be complete");
        assert_eq!(
            reply.status, None,
            "{name} answer after marker must not be stored"
        );
    }
}
