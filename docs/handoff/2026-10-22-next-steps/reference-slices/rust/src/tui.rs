//! Terminal handling. A ratatui `Viewport::Fixed` rectangle that we move ourselves is the
//! live region; finished lines are written above it with a scroll region that starts at the
//! top of the screen, so the terminal pushes them into its own scrollback.
//! The insertion technique follows OpenAI Codex's `insert_history.rs` (Apache-2.0, see NOTICE).
use crate::state::OutLine;
use crate::state::State;
use crate::ui;
use anyhow::Result;
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::style::{Attribute, Color as CColor, Print, SetAttribute, SetForegroundColor};
use crossterm::terminal::{
    self, disable_raw_mode, enable_raw_mode, BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate,
};
use crossterm::{cursor, queue};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use ratatui::{Terminal, TerminalOptions, Viewport};
use std::io::{stdout, Stdout, Write};

pub struct Tui {
    term: Terminal<CrosstermBackend<Stdout>>,
    cols: u16,
    rows: u16,
    top: u16,
    height: u16,
}

fn make(rect: Rect) -> Result<Terminal<CrosstermBackend<Stdout>>> {
    Ok(Terminal::with_options(
        CrosstermBackend::new(stdout()),
        TerminalOptions { viewport: Viewport::Fixed(rect) },
    )?)
}

impl Tui {
    pub fn new(height: u16) -> Result<Self> {
        enable_raw_mode()?;
        let (cols, rows) = terminal::size()?;
        let (x, y) = cursor::position()?; // the only cursor query; done before any key reader runs
        let mut out = stdout();
        let mut top = y;
        if x != 0 {
            queue!(out, Print("\r\n"))?;
            top = (y + 1).min(rows.saturating_sub(1));
        }
        let height = height.min(rows);
        let mut me = Tui { term: make(Rect::new(0, top, cols, height))?, cols, rows, top, height };
        me.make_room(height)?;
        queue!(out, EnableBracketedPaste)?;
        out.flush()?;
        Ok(me)
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn width(&self) -> u16 {
        self.cols
    }

    /// Scroll the screen so a viewport of `h` rows starting at `self.top` fits.
    fn make_room(&mut self, h: u16) -> Result<()> {
        let mut out = stdout();
        if self.top + h > self.rows {
            let deficit = self.top + h - self.rows;
            queue!(out, cursor::MoveTo(0, self.rows - 1))?;
            for _ in 0..deficit {
                queue!(out, Print("\n"))?;
            }
            self.top = self.rows - h;
        }
        out.flush()?;
        Ok(())
    }

    pub fn set_height(&mut self, h: u16) -> Result<()> {
        let h = h.min(self.rows).max(1);
        if h == self.height {
            return Ok(());
        }
        let mut out = stdout();
        queue!(out, BeginSynchronizedUpdate, cursor::MoveTo(0, self.top), Clear(ClearType::FromCursorDown))?;
        self.make_room(h)?;
        self.height = h;
        self.term.resize(Rect::new(0, self.top, self.cols, h))?;
        queue!(out, EndSynchronizedUpdate)?;
        out.flush()?;
        Ok(())
    }

    pub fn resized(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.cols = cols;
        self.rows = rows;
        self.height = self.height.min(rows);
        self.top = self.top.min(rows - self.height);
        self.term.resize(Rect::new(0, self.top, cols, self.height))?;
        Ok(())
    }

    /// Write finished lines above the viewport; the terminal keeps what scrolls off.
    pub fn insert(&mut self, lines: &[OutLine]) -> Result<()> {
        if lines.is_empty() {
            return Ok(());
        }
        let n = lines.len() as u16;
        let mut out = stdout();
        queue!(out, BeginSynchronizedUpdate)?;
        let old_top = self.top;
        let mut moved = false;
        if self.top + self.height < self.rows {
            // Room below: slide the viewport down by reverse-indexing the rows under it.
            let shift = n.min(self.rows - (self.top + self.height));
            write!(out, "\x1b[{};{}r", self.top + 1 + 1, self.rows)?; // region: row below the top .. bottom (1-based)
            queue!(out, cursor::MoveTo(0, self.top))?;
            for _ in 0..shift {
                write!(out, "\x1bM")?; // reverse index
            }
            write!(out, "\x1b[r")?;
            self.top += shift;
            moved = true;
        }
        // Region = everything above the viewport; start at its last row (old_top - 1).
        let region_end = self.top.max(1);
        write!(out, "\x1b[1;{region_end}r")?;
        queue!(out, cursor::MoveTo(0, old_top.saturating_sub(1)))?;
        for (i, l) in lines.iter().enumerate() {
            // With the viewport on screen row 0 there is no row above it to start from.
            if !(i == 0 && old_top == 0) {
                queue!(out, Print("\r\n"))?;
            }
            write_line(&mut out, l)?;
        }
        write!(out, "\x1b[r")?;
        queue!(out, EndSynchronizedUpdate)?;
        out.flush()?;
        if moved {
            self.term.resize(Rect::new(0, self.top, self.cols, self.height))?;
        }
        Ok(())
    }

    pub fn draw(&mut self, s: &State, now_ms: u64) -> Result<()> {
        queue!(stdout(), BeginSynchronizedUpdate)?;
        self.term.draw(|f| ui::draw_live(f, s, now_ms))?;
        queue!(stdout(), EndSynchronizedUpdate)?;
        stdout().flush()?;
        Ok(())
    }

    pub fn restore(&mut self) -> Result<()> {
        let mut out = stdout();
        queue!(
            out,
            cursor::MoveTo(0, self.top),
            Clear(ClearType::FromCursorDown),
            DisableBracketedPaste,
            SetAttribute(Attribute::Reset),
            cursor::Show
        )?;
        out.flush()?;
        disable_raw_mode()?;
        Ok(())
    }
}

fn cc(c: Color) -> Option<CColor> {
    Some(match c {
        Color::Black => CColor::Black,
        Color::Red => CColor::DarkRed,
        Color::Green => CColor::DarkGreen,
        Color::Yellow => CColor::DarkYellow,
        Color::Cyan => CColor::DarkCyan,
        _ => return None,
    })
}

fn write_line(out: &mut impl Write, l: &OutLine) -> Result<()> {
    let line = ui::style_line(l);
    for sp in &line.spans {
        let st = line.style.patch(sp.style);
        if let Some(c) = st.fg.and_then(cc) {
            queue!(out, SetForegroundColor(c))?;
        }
        if st.add_modifier.contains(Modifier::BOLD) {
            queue!(out, SetAttribute(Attribute::Bold))?;
        }
        if st.add_modifier.contains(Modifier::DIM) {
            queue!(out, SetAttribute(Attribute::Dim))?;
        }
        queue!(out, Print(sp.content.as_ref()), SetAttribute(Attribute::Reset))?;
    }
    Ok(())
}

/// Key reader on its own thread (crossterm `poll`/`read`), feeding an async channel.
pub struct KeyThread {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl KeyThread {
    pub fn spawn(tx: tokio::sync::mpsc::UnboundedSender<crossterm::event::Event>) -> Self {
        use std::sync::atomic::Ordering::SeqCst;
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let s = stop.clone();
        std::thread::spawn(move || {
            while !s.load(SeqCst) {
                if let Ok(true) = crossterm::event::poll(std::time::Duration::from_millis(25)) {
                    if let Ok(ev) = crossterm::event::read() {
                        if tx.send(ev).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        KeyThread { stop }
    }

    pub fn stop(&self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}
