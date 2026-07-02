//! Resource inference: determine which kitchen resource a step needs.
//!
//! Given a step's classified [`TaskType`] and its body/timer text, infer the
//! [`ResourceRequirement`] it places on the kitchen — which appliance it
//! occupies (oven vs. stove), the oven temperature if one is stated, and
//! whether it demands the cook's hands-on attention.

use std::sync::LazyLock;

use regex::Regex;

use crate::model::TaskType;
use crate::resource::{OvenTemp, ResourceRequirement};

/// Keywords that indicate a step uses the oven.
const OVEN_KEYWORDS: &[&str] = &[
    "oven",
    "bake",
    "baking",
    "roast",
    "roasting",
    "broil",
    "broiling",
    "preheat",
    "preheating",
    "gas mark",
];

/// Keywords that indicate a step uses a stovetop burner.
const STOVE_KEYWORDS: &[&str] = &[
    "simmer",
    "simmering",
    "boil",
    "boiling",
    "sear",
    "searing",
    "sauté",
    "saute",
    "sautéing",
    "sauteing",
    "fry",
    "frying",
    "stir-fry",
    "deep-fry",
    "steam",
    "steaming",
    "poach",
    "poaching",
    "griddle",
    "deglaze",
    "reduce",
    "reducing",
    "stovetop",
    "stove",
    "saucepan",
    "skillet",
    "wok",
];

/// Matches an oven temperature such as "425°F", "425 F", "180C", "gas mark 6".
static TEMP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:gas\s*mark\s*(\d+))|(\d{2,3})\s*(?:°|degrees?\s*)?\s*(f|c|fahrenheit|celsius)\b",
    )
    .expect("temperature regex")
});

/// Infer the resource requirement for a step from its task type and text.
///
/// - Broiling is treated as an oven task (US ovens broil from the top element).
/// - Oven keywords win over stove keywords when both appear, since the oven is
///   the exclusive, contention-prone resource worth tracking precisely.
/// - Active tasks always demand the cook's attention.
pub fn infer_resource(body: &str, task_type: TaskType) -> ResourceRequirement {
    let lower = body.to_lowercase();
    let needs_cook = task_type.is_active();

    let mentions_oven = contains_any(&lower, OVEN_KEYWORDS);
    let mentions_stove = contains_any(&lower, STOVE_KEYWORDS);

    if mentions_oven {
        let temp = parse_oven_temp(body);
        return ResourceRequirement {
            appliance: Some(crate::resource::ResourceKind::Oven),
            oven_temp: temp,
            needs_cook,
        };
    }

    if mentions_stove {
        return ResourceRequirement::stove(needs_cook);
    }

    // No appliance detected. Fall back on task type: cooking steps without an
    // explicit appliance keyword default to the stove (the common case), prep
    // and passive/rest steps occupy no appliance.
    match task_type {
        TaskType::ActiveCook | TaskType::PassiveCook => ResourceRequirement::stove(needs_cook),
        _ => ResourceRequirement {
            appliance: None,
            oven_temp: None,
            needs_cook,
        },
    }
}

/// Parse an oven temperature from step text, normalizing to Fahrenheit.
///
/// Handles "425°F", "425 F", "180C"/"180 Celsius" (converted to °F), and
/// British "gas mark N" (mapped to its conventional Fahrenheit equivalent).
pub fn parse_oven_temp(text: &str) -> Option<OvenTemp> {
    let caps = TEMP_RE.captures(text)?;

    // Gas mark branch
    if let Some(mark) = caps.get(1) {
        let n: i32 = mark.as_str().parse().ok()?;
        let fahrenheit = gas_mark_to_fahrenheit(n)?;
        return Some(OvenTemp::from_fahrenheit(
            fahrenheit,
            caps.get(0)?.as_str().trim().to_string(),
        ));
    }

    // Numeric + unit branch
    let value: i32 = caps.get(2)?.as_str().parse().ok()?;
    let unit = caps.get(3)?.as_str().to_lowercase();
    let original = caps.get(0)?.as_str().trim().to_string();

    let fahrenheit = match unit.chars().next() {
        Some('c') => celsius_to_fahrenheit(value),
        _ => value, // fahrenheit
    };

    Some(OvenTemp::from_fahrenheit(fahrenheit, original))
}

fn celsius_to_fahrenheit(c: i32) -> i32 {
    (c as f64 * 9.0 / 5.0 + 32.0).round() as i32
}

/// Conventional UK gas mark → Fahrenheit mapping.
fn gas_mark_to_fahrenheit(mark: i32) -> Option<i32> {
    let f = match mark {
        1 => 275,
        2 => 300,
        3 => 325,
        4 => 350,
        5 => 375,
        6 => 400,
        7 => 425,
        8 => 450,
        9 => 475,
        _ => return None,
    };
    Some(f)
}

fn contains_any(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|kw| text.contains(kw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fahrenheit() {
        let t = parse_oven_temp("Bake at 425°F until golden").unwrap();
        assert_eq!(t.fahrenheit, 425);
    }

    #[test]
    fn parses_fahrenheit_plain() {
        let t = parse_oven_temp("Preheat oven to 350 F").unwrap();
        assert_eq!(t.fahrenheit, 350);
    }

    #[test]
    fn parses_celsius() {
        let t = parse_oven_temp("Roast at 180C for 40 minutes").unwrap();
        assert_eq!(t.fahrenheit, 356); // 180C = 356F
    }

    #[test]
    fn parses_gas_mark() {
        let t = parse_oven_temp("Bake at gas mark 6").unwrap();
        assert_eq!(t.fahrenheit, 400);
    }

    #[test]
    fn no_temp_returns_none() {
        assert!(parse_oven_temp("Simmer until reduced").is_none());
    }

    #[test]
    fn oven_step_detected() {
        let r = infer_resource("Bake at 425°F for 30 minutes", TaskType::PassiveCook);
        assert!(r.is_oven());
        assert_eq!(r.oven_temp.unwrap().fahrenheit, 425);
        assert!(!r.needs_cook);
    }

    #[test]
    fn broil_is_oven() {
        let r = infer_resource("Broil for 3 minutes to brown the top", TaskType::ActiveCook);
        assert!(r.is_oven());
        assert!(r.needs_cook); // active
    }

    #[test]
    fn stove_step_detected() {
        let r = infer_resource("Simmer the sauce for 20 minutes", TaskType::PassiveCook);
        assert!(r.is_stove());
        assert!(!r.needs_cook);
    }

    #[test]
    fn active_stove_needs_cook() {
        let r = infer_resource("Sear the chicken on high heat", TaskType::ActiveCook);
        assert!(r.is_stove());
        assert!(r.needs_cook);
    }

    #[test]
    fn prep_step_needs_no_appliance() {
        let r = infer_resource("Chop the onions finely", TaskType::ActivePrep);
        assert!(r.appliance.is_none());
        assert!(r.needs_cook); // hands-on prep
    }

    #[test]
    fn marinate_needs_nothing() {
        let r = infer_resource("Cover and refrigerate to marinate", TaskType::PassivePrep);
        assert!(r.appliance.is_none());
        assert!(!r.needs_cook);
    }

    #[test]
    fn oven_wins_over_stove_keywords() {
        // "reduce" (stove) appears but the step is fundamentally an oven bake.
        let r = infer_resource("Bake at 400F, then reduce heat", TaskType::PassiveCook);
        assert!(r.is_oven());
    }
}
