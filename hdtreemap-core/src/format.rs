//! Human-readable formatting for the UI: sizes, fractions, counts and durations (volume
//! cards, the scan and results screens; the status bar, folder tree and legend later). Pure
//! functions, unit-tested here.

use std::time::Duration;

/// Size units for [`format_bytes`], each 1024× the previous.
const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];

/// Format a byte count with base-1024 units and one decimal: `0 B`, `512 B`, `1.0 KB`,
/// `1.5 MB`, `2.0 TB`. Anything below 1 KB is shown in whole bytes; terabytes are the
/// largest unit. A value that would round up to `1024.0` of a unit is shown in the next one.
pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while unit + 1 < UNITS.len() && round1(value) >= 1024.0 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{:.1} {}", value, UNITS[unit])
}

/// Round to one decimal place, the precision [`format_bytes`] prints.
fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// Used fraction of a volume in `0.0..=1.0`; an empty (zero-sized) volume counts as unused.
pub fn used_fraction(total_bytes: u64, free_bytes: u64) -> f64 {
    if total_bytes == 0 {
        return 0.0;
    }
    let used = total_bytes.saturating_sub(free_bytes);
    (used as f64 / total_bytes as f64).clamp(0.0, 1.0)
}

/// A count with thousands separators: `1234567` → `"1,234,567"`.
pub fn format_count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Elapsed time as a clock: `"0:07"`, `"12:34"`, `"1:02:03"`.
pub fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// A short duration for reports: `"850 ms"`, `"4.2 s"`, then [`format_elapsed`].
pub fn format_duration(elapsed: Duration) -> String {
    let ms = elapsed.as_millis();
    if ms < 1000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        format!("{:.1} s", elapsed.as_secs_f64())
    } else {
        format_elapsed(elapsed)
    }
}

/// `part` as a percentage of `whole` with one decimal (`"12.5%"`); `"0.0%"` of nothing.
pub fn format_percent(part: u64, whole: u64) -> String {
    if whole == 0 {
        return "0.0%".to_string();
    }
    format!("{:.1}%", part as f64 * 100.0 / whole as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    #[test]
    fn bytes_below_one_kilobyte_are_whole_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1), "1 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn each_unit_is_base_1024_with_one_decimal() {
        assert_eq!(format_bytes(KB), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(MB), "1.0 MB");
        assert_eq!(format_bytes(MB + MB / 2), "1.5 MB");
        assert_eq!(format_bytes(512 * GB), "512.0 GB");
        assert_eq!(format_bytes(9 * GB), "9.0 GB");
        assert_eq!(format_bytes(TB), "1.0 TB");
        assert_eq!(format_bytes(8 * TB), "8.0 TB");
        assert_eq!(format_bytes(TB + TB / 5), "1.2 TB");
    }

    #[test]
    fn values_that_round_to_1024_move_to_the_next_unit() {
        // 1023.96 KB would print as "1024.0 KB".
        assert_eq!(format_bytes(MB - 40), "1.0 MB");
        assert_eq!(format_bytes(GB - 1), "1.0 GB");
        // Just below the rounding edge stays in the smaller unit.
        assert_eq!(format_bytes(1023 * KB), "1023.0 KB");
    }

    #[test]
    fn terabytes_are_the_largest_unit() {
        assert_eq!(format_bytes(2048 * TB), "2048.0 TB");
        assert_eq!(format_bytes(u64::MAX), "16777216.0 TB");
    }

    #[test]
    fn used_fraction_handles_edges() {
        assert_eq!(used_fraction(0, 0), 0.0);
        assert_eq!(used_fraction(100, 100), 0.0);
        assert_eq!(used_fraction(100, 0), 1.0);
        assert_eq!(used_fraction(100, 25), 0.75);
        // Free larger than total (racy OS numbers) clamps to unused, never negative.
        assert_eq!(used_fraction(100, 200), 0.0);
    }

    #[test]
    fn counts_get_thousands_separators() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1000), "1,000");
        assert_eq!(format_count(1_234_567), "1,234,567");
        assert_eq!(format_count(100_000), "100,000");
    }

    #[test]
    fn durations_read_as_clocks_and_reports() {
        assert_eq!(format_elapsed(Duration::from_secs(7)), "0:07");
        assert_eq!(format_elapsed(Duration::from_secs(754)), "12:34");
        assert_eq!(format_elapsed(Duration::from_secs(3723)), "1:02:03");
        assert_eq!(format_duration(Duration::from_millis(850)), "850 ms");
        assert_eq!(format_duration(Duration::from_millis(4210)), "4.2 s");
        assert_eq!(format_duration(Duration::from_secs(61)), "1:01");
    }

    #[test]
    fn percentages_have_one_decimal() {
        assert_eq!(format_percent(1, 8), "12.5%");
        assert_eq!(format_percent(5, 0), "0.0%");
        assert_eq!(format_percent(3, 3), "100.0%");
    }
}
