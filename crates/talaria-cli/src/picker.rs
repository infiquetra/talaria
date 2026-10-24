//! The resume list. Rows, the prompt, and the out-of-range hint go to `output`.

use std::io::{BufRead, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use talaria_app::launch::{Choice, ListRow, format_rows, parse_choice};

/// Print the rows and ask until the person picks a row, types an id, or cancels. `None` means
/// cancelled, or no rows to pick from; both end the launch with exit 0.
pub fn pick(
    rows: &[ListRow],
    input: &mut impl BufRead,
    output: &mut impl Write,
    profile: &str,
) -> std::io::Result<Option<String>> {
    if rows.is_empty() {
        writeln!(output, "No sessions found for profile {profile}.")?;
        output.flush()?;
        return Ok(None);
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64());
    write!(output, "{}", format_rows(rows, now))?;
    loop {
        write!(output, "Number or id (Enter to cancel): ")?;
        output.flush()?;
        let mut line = String::new();
        let read = input.read_line(&mut line)?;
        let choice = if read == 0 {
            Choice::Cancel
        } else {
            parse_choice(&line, rows.len())
        };
        match choice {
            Choice::Row(number) => return Ok(Some(rows[number - 1].session_id.clone())),
            Choice::Id(id) => return Ok(Some(id)),
            Choice::Cancel => {
                writeln!(output, "Cancelled.")?;
                output.flush()?;
                return Ok(None);
            }
            Choice::OutOfRange(number) => writeln!(
                output,
                "No row {number}. Enter a number from 1 to {}, an id, or press Enter to cancel.",
                rows.len()
            )?,
        }
    }
}
