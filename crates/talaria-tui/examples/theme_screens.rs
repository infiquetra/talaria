//! Print the synthetic screens for the built-in themes.
//!
//! Each frame is a fixed conversation drawn through the theme resolver and the
//! views. The text is synthetic: no host, user name, address or path.

use talaria_tui::theme::screens;

fn main() -> std::io::Result<()> {
    let directory = std::path::Path::new("/tmp/talaria-487-screens");
    std::fs::create_dir_all(directory)?;
    for frame in screens::frames() {
        let background = frame
            .terminal_background
            .map(|color| color.hex())
            .unwrap_or_else(|| "none".to_owned());
        let variant = if frame.inline {
            if background == "#ffffff" {
                "inline-light"
            } else {
                "inline-dark"
            }
        } else {
            "workspace"
        };
        let path = directory.join(format!(
            "{}-{}x{}-{variant}.ansi",
            frame.theme_id, frame.width, frame.height
        ));
        std::fs::write(&path, frame.ansi)?;
        println!("{}", path.display());
    }
    Ok(())
}
