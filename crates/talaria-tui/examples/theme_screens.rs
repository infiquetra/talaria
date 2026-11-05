//! Print the synthetic screens for the built-in themes.
//!
//! Each frame is a fixed conversation drawn through the theme resolver and the
//! views. The text is synthetic: no host, user name, address or path.

use talaria_tui::theme::screens;

fn main() {
    for frame in screens::frames() {
        let background = frame
            .terminal_background
            .map(|color| color.hex())
            .unwrap_or_else(|| "none".to_owned());
        println!(
            "# {} {}x{} inline {} background {background}",
            frame.theme_id, frame.width, frame.height, frame.inline
        );
        println!("{}", frame.text);
    }
}
