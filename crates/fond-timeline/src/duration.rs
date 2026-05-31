use std::sync::LazyLock;

use regex::Regex;

/// Matches a timer duration string: "<number_or_range> <unit>".
/// Examples: "30 minutes", "2.5-3 hours", "1 minute", "45 secs".
static DURATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^\s*(\d+(?:\.\d+)?(?:\s*-\s*\d+(?:\.\d+)?)?)\s*(minutes?|mins?|hours?|hrs?|seconds?|secs?|h|m|s)\s*$",
    )
    .expect("duration regex")
});

/// Matches heuristic timing cues in step body text.
/// Examples: "for 30 minutes", "about 2 hours", "at least 1 hour".
static HEURISTIC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:for|about|approximately|roughly|at\s+least)\s+(?:about\s+)?(\d+(?:\.\d+)?(?:\s*-\s*\d+(?:\.\d+)?)?)\s*(minutes?|mins?|hours?|hrs?|seconds?|secs?)\b",
    )
    .expect("heuristic regex")
});

/// Parse a timer duration string (e.g., "30 minutes", "2.5-3 hours") into seconds.
///
/// For ranges (e.g., "40-45 minutes"), takes the **maximum** value
/// for conservative scheduling.
pub fn parse_duration_str(s: &str) -> Option<u64> {
    let caps = DURATION_RE.captures(s)?;
    let number_part = caps.get(1)?.as_str();
    let unit = caps.get(2)?.as_str();

    let value = parse_number_or_range_max(number_part)?;
    let multiplier = unit_to_seconds(unit)?;

    Some((value * multiplier as f64).round() as u64)
}

/// Try to extract a duration from step body text using heuristic patterns.
///
/// Returns `(seconds, matched_text)` if a timing cue is found.
pub fn extract_duration_from_text(text: &str) -> Option<(u64, String)> {
    let caps = HEURISTIC_RE.captures(text)?;
    let full_match = caps.get(0)?.as_str().to_string();
    let number_part = caps.get(1)?.as_str();
    let unit = caps.get(2)?.as_str();

    let value = parse_number_or_range_max(number_part)?;
    let multiplier = unit_to_seconds(unit)?;
    let seconds = (value * multiplier as f64).round() as u64;

    Some((seconds, full_match))
}

/// Parse "30", "2.5", or "40-45" / "2.5-3". For ranges, returns the max.
fn parse_number_or_range_max(s: &str) -> Option<f64> {
    if let Some((_, max_str)) = s.split_once('-') {
        max_str.trim().parse::<f64>().ok()
    } else {
        s.trim().parse::<f64>().ok()
    }
}

/// Convert a time unit string to its multiplier in seconds.
fn unit_to_seconds(unit: &str) -> Option<u64> {
    match unit.to_lowercase().as_str() {
        "s" | "sec" | "secs" | "second" | "seconds" => Some(1),
        "m" | "min" | "mins" | "minute" | "minutes" => Some(60),
        "h" | "hr" | "hrs" | "hour" | "hours" => Some(3600),
        _ => None,
    }
}

/// Format a duration in seconds into a human-readable string.
pub fn format_duration(seconds: u64) -> String {
    if seconds == 0 {
        return "0s".to_string();
    }
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let hours = seconds / 3600;
    let mins = (seconds % 3600) / 60;
    match (hours, mins) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} hr"),
        (h, m) => format!("{h} hr {m} min"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_minutes() {
        assert_eq!(parse_duration_str("30 minutes"), Some(1800));
        assert_eq!(parse_duration_str("1 minute"), Some(60));
        assert_eq!(parse_duration_str("5 min"), Some(300));
        assert_eq!(parse_duration_str("45 mins"), Some(2700));
    }

    #[test]
    fn parse_simple_hours() {
        assert_eq!(parse_duration_str("1 hour"), Some(3600));
        assert_eq!(parse_duration_str("2 hours"), Some(7200));
        assert_eq!(parse_duration_str("1 hr"), Some(3600));
        assert_eq!(parse_duration_str("3 hrs"), Some(10800));
    }

    #[test]
    fn parse_simple_seconds() {
        assert_eq!(parse_duration_str("30 seconds"), Some(30));
        assert_eq!(parse_duration_str("90 secs"), Some(90));
        assert_eq!(parse_duration_str("1 second"), Some(1));
    }

    #[test]
    fn parse_decimal() {
        assert_eq!(parse_duration_str("2.5 hours"), Some(9000));
        assert_eq!(parse_duration_str("1.5 minutes"), Some(90));
    }

    #[test]
    fn parse_range_takes_max() {
        assert_eq!(parse_duration_str("40-45 minutes"), Some(2700));
        assert_eq!(parse_duration_str("12-16 hours"), Some(57600));
        assert_eq!(parse_duration_str("2.5-3 hours"), Some(10800));
        assert_eq!(parse_duration_str("20-25 minutes"), Some(1500));
        assert_eq!(parse_duration_str("3-4 minutes"), Some(240));
    }

    #[test]
    fn parse_invalid_returns_none() {
        assert_eq!(parse_duration_str(""), None);
        assert_eq!(parse_duration_str("a few minutes"), None);
        assert_eq!(parse_duration_str("until done"), None);
        assert_eq!(parse_duration_str("30"), None); // no unit
    }

    #[test]
    fn parse_case_insensitive() {
        assert_eq!(parse_duration_str("30 Minutes"), Some(1800));
        assert_eq!(parse_duration_str("1 HOUR"), Some(3600));
    }

    #[test]
    fn heuristic_extraction() {
        let (secs, _) = extract_duration_from_text("cook for 30 minutes until golden").unwrap();
        assert_eq!(secs, 1800);

        let (secs, _) =
            extract_duration_from_text("let stand for about 10 minutes before serving").unwrap();
        assert_eq!(secs, 600);

        let (secs, _) =
            extract_duration_from_text("refrigerate for at least 4 hours or overnight").unwrap();
        assert_eq!(secs, 14400);

        let (secs, _) = extract_duration_from_text("simmer for approximately 20 minutes").unwrap();
        assert_eq!(secs, 1200);
    }

    #[test]
    fn heuristic_no_match() {
        assert!(extract_duration_from_text("chop the onions finely").is_none());
        assert!(extract_duration_from_text("cook until tender").is_none());
        assert!(extract_duration_from_text("serve immediately").is_none());
    }

    #[test]
    fn format_display() {
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(60), "1 min");
        assert_eq!(format_duration(300), "5 min");
        assert_eq!(format_duration(1800), "30 min");
        assert_eq!(format_duration(3600), "1 hr");
        assert_eq!(format_duration(5400), "1 hr 30 min");
        assert_eq!(format_duration(7200), "2 hr");
        assert_eq!(format_duration(45000), "12 hr 30 min");
    }
}
