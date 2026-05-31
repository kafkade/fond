use crate::model::TaskType;

const REST_KEYWORDS: &[&str] = &["rest", "resting", "cool", "cooling", "cool down"];

const PASSIVE_PREP_KEYWORDS: &[&str] = &[
    "marinate",
    "marinading",
    "marinating",
    "soak",
    "soaking",
    "rise",
    "rising",
    "ferment",
    "fermenting",
    "proof",
    "proofing",
    "chill",
    "chilling",
    "refrigerate",
    "refrigerating",
    "cold proof",
    "set aside",
    "let sit",
    "autolyse",
];

const PASSIVE_COOK_KEYWORDS: &[&str] = &[
    "simmer",
    "simmering",
    "bake",
    "baking",
    "roast",
    "roasting",
    "braise",
    "braising",
    "boil",
    "boiling",
    "steam",
    "steaming",
    "slow cook",
    "preheat",
    "preheating",
    "oven",
];

const ACTIVE_COOK_KEYWORDS: &[&str] = &[
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
    "grill",
    "grilling",
    "broil",
    "broiling",
    "char",
    "toast",
    "toasting",
    "brown",
    "browning",
    "deglaze",
    "reduce",
    "reducing",
    "caramelize",
    "caramelizing",
    "flip",
    "griddle",
];

/// Classify a step's task type based on timer names and step body text.
///
/// Uses a "last keyword wins" heuristic for body text: the keyword
/// appearing latest in the text determines the classification, since
/// steps typically describe setup first and the main action last
/// (e.g., "reduce heat, cover, and simmer for 35 min" → simmer wins).
pub fn classify_task_type(body: &str, timer_names: &[Option<String>]) -> TaskType {
    // Timer names are the strongest signal
    for name in timer_names.iter().filter_map(|n| n.as_deref()) {
        let lower = name.to_lowercase();
        if contains_any(&lower, REST_KEYWORDS) {
            return TaskType::Rest;
        }
        if contains_any(&lower, PASSIVE_PREP_KEYWORDS) {
            return TaskType::PassivePrep;
        }
        if contains_any(&lower, PASSIVE_COOK_KEYWORDS) {
            return TaskType::PassiveCook;
        }
    }

    // Fall back to body text with "last keyword wins"
    classify_from_body(body)
}

/// Classify based on body text using the last-keyword-position heuristic.
fn classify_from_body(body: &str) -> TaskType {
    let lower = body.to_lowercase();

    let last_rest = find_last_keyword_pos(&lower, REST_KEYWORDS);
    let last_passive_prep = find_last_keyword_pos(&lower, PASSIVE_PREP_KEYWORDS);
    let last_passive_cook = find_last_keyword_pos(&lower, PASSIVE_COOK_KEYWORDS);
    let last_active_cook = find_last_keyword_pos(&lower, ACTIVE_COOK_KEYWORDS);

    let candidates = [
        (last_rest, TaskType::Rest),
        (last_passive_prep, TaskType::PassivePrep),
        (last_passive_cook, TaskType::PassiveCook),
        (last_active_cook, TaskType::ActiveCook),
    ];

    candidates
        .iter()
        .filter_map(|(pos, tt)| pos.map(|p| (p, *tt)))
        .max_by_key(|(pos, _)| *pos)
        .map(|(_, tt)| tt)
        .unwrap_or(TaskType::ActivePrep)
}

/// Find the rightmost occurrence of any keyword in the text.
fn find_last_keyword_pos(text: &str, keywords: &[&str]) -> Option<usize> {
    keywords.iter().filter_map(|kw| text.rfind(kw)).max()
}

/// Check if text contains any of the given keywords.
fn contains_any(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|kw| text.contains(kw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_name_marinate() {
        assert_eq!(
            classify_task_type("Add chicken", &[Some("marinate".into())]),
            TaskType::PassivePrep,
        );
    }

    #[test]
    fn timer_name_rest() {
        assert_eq!(
            classify_task_type("Remove from heat", &[Some("rest".into())]),
            TaskType::Rest,
        );
    }

    #[test]
    fn timer_name_braise() {
        assert_eq!(
            classify_task_type("Cover tightly", &[Some("braise".into())]),
            TaskType::PassiveCook,
        );
    }

    #[test]
    fn body_simmer_wins_over_reduce() {
        // "reduce heat... simmer" → simmer appears later → passive cook
        assert_eq!(
            classify_task_type(
                "Reduce heat to medium-low, cover, and simmer for 35 minutes",
                &[]
            ),
            TaskType::PassiveCook,
        );
    }

    #[test]
    fn body_sear() {
        assert_eq!(
            classify_task_type("Sear in oil over high heat until deeply browned", &[]),
            TaskType::ActiveCook,
        );
    }

    #[test]
    fn body_fry() {
        assert_eq!(
            classify_task_type("Add the curry paste and fry until very fragrant", &[]),
            TaskType::ActiveCook,
        );
    }

    #[test]
    fn body_chop() {
        assert_eq!(
            classify_task_type("Chop the onions finely", &[]),
            TaskType::ActivePrep,
        );
    }

    #[test]
    fn body_combine() {
        assert_eq!(
            classify_task_type(
                "Combine soy sauce, vinegar, garlic, and peppercorns in a bowl",
                &[]
            ),
            TaskType::ActivePrep,
        );
    }

    #[test]
    fn body_bake() {
        assert_eq!(
            classify_task_type("Bake for 40-45 minutes until set at edges", &[]),
            TaskType::PassiveCook,
        );
    }

    #[test]
    fn body_refrigerate() {
        assert_eq!(
            classify_task_type("Refrigerate for at least 4 hours or up to 2 days", &[]),
            TaskType::PassivePrep,
        );
    }

    #[test]
    fn body_default_active_prep() {
        assert_eq!(
            classify_task_type("Strain through a fine-mesh sieve", &[]),
            TaskType::ActivePrep,
        );
    }

    #[test]
    fn cold_proof_passive_prep() {
        assert_eq!(
            classify_task_type("Place in bannetons", &[Some("cold proof".into())]),
            TaskType::PassivePrep,
        );
    }

    #[test]
    fn timer_name_overrides_body() {
        // Timer name "soak chiles" → passive prep, even if body mentions "toast"
        assert_eq!(
            classify_task_type(
                "Toast chiles in a dry skillet",
                &[Some("soak chiles".into())]
            ),
            TaskType::PassivePrep,
        );
    }

    #[test]
    fn griddle_is_active_cook() {
        assert_eq!(
            classify_task_type("Griddle in a comal until crispy", &[]),
            TaskType::ActiveCook,
        );
    }
}
