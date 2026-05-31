use std::sync::LazyLock;

use regex::Regex;

/// A parsed numeric quantity from a recipe ingredient string.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedQuantity {
    pub value: f64,
}

// Common Unicode fraction characters mapped to their f64 values.
const UNICODE_FRACTIONS: &[(char, f64)] = &[
    ('½', 0.5),
    ('⅓', 1.0 / 3.0),
    ('⅔', 2.0 / 3.0),
    ('¼', 0.25),
    ('¾', 0.75),
    ('⅕', 0.2),
    ('⅖', 0.4),
    ('⅗', 0.6),
    ('⅘', 0.8),
    ('⅙', 1.0 / 6.0),
    ('⅚', 5.0 / 6.0),
    ('⅛', 0.125),
    ('⅜', 0.375),
    ('⅝', 0.625),
    ('⅞', 0.875),
];

static FRACTION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+)\s*/\s*(\d+)$").unwrap());

static MIXED_FRACTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d+)\s+(\d+)\s*/\s*(\d+)$").unwrap());

static APPROX_PREFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:approximately|approx|about|~)\s*").unwrap());

static RANGE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.+?)\s*(?:-|–|to)\s*(.+)$").unwrap());

/// Attempt to parse a quantity string into a numeric value.
///
/// Handles: integers, decimals, fractions (1/2), mixed fractions (1 1/2),
/// Unicode fractions (½, ¼), ranges (returns the lower bound), and
/// approximate prefixes ("about 1/2").
///
/// Returns `None` for vague quantities ("a pinch", "some", "to taste").
pub fn parse_quantity(s: &str) -> Option<ParsedQuantity> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Strip approximate prefixes
    let cleaned = APPROX_PREFIX_RE.replace(trimmed, "");
    let cleaned = cleaned.trim();

    // Handle ranges — scale both endpoints would require more complex
    // output, so we take the lower bound (conservative for scaling up).
    if let Some(caps) = RANGE_RE.captures(cleaned) {
        let low = caps.get(1)?.as_str();
        return parse_single_quantity(low);
    }

    parse_single_quantity(cleaned)
}

/// Parse a single (non-range) quantity value.
fn parse_single_quantity(s: &str) -> Option<ParsedQuantity> {
    let s = s.trim();

    // Try: whole number with Unicode fraction (e.g., "1½", "2⅓")
    if s.len() > 1 {
        let last_char = s.chars().last()?;
        if let Some(&(_, frac_val)) = UNICODE_FRACTIONS.iter().find(|(c, _)| *c == last_char) {
            let prefix = &s[..s.len() - last_char.len_utf8()].trim_end();
            if prefix.is_empty() {
                return Some(ParsedQuantity { value: frac_val });
            }
            if let Ok(whole) = prefix.parse::<f64>() {
                return Some(ParsedQuantity {
                    value: whole + frac_val,
                });
            }
        }
    }

    // Try: standalone Unicode fraction
    if s.chars().count() == 1 {
        let ch = s.chars().next()?;
        if let Some(&(_, val)) = UNICODE_FRACTIONS.iter().find(|(c, _)| *c == ch) {
            return Some(ParsedQuantity { value: val });
        }
    }

    // Try: mixed fraction (1 1/2)
    if let Some(caps) = MIXED_FRACTION_RE.captures(s) {
        let whole: f64 = caps[1].parse().ok()?;
        let num: f64 = caps[2].parse().ok()?;
        let den: f64 = caps[3].parse().ok()?;
        if den == 0.0 {
            return None;
        }
        return Some(ParsedQuantity {
            value: whole + num / den,
        });
    }

    // Try: simple fraction (1/2)
    if let Some(caps) = FRACTION_RE.captures(s) {
        let num: f64 = caps[1].parse().ok()?;
        let den: f64 = caps[2].parse().ok()?;
        if den == 0.0 {
            return None;
        }
        return Some(ParsedQuantity { value: num / den });
    }

    // Try: decimal with comma (1,5 → 1.5) — common in European imports
    if s.contains(',') && !s.contains('.') {
        let normalized = s.replace(',', ".");
        if let Ok(v) = normalized.parse::<f64>()
            && v.is_finite()
            && v >= 0.0
        {
            return Some(ParsedQuantity { value: v });
        }
    }

    // Try: plain number (integer or decimal)
    if let Ok(v) = s.parse::<f64>()
        && v.is_finite()
        && v >= 0.0
    {
        return Some(ParsedQuantity { value: v });
    }

    None
}

