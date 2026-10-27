use super::IsoInstant;

pub fn parse_iso_instant(text: &str) -> Option<IsoInstant> {
    let b = text.as_bytes();
    if b.len() < 17
        || b.get(4) != Some(&b'-')
        || b.get(7) != Some(&b'-')
        || b.get(10) != Some(&b'T')
        || b.get(13) != Some(&b':')
    {
        return None;
    }
    fn number(b: &[u8]) -> Option<i64> {
        b.iter().try_fold(0_i64, |n, c| {
            c.is_ascii_digit().then(|| n * 10 + i64::from(c - b'0'))
        })
    }
    let year = number(b.get(..4)?)?;
    let month = number(b.get(5..7)?)?;
    let day = number(b.get(8..10)?)?;
    let hour = number(b.get(11..13)?)?;
    let minute = number(b.get(14..16)?)?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if day < 1 || day > days || hour > 23 || minute > 59 {
        return None;
    }
    let mut pos = 16;
    let mut second = 0;
    if b.get(pos) == Some(&b':') {
        second = number(b.get(pos + 1..pos + 3)?)?;
        if second > 59 {
            return None;
        }
        pos += 3;
        if b.get(pos) == Some(&b'.') {
            pos += 1;
            let begin = pos;
            while b.get(pos).is_some_and(u8::is_ascii_digit) {
                pos += 1;
            }
            if pos == begin {
                return None;
            }
        }
    }
    let offset = match b.get(pos) {
        Some(b'Z') if pos + 1 == b.len() => 0,
        Some(sign @ (b'+' | b'-')) if pos + 6 == b.len() && b.get(pos + 3) == Some(&b':') => {
            let h = number(b.get(pos + 1..pos + 3)?)?;
            let m = number(b.get(pos + 4..pos + 6)?)?;
            if h > 23 || m > 59 {
                return None;
            }
            (h * 3600 + m * 60) * if *sign == b'+' { 1 } else { -1 }
        }
        _ => return None,
    };
    // Gregorian civil-date conversion, with March as the first month.
    let y = year - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * shifted_month + 2) / 5 + day - 1;
    let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
    Some(IsoInstant {
        unix: days * 86400 + hour * 3600 + minute * 60 + second - offset,
        wall: format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            year, month, day, hour, minute
        ),
    })
}

pub fn relative(from_unix: i64, to_unix: i64) -> String {
    let delta = i128::from(to_unix) - i128::from(from_unix);
    let seconds = delta.abs();
    if seconds < 60 {
        return "now".into();
    }
    let text = if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        let (h, m) = (seconds / 3600, seconds % 3600 / 60);
        if m == 0 {
            format!("{h}h")
        } else {
            format!("{h}h {m}m")
        }
    } else {
        let (d, h) = (seconds / 86400, seconds % 86400 / 3600);
        if h == 0 {
            format!("{d}d")
        } else {
            format!("{d}d {h}h")
        }
    };
    if delta < 0 {
        format!("{text} ago")
    } else {
        format!("in {text}")
    }
}

pub fn duration_text(seconds: i64) -> String {
    let s = seconds.max(0);
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m {:02}s", s / 60, s % 60)
    } else {
        format!("{}h {:02}m", s / 3600, s % 3600 / 60)
    }
}

pub(super) fn utc_text(unix: i64) -> String {
    let days = unix.div_euclid(86400) + 719468;
    let era = days.div_euclid(146097);
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let s = unix.rem_euclid(86400);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_validate_calendar_zone_and_fraction() {
        assert_eq!(parse_iso_instant("1970-01-01T00:00Z").unwrap().unix, 0);
        assert_eq!(parse_iso_instant("1969-12-31T23:00-01:00").unwrap().unix, 0);
        assert_eq!(
            parse_iso_instant("2000-02-29T00:00:01.005Z").unwrap().wall,
            "2000-02-29 00:00"
        );
        for text in [
            "1900-02-29T00:00Z",
            "2026-04-31T00:00Z",
            "2026-00-01T00:00Z",
            "2026-01-00T00:00Z",
            "2026-01-01T24:00Z",
            "2026-01-01T00:60Z",
            "2026-01-01T00:00:60Z",
            "2026-01-01T00:00:00.Z",
            "2026-01-01T00:00+24:00",
            "2026-01-01T00:00+01:60",
            "2026-01-01T00:00Zx",
            "2026-01-01 00:00Z",
            "2026-01-01T00:00",
            "⏱️",
        ] {
            assert!(parse_iso_instant(text).is_none(), "{text}");
        }
        assert_eq!(utc_text(-1), "1969-12-31 23:59:59 UTC");
        assert_eq!(duration_text(-1), "0s");
        assert!(relative(i64::MIN, i64::MAX).starts_with("in "));
    }
}
