//! The status line names the connected profile when it is not the default one.

use talaria_tui::status::{Segment, StatusInputs, Tone, plan};

fn inputs(profile: Option<&str>) -> StatusInputs {
    StatusInputs {
        profile: profile.map(str::to_owned),
        model: Some("fixture-model".into()),
        working_dir: Some("fixture/project".into()),
        ..StatusInputs::default()
    }
}

fn profile_text(inputs: &StatusInputs, columns: u16) -> Option<String> {
    plan(inputs, None, columns)
        .pieces
        .into_iter()
        .find(|(kind, _, _)| *kind == Segment::Profile)
        .map(|(_, text, _)| text)
}

#[test]
fn another_profile_is_named_before_the_model() {
    let input = inputs(Some("reviewer"));
    let wide = plan(&input, None, 120);
    let at = wide
        .pieces
        .iter()
        .position(|(kind, _, _)| *kind == Segment::Profile)
        .expect("the profile segment is shown");
    assert_eq!(wide.pieces[at].1, "profile reviewer");
    assert_eq!(wide.pieces[at].2, Tone::Accent);
    assert_eq!(wide.pieces[at + 1].0, Segment::Model);
}

#[test]
fn the_default_profile_and_an_unknown_profile_show_nothing() {
    assert_eq!(profile_text(&inputs(None), 120), None);
    assert_eq!(profile_text(&inputs(Some("")), 120), None);
    assert_eq!(profile_text(&inputs(Some("  ")), 120), None);
}

#[test]
fn the_profile_outlasts_the_folder_and_context_on_a_narrow_line() {
    let input = StatusInputs {
        context_percent: Some(34),
        ..inputs(Some("reviewer"))
    };
    assert!(Segment::Profile.priority() < Segment::Context.priority());
    assert!(Segment::Profile.priority() < Segment::Folder.priority());
    assert!(Segment::Profile.priority() > Segment::Model.priority());
    // At every width, the folder is never kept while the profile is dropped, and some width
    // keeps the profile without the folder.
    let mut profile_alone = false;
    for columns in 10..=120 {
        let kinds: Vec<Segment> = plan(&input, None, columns)
            .pieces
            .iter()
            .map(|(kind, _, _)| *kind)
            .collect();
        let (profile, folder) = (
            kinds.contains(&Segment::Profile),
            kinds.contains(&Segment::Folder),
        );
        assert!(profile || !folder, "{columns} columns: {kinds:?}");
        profile_alone |= profile && !folder;
    }
    assert!(profile_alone);
}

#[test]
fn a_hostile_profile_name_is_made_safe() {
    let text = profile_text(&inputs(Some("re\u{202e}viewer\u{1b}[31m")), 120).unwrap();
    assert!(
        !text.contains('\u{202e}') && !text.contains('\u{1b}'),
        "{text:?}"
    );
    assert!(text.starts_with("profile re"), "{text:?}");
}
