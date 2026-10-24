//! Tooling program that renders a Markdown file.
//!
//! This is the only program in this crate that writes terminal control sequences.
//! The markdown module returns styled lines and does not write them.

#[path = "../benches/support/doc.rs"]
mod doc;

use std::io::{self, IsTerminal, Write};
use std::time::{Duration, Instant};

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use talaria_tui::markdown::highlight::is_loaded;
use talaria_tui::markdown::{
    MarkdownOptions, MarkdownStream, MarkdownTheme, StreamUpdate, render_markdown, warm_up,
};

const ESC: u8 = 0x1b;

struct Args {
    requested_width: u16,
    stream: bool,
    delta_bytes: usize,
    delay_ms: u64,
    no_live: bool,
    trace: bool,
    color: bool,
    generate: Option<String>,
    file: Option<String>,
}

fn main() {
    let args = match parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => fail(message),
    };
    if args.requested_width < 8 {
        eprintln!("note: width {} raised to 8", args.requested_width);
    }
    let (cols, rows) = terminal_size();
    let width = args.requested_width.max(8).min(cols);
    if let Err(message) = wait_ready() {
        eprintln!("{message}");
    }
    let text = match read_input(&args) {
        Ok(text) => text,
        Err(message) => fail(message),
    };
    let options = MarkdownOptions {
        width,
        theme: MarkdownTheme::default(),
        color: args.color,
        base_style: Style::default(),
    };
    let tty = io::stdout().is_terminal();
    if args.stream {
        stream_text(&text, &options, &args, rows, tty);
    } else {
        for line in render_markdown(&text, &options) {
            write_line(&line, tty);
        }
    }
}

fn stream_text(text: &str, options: &MarkdownOptions, args: &Args, rows: u16, tty: bool) {
    let deltas = doc::cut_deltas(text, &[args.delta_bytes]);
    let mut markdown = MarkdownStream::new(options.clone());
    let live = tty && !args.no_live;
    let mut rows_drawn = 0u16;
    for (index, delta) in deltas.iter().enumerate() {
        let started = Instant::now();
        let update = markdown.push(delta);
        if args.trace {
            eprintln!(
                "push={} committed={} tail={} micros={}",
                index + 1,
                update.committed.len(),
                update.tail.len(),
                started.elapsed().as_micros()
            );
        }
        paint(&update, live, tty, rows, &mut rows_drawn);
        if args.delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(args.delay_ms));
        }
    }
    let update = markdown.finish();
    paint(&update, live, tty, rows, &mut rows_drawn);
}

fn paint(update: &StreamUpdate, live: bool, tty: bool, rows: u16, rows_drawn: &mut u16) {
    if !live {
        for line in &update.committed {
            write_line(line, tty);
        }
        return;
    }
    let mut out = Vec::new();
    if *rows_drawn > 0 {
        push_csi(&mut out, &rows_drawn.to_string(), b'F');
    }
    push_csi(&mut out, "", b'J');
    for line in &update.committed {
        push_styled(&mut out, line);
        out.push(b'\n');
    }
    let room = usize::from(rows.saturating_sub(1));
    let tail = tail_window(&update.tail, room);
    for line in tail {
        push_styled(&mut out, line);
        out.push(b'\n');
    }
    *rows_drawn = u16::try_from(tail.len()).unwrap_or(u16::MAX);
    emit(&out);
}

