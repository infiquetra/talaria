//! Dropped paths and paste routes. Issue 224. No file is read here.

use std::path::{Path, PathBuf};

use talaria_core::attachments::{
    CollapseRule, PasteRoute, drop_candidates, route_paste, sniff_image, unsupported_image,
};

fn home() -> &'static Path {
    Path::new("/srv/talaria-home")
}

fn paths(text: &str) -> Vec<(PathBuf, String)> {
    drop_candidates(text, home())
        .into_iter()
        .map(|candidate| (candidate.path, candidate.remainder))
        .collect()
}

#[test]
fn recognises_and_rejects_drop_shapes() {
    assert_eq!(
        paths("file:///tmp/a%20b.png"),
        vec![(PathBuf::from("/tmp/a b.png"), String::new())]
    );
    assert_eq!(
        paths("'/tmp/x y.png'"),
        vec![(PathBuf::from("/tmp/x y.png"), String::new())]
    );
    assert_eq!(
        paths("\"/tmp/x y.png\" what is it"),
        vec![(PathBuf::from("/tmp/x y.png"), "what is it".to_string())]
    );
    assert_eq!(
        paths(r"/tmp/x\ y.png"),
        vec![(PathBuf::from("/tmp/x y.png"), String::new())]
    );
    assert_eq!(
        paths("~/a.png"),
        vec![(PathBuf::from("/srv/talaria-home/a.png"), String::new())]
    );
    assert_eq!(
        paths("./a.png"),
        vec![(PathBuf::from("./a.png"), String::new())]
    );
    assert_eq!(
        paths("../a.png"),
        vec![(PathBuf::from("../a.png"), String::new())]
    );
    assert_eq!(
        paths("/tmp/a.png look here"),
        vec![
            (PathBuf::from("/tmp/a.png look here"), String::new()),
            (PathBuf::from("/tmp/a.png look"), "here".to_string()),
            (PathBuf::from("/tmp/a.png"), "look here".to_string()),
        ]
    );
    assert!(paths("/help").is_empty());
    assert!(paths("/api").is_empty());
    assert!(paths("/a").is_empty());
    assert_eq!(
        paths("/usr/lib"),
        vec![(PathBuf::from("/usr/lib"), String::new())]
    );
    assert!(paths("\n\nhello").is_empty());
}

#[test]
fn sniffs_and_routes_paste_kinds() {
    let png = sniff_image(b"\x89PNG\r\n\x1a\n").expect("png");
    assert_eq!((png.mime, png.ext), ("image/png", "png"));
    let jpeg = sniff_image(b"\xff\xd8\xff").expect("jpeg");
    assert_eq!((jpeg.mime, jpeg.ext), ("image/jpeg", "jpg"));
    let gif87 = sniff_image(b"GIF87a").expect("gif87");
    assert_eq!((gif87.mime, gif87.ext), ("image/gif", "gif"));
    let gif89 = sniff_image(b"GIF89a").expect("gif89");
    assert_eq!((gif89.mime, gif89.ext), ("image/gif", "gif"));
    let webp = sniff_image(b"RIFF\x00\x00\x00\x00WEBP").expect("webp");
    assert_eq!((webp.mime, webp.ext), ("image/webp", "webp"));
    assert!(sniff_image(b"BM").is_none());
    assert!(unsupported_image(b"BM", Path::new("x.png")));
    assert!(unsupported_image(b"not-an-image", Path::new("x.svg")));
    assert!(unsupported_image(b"BM", Path::new("x.TIFF")));

    let rule = CollapseRule::HERMES_DEFAULT;
    assert!(matches!(
        route_paste("", &rule, home()),
        PasteRoute::ProbeClipboard {
            fallback_text: None
        }
    ));
    assert!(matches!(
        route_paste("  \n", &rule, home()),
        PasteRoute::ProbeClipboard { fallback_text: Some(text) } if text == "  \n"
    ));
    assert!(matches!(
        route_paste("/tmp/red.png", &rule, home()),
        PasteRoute::Drop(_)
    ));
    assert!(matches!(
        route_paste("/tmp/pic.bmp", &rule, home()),
        PasteRoute::Drop(_)
    ));
    assert!(matches!(
        route_paste("/tmp/pic.svg", &rule, home()),
        PasteRoute::Drop(_)
    ));
    assert!(matches!(
        route_paste("./notes.txt", &rule, home()),
        PasteRoute::Drop(_)
    ));
    assert!(matches!(
        route_paste("/srv/talaria-home/.ssh/id_ed25519", &rule, home()),
        PasteRoute::Drop(_)
    ));
    let long = "x".repeat(2400);
    assert!(matches!(
        route_paste(&long, &rule, home()),
        PasteRoute::Collapse
    ));
    let lines = "line\n".repeat(120);
    assert!(matches!(
        route_paste(&lines, &rule, home()),
        PasteRoute::Collapse
    ));
    assert!(matches!(
        route_paste("a\nb\nc\nd", &rule, home()),
        PasteRoute::Inline
    ));
}