/// Denominator set for human-friendly fraction formatting.
/// Includes thirds and sixths for cooking accuracy.
const FRACTION_DENOMS: &[u32] = &[2, 3, 4, 6, 8, 12, 16];

/// Format a f64 quantity back into a human-readable string.
///
/// Uses common cooking fractions (1/2, 1/3, 1/4, etc.) when the value
/// is close enough. Falls back to one decimal place for values that
/// don't map to a clean fraction.
pub fn format_quantity(value: f64) -> String {
    if value <= 0.0 {
        return "0".to_string();
    }

    let whole = value.floor() as u64;
    let frac = value - whole as f64;

    // Pure whole number
    if frac < 0.01 {
        return whole.to_string();
    }

    // Try to find the best fraction match
    if let Some((num, den)) = best_fraction(frac) {
        if whole == 0 {
            format!("{num}/{den}")
        } else {
            format!("{whole} {num}/{den}")
        }
    } else if whole == 0 {
        // Fallback to decimal
        format_decimal(value)
    } else {
        format_decimal(value)
    }
}

/// Find the best fraction representation for a value in (0, 1).
fn best_fraction(frac: f64) -> Option<(u32, u32)> {
    let tolerance = 0.02;
    let mut best: Option<(u32, u32, f64)> = None;

    for &den in FRACTION_DENOMS {
        let num = (frac * den as f64).round() as u32;
        if num == 0 || num >= den {
            continue;
        }
        let approx = num as f64 / den as f64;
        let error = (approx - frac).abs();
        if error < tolerance {
            // Prefer simpler (smaller denominator) fractions
            let is_better = match best {
                None => true,
                Some((_, _, prev_err)) => error < prev_err - 0.001,
            };
            if is_better {
                // Reduce the fraction
                let g = gcd(num, den);
                best = Some((num / g, den / g, error));
            }
        }
    }

    best.map(|(n, d, _)| (n, d))
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

fn format_decimal(value: f64) -> String {
    if (value - value.round()).abs() < 0.01 {
        format!("{}", value.round() as u64)
    } else {
        let s = format!("{value:.1}");
        // Remove trailing zero after decimal (e.g., "2.0" → "2")
        if s.ends_with(".0") {
            s[..s.len() - 2].to_string()
        } else {
            s
        }
    }
}

/// Parse a servings string into a numeric value.
///
/// Handles: "4", "4 servings", "Serves 4", "4-6" (takes lower bound),
/// "about 8", "Yield: 12 cookies".
pub fn parse_servings(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Extract the first number-like token from the string
    static FIRST_NUMBER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(\d+(?:\.\d+)?)").unwrap());

    FIRST_NUMBER_RE
        .captures(trimmed)
        .and_then(|caps| caps[1].parse::<f64>().ok())
        .filter(|v| *v > 0.0 && v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_integer() {
        assert_eq!(parse_quantity("2").unwrap().value, 2.0);
        assert_eq!(parse_quantity("12").unwrap().value, 12.0);
    }

    #[test]
    fn parse_decimal() {
        assert_eq!(parse_quantity("2.5").unwrap().value, 2.5);
        assert_eq!(parse_quantity("0.25").unwrap().value, 0.25);
    }

    #[test]
    fn parse_simple_fraction() {
        assert_eq!(parse_quantity("1/2").unwrap().value, 0.5);
        assert_eq!(parse_quantity("3/4").unwrap().value, 0.75);
        assert_eq!(parse_quantity("1/3").unwrap().value, 1.0 / 3.0);
    }

    #[test]
    fn parse_mixed_fraction() {
        assert_eq!(parse_quantity("1 1/2").unwrap().value, 1.5);
        assert_eq!(parse_quantity("2 3/4").unwrap().value, 2.75);
    }

    #[test]
    fn parse_unicode_fraction() {
        assert_eq!(parse_quantity("½").unwrap().value, 0.5);
        assert_eq!(parse_quantity("¼").unwrap().value, 0.25);
        assert_eq!(parse_quantity("¾").unwrap().value, 0.75);
        assert_eq!(parse_quantity("⅓").unwrap().value, 1.0 / 3.0);
        assert_eq!(parse_quantity("⅛").unwrap().value, 0.125);
    }

    #[test]
    fn parse_mixed_unicode_fraction() {
        assert_eq!(parse_quantity("1½").unwrap().value, 1.5);
        assert_eq!(parse_quantity("2⅓").unwrap().value, 2.0 + 1.0 / 3.0);
    }

    #[test]
    fn parse_approximate() {
        // Test each case separately for clearer failure messages
        let r1 = parse_quantity("about 1/2");
        assert!(r1.is_some(), "about 1/2 should parse, got None");
        assert_eq!(r1.unwrap().value, 0.5);

        let r2 = parse_quantity("~2");
        assert!(r2.is_some(), "~2 should parse, got None");
        assert_eq!(r2.unwrap().value, 2.0);

        let r3 = parse_quantity("approximately 3");
        assert!(r3.is_some(), "approximately 3 should parse, got None");
        assert_eq!(r3.unwrap().value, 3.0);
    }

    #[test]
    fn parse_range_takes_lower() {
        assert_eq!(parse_quantity("1-2").unwrap().value, 1.0);
        assert_eq!(parse_quantity("4 to 6").unwrap().value, 4.0);
    }

    #[test]
    fn parse_comma_decimal() {
        assert_eq!(parse_quantity("1,5").unwrap().value, 1.5);
    }

    #[test]
    fn parse_whitespace() {
        assert_eq!(parse_quantity("  2  ").unwrap().value, 2.0);
        assert_eq!(parse_quantity("  1 / 2  ").unwrap().value, 0.5);
    }

    #[test]
    fn parse_vague_returns_none() {
        assert!(parse_quantity("a pinch").is_none());
        assert!(parse_quantity("some").is_none());
        assert!(parse_quantity("to taste").is_none());
        assert!(parse_quantity("").is_none());
    }

    #[test]
    fn parse_zero_denominator_returns_none() {
        assert!(parse_quantity("1/0").is_none());
    }

    #[test]
    fn format_whole_numbers() {
        assert_eq!(format_quantity(1.0), "1");
        assert_eq!(format_quantity(4.0), "4");
        assert_eq!(format_quantity(12.0), "12");
    }

    #[test]
    fn format_common_fractions() {
        assert_eq!(format_quantity(0.5), "1/2");
        assert_eq!(format_quantity(0.25), "1/4");
        assert_eq!(format_quantity(0.75), "3/4");
    }

    #[test]
    fn format_thirds() {
        assert_eq!(format_quantity(1.0 / 3.0), "1/3");
        assert_eq!(format_quantity(2.0 / 3.0), "2/3");
    }

    #[test]
    fn format_mixed_fractions() {
        assert_eq!(format_quantity(1.5), "1 1/2");
        assert_eq!(format_quantity(2.25), "2 1/4");
        assert_eq!(format_quantity(3.75), "3 3/4");
    }

    #[test]
    fn format_known_fractions() {
        // All common cooking fractions map correctly
        assert_eq!(format_quantity(1.0 / 6.0), "1/6");
        assert_eq!(format_quantity(5.0 / 6.0), "5/6");
        assert_eq!(format_quantity(1.0 / 8.0), "1/8");
        assert_eq!(format_quantity(3.0 / 8.0), "3/8");
    }

    #[test]
    fn format_zero() {
        assert_eq!(format_quantity(0.0), "0");
    }

    #[test]
    fn parse_servings_simple() {
        assert_eq!(parse_servings("4"), Some(4.0));
        assert_eq!(parse_servings("8"), Some(8.0));
    }

    #[test]
    fn parse_servings_with_text() {
        assert_eq!(parse_servings("4 servings"), Some(4.0));
        assert_eq!(parse_servings("Serves 4"), Some(4.0));
        assert_eq!(parse_servings("Yield: 12 cookies"), Some(12.0));
    }

    #[test]
    fn parse_servings_range() {
        assert_eq!(parse_servings("4-6"), Some(4.0));
    }

    #[test]
    fn parse_servings_empty() {
        assert_eq!(parse_servings(""), None);
    }
}