fn tail_window<'a>(tail: &'a [Line<'static>], room: usize) -> &'a [Line<'static>] {
    if room == 0 {
        return &[];
    }
    if tail.len() > room {
        &tail[tail.len() - room..]
    } else {
        tail
    }
}

fn write_line(line: &Line<'_>, styled: bool) {
    let mut out = Vec::new();
    if styled {
        push_styled(&mut out, line);
    } else {
        for span in &line.spans {
            out.extend_from_slice(span.content.as_bytes());
        }
    }
    out.push(b'\n');
    emit(&out);
}

fn push_styled(out: &mut Vec<u8>, line: &Line<'_>) {
    for span in &line.spans {
        push_sgr(out, line.style.patch(span.style));
        out.extend_from_slice(span.content.as_bytes());
    }
}

fn push_sgr(out: &mut Vec<u8>, style: Style) {
    let mut params: Vec<u16> = Vec::new();
    if has(style, Modifier::BOLD) {
        params.push(1);
    }
    if has(style, Modifier::DIM) {
        params.push(2);
    }
    if has(style, Modifier::ITALIC) {
        params.push(3);
    }
    if has(style, Modifier::UNDERLINED) {
        params.push(4);
    }
    if has(style, Modifier::CROSSED_OUT) {
        params.push(9);
    }
    if let Some(code) = style.fg.and_then(colour_code) {
        params.push(u16::from(code));
    }
    out.push(ESC);
    out.push(b'[');
    if params.is_empty() {
        out.push(b'0');
    } else {
        for (index, param) in params.iter().enumerate() {
            if index > 0 {
                out.push(b';');
            }
            out.extend_from_slice(param.to_string().as_bytes());
        }
    }
    out.push(b'm');
}

fn has(style: Style, modifier: Modifier) -> bool {
    style.add_modifier.contains(modifier) && !style.sub_modifier.contains(modifier)
}

fn colour_code(colour: Color) -> Option<u8> {
    Some(match colour {
        Color::Black => 30,
        Color::Red => 31,
        Color::Green => 32,
        Color::Yellow => 33,
        Color::Blue => 34,
        Color::Magenta => 35,
        Color::Cyan => 36,
        Color::Gray => 37,
        Color::DarkGray => 90,
        Color::LightRed => 91,
        Color::LightGreen => 92,
        Color::LightYellow => 93,
        Color::LightBlue => 94,
        Color::LightMagenta => 95,
        Color::LightCyan => 96,
        Color::White => 97,
        _ => return None,
    })
}

fn push_csi(out: &mut Vec<u8>, params: &str, final_byte: u8) {
    out.push(ESC);
    out.push(b'[');
    out.extend_from_slice(params.as_bytes());
    out.push(final_byte);
}

fn emit(bytes: &[u8]) {
    let mut stdout = io::stdout().lock();
    if stdout.write_all(bytes).is_err() || stdout.flush().is_err() {
        std::process::exit(1);
    }
}

fn terminal_size() -> (u16, u16) {
    match crossterm::terminal::size() {
        Ok((cols, rows)) if cols > 0 && rows > 0 => (cols, rows),
        _ => (80, 24),
    }
}

fn wait_ready() -> Result<(), String> {
    warm_up();
    let started = Instant::now();
    while !is_loaded() {
        if started.elapsed() >= Duration::from_secs(10) {
            return Err("highlighter was not ready within 10 seconds".to_string());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn read_input(args: &Args) -> Result<String, String> {
    if let Some(name) = &args.generate {
        return doc::document(name).ok_or_else(|| format!("cannot read {name}: unknown document"));
    }
    match args.file.as_deref() {
        Some("-") => {
            let mut buf = String::new();
            io::Read::read_to_string(&mut io::stdin(), &mut buf)
                .map_err(|err| format!("cannot read -: {err}"))?;
            Ok(buf)
        }
        Some(path) => {
            std::fs::read_to_string(path).map_err(|err| format!("cannot read {path}: {err}"))
        }
        None => Err("cannot read : missing path".to_string()),
    }
}

fn parse(tokens: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut args = Args {
        requested_width: 80,
        stream: false,
        delta_bytes: 64,
        delay_ms: 0,
        no_live: false,
        trace: false,
        color: std::env::var("NO_COLOR")
            .ok()
            .filter(|value| !value.is_empty())
            .is_none(),
        generate: None,
        file: None,
    };
    let mut tokens = tokens.peekable();
    while let Some(token) = tokens.next() {
        match token.as_str() {
            "--stream" => args.stream = true,
            "--no-live" => args.no_live = true,
            "--trace" => args.trace = true,
            "--no-color" => args.color = false,
            "--width" => args.requested_width = parse_u16(&required(&mut tokens, "--width")?)?,
            "--delta-bytes" => {
                args.delta_bytes = parse_usize(&required(&mut tokens, "--delta-bytes")?)?;
            }
            "--delay-ms" => args.delay_ms = parse_u64(&required(&mut tokens, "--delay-ms")?)?,
            "--generate" => args.generate = Some(required(&mut tokens, "--generate")?),
            other if other.starts_with("--") => {
                return Err(format!("unknown argument {other}"));
            }
            other => {
                if args.file.is_some() {
                    return Err(format!("cannot read {other}: unexpected extra path"));
                }
                args.file = Some(other.to_string());
            }
        }
    }
    if args.generate.is_none() && args.file.is_none() {
        return Err("cannot read : missing path".to_string());
    }
    Ok(args)
}

fn required(
    tokens: &mut std::iter::Peekable<impl Iterator<Item = String>>,
    flag: &str,
) -> Result<String, String> {
    tokens
        .next()
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn parse_u16(value: &str) -> Result<u16, String> {
    value.parse().map_err(|_| format!("bad number {value}"))
}

fn parse_usize(value: &str) -> Result<usize, String> {
    value.parse().map_err(|_| format!("bad number {value}"))
}

fn parse_u64(value: &str) -> Result<u64, String> {
    value.parse().map_err(|_| format!("bad number {value}"))
}

fn fail(message: String) -> ! {
    eprintln!("{message}");
    std::process::exit(2);
}
