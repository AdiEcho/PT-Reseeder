pub fn format_bytes(bytes: i64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;
    let b = bytes as f64;
    if b >= TB {
        format!("{:.2} TB", b / TB)
    } else if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

pub fn format_duration(seconds: i64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h", hours)
    } else {
        let mins = seconds / 60;
        format!("{}m", mins)
    }
}

/// Minutes to add to a UTC timestamp to reach the viewer's local time
/// (`Asia/Shanghai` → `480`).
///
/// Returns `None` outside the browser: an SSR render has no viewer timezone,
/// and the process timezone is not it.
pub fn local_tz_offset_minutes() -> Option<i32> {
    #[cfg(target_arch = "wasm32")]
    {
        // `Date::getTimezoneOffset` counts the other way — UTC+8 reports -480 —
        // and reflects the offset in effect right now, so a DST switch inside
        // the visible range shifts older rows by an hour. Acceptable for a log
        // viewer, and it keeps one offset for the whole page.
        let offset = js_sys::Date::new_0().get_timezone_offset();
        if offset.is_nan() {
            None
        } else {
            Some(-(offset as i32))
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// Renders a UTC ISO-8601 timestamp as `YYYY-MM-DD HH:MM:SS.mmm` in the
/// timezone `offset_minutes` describes.
///
/// Returns `raw` untouched when the offset is unknown or the input is not the
/// expected shape. Callers rely on that: the server and the first hydration
/// pass both see `None` and so emit identical markup, and only the post-mount
/// offset turns the cell into local time.
pub fn format_local_timestamp(raw: &str, offset_minutes: Option<i32>) -> String {
    let Some(offset) = offset_minutes else {
        return raw.to_string();
    };
    let Some((year, month, day, hour, minute, second, frac)) = parse_utc_iso(raw) else {
        return raw.to_string();
    };

    let shifted = days_from_civil(year, month, day) * 1440
        + i64::from(hour) * 60
        + i64::from(minute)
        + i64::from(offset);
    let (year, month, day) = civil_from_days(shifted.div_euclid(1440));
    let minutes_of_day = shifted.rem_euclid(1440);

    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{second:02}{}",
        minutes_of_day / 60,
        minutes_of_day % 60,
        if frac.is_empty() {
            String::new()
        } else {
            format!(".{}", &frac[..frac.len().min(3)])
        },
    )
}

/// `(year, month, day, hour, minute, second, fractional_digits)`.
type UtcParts<'a> = (i64, u32, u32, u32, u32, u32, &'a str);

/// Splits `YYYY-MM-DDTHH:MM:SS[.fraction]Z` into its parts, with the fractional
/// digits left as written.
fn parse_utc_iso(raw: &str) -> Option<UtcParts<'_>> {
    let (date, time) = raw.strip_suffix('Z')?.split_once('T')?;

    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() {
        return None;
    }

    let (time, frac) = time.split_once('.').unwrap_or((time, ""));
    let mut time_parts = time.split(':');
    let hour: u32 = time_parts.next()?.parse().ok()?;
    let minute: u32 = time_parts.next()?.parse().ok()?;
    let second: u32 = time_parts.next()?.parse().ok()?;
    if time_parts.next().is_some() {
        return None;
    }

    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
        || !frac.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }

    Some((year, month, day, hour, minute, second, frac))
}

/// Days from `1970-01-01` to the given proleptic Gregorian date, and back —
/// Howard Hinnant's `days_from_civil` / `civil_from_days`.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    // March-based years put the leap day last, so the month-length pattern
    // becomes a single linear formula.
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted_month = i64::from((month + 9) % 12);
    let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::{civil_from_days, days_from_civil, format_local_timestamp};

    #[test]
    fn formats_timestamp_in_the_given_offset() {
        assert_eq!(
            format_local_timestamp("2026-08-13T02:52:57.354Z", Some(480)),
            "2026-08-13 10:52:57.354",
        );
        // Sub-second digits are truncated to milliseconds, not rounded.
        assert_eq!(
            format_local_timestamp("2026-07-29T02:38:20.490753Z", Some(480)),
            "2026-07-29 10:38:20.490",
        );
        // A timestamp without a fractional part keeps none.
        assert_eq!(
            format_local_timestamp("2026-07-29T02:38:20Z", Some(480)),
            "2026-07-29 10:38:20",
        );
    }

    #[test]
    fn carries_the_offset_across_date_boundaries() {
        // Day, month and year rollovers forward…
        assert_eq!(
            format_local_timestamp("2026-08-12T20:00:00.000Z", Some(480)),
            "2026-08-13 04:00:00.000",
        );
        assert_eq!(
            format_local_timestamp("2025-12-31T16:30:00.000Z", Some(480)),
            "2026-01-01 00:30:00.000",
        );
        // …and backwards, for offsets west of UTC.
        assert_eq!(
            format_local_timestamp("2026-01-01T02:00:00.000Z", Some(-300)),
            "2025-12-31 21:00:00.000",
        );
        // Leap day into March.
        assert_eq!(
            format_local_timestamp("2028-02-29T20:00:00.000Z", Some(480)),
            "2028-03-01 04:00:00.000",
        );
    }

    #[test]
    fn returns_input_unchanged_without_an_offset() {
        // The SSR and first-hydration path: byte-identical to what the server
        // already renders today.
        assert_eq!(
            format_local_timestamp("2026-08-13T02:52:57.354Z", None),
            "2026-08-13T02:52:57.354Z",
        );
    }

    #[test]
    fn returns_input_unchanged_when_it_is_not_a_utc_timestamp() {
        for raw in [
            "",
            "not a timestamp",
            // Unparsed lines reach the viewer with an empty timestamp field.
            "2026-08-13T02:52:57.354",  // no zone marker
            "2026-08-13 02:52:57.354Z", // no date/time separator
            "2026-08-13T02:52Z",        // seconds missing
            "2026-13-01T00:00:00Z",     // month out of range
            "2026-08-13T24:00:00Z",     // hour out of range
            "2026-08-13T02:52:57.35xZ", // non-numeric fraction
        ] {
            assert_eq!(format_local_timestamp(raw, Some(480)), raw);
        }
    }

    #[test]
    fn civil_day_conversion_round_trips() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        for date in [
            (1970, 1, 1),
            (1999, 12, 31),
            (2000, 2, 29),
            (2026, 8, 13),
            (2100, 3, 1),
        ] {
            let (year, month, day) = date;
            assert_eq!(civil_from_days(days_from_civil(year, month, day)), date);
        }
    }
}
