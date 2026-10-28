//! Status-line pull-request segment red test. Issue #245.

use talaria_core::review::{LabelTone, PrLabel};
use talaria_tui::status::{ExternalSegment, Segment, StatusInputs, Tone, plan};

/// The segment drops right after `External` and before every other segment,
/// and sits just before it on the line. Relative order, never absolute numbers.
#[test]
fn pr_segment_priority() {
    assert_eq!(
        Segment::PullRequest.priority() + 1,
        Segment::External.priority(),
        "one step above External"
    );
    assert_eq!(
        Segment::PullRequest.position() + 1,
        Segment::External.position(),
        "just before External on the line"
    );
    for segment in Segment::ALL {
        if segment != Segment::External
            && segment != Segment::PullRequest
            && segment != Segment::Ultracode
        {
            assert!(
                segment.priority() < Segment::PullRequest.priority(),
                "{segment:?} drops after the pull request"
            );
        }
    }
    assert!(Segment::Ultracode.priority() > Segment::External.priority());
    assert_eq!(Segment::ALL.last(), Some(&Segment::Ultracode));
    let adjacent = Segment::ALL
        .windows(2)
        .any(|pair| pair == [Segment::PullRequest, Segment::External]);
    assert!(adjacent, "dropped last to first ends PullRequest, External");

    // Once inputs name a pull request, the plan carries its piece.
    let inputs = StatusInputs {
        branch: Some("talaria/work".to_owned()),
        pull_request: Some(PrLabel {
            text: "#12 open ✓".to_owned(),
            tone: LabelTone::Success,
        }),
        ..StatusInputs::default()
    };
    let external = ExternalSegment {
        text: "external".to_owned(),
        tone: Tone::Muted,
    };
    let line = plan(&inputs, Some(&external), 200);
    assert!(
        line.pieces
            .iter()
            .any(|(segment, _, _)| *segment == Segment::PullRequest),
        "the plan carries the pull request: {:?}",
        line.pieces
            .iter()
            .map(|(segment, _, _)| segment)
            .collect::<Vec<_>>()
    );

    // Shrinking the line drops External first, then the pull request.
    let mut dropped_external = false;
    for cols in (40..=200).rev() {
        let line = plan(&inputs, Some(&external), cols);
        let kept: Vec<Segment> = line.pieces.iter().map(|(segment, _, _)| *segment).collect();
        if !kept.contains(&Segment::External) {
            dropped_external = true;
        }
        if dropped_external && kept.contains(&Segment::External) {
            panic!("External comes back at {cols} columns");
        }
        if !kept.contains(&Segment::PullRequest) {
            assert!(
                dropped_external,
                "the pull request drops only after External, at {cols} columns"
            );
            break;
        }
    }
}
