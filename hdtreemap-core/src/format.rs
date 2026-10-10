//! Human-readable formatting of sizes and fractions for the UI (start-screen volume cards
//! now; the status bar, folder tree and legend later). Pure functions, unit-tested here.

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
}
