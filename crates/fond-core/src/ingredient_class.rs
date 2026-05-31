//! Ingredient scaling categories and non-linear warnings.
//!
//! Classifies ingredients by name to detect those that don't scale
//! linearly (leavening, salt, strong spices, thickeners). Uses
//! word-boundary matching to avoid false positives.

/// How an ingredient scales when a recipe is multiplied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScalingCategory {
    /// Scales linearly — no warning needed.
    Linear,
    /// Leavening agents (baking soda, yeast) — don't double blindly.
    Leavening,
    /// Salt and high-sodium seasonings — scale cautiously.
    Salt,
    /// Strong spices — flavor compounds don't scale linearly.
    Spice,
    /// Thickeners — flour/cornstarch as thickener, not bulk ingredient.
    Thickener,
}

impl ScalingCategory {
    /// Warning message for this category, if any.
    pub fn warning(&self, scale_factor: f64) -> Option<String> {
        if (scale_factor - 1.0).abs() < f64::EPSILON {
            return None; // No warning at 1x
        }

        let direction = if scale_factor > 1.0 {
            "scaling up"
        } else {
            "scaling down"
        };

        match self {
            Self::Linear => None,
            Self::Leavening => Some(format!(
                "leavening — when {direction}, adjust cautiously; \
                 don't simply multiply"
            )),
            Self::Salt => Some(format!(
                "salt/seasoning — when {direction}, start with less \
                 and adjust to taste"
            )),
            Self::Spice => Some(format!(
                "spice — flavor compounds don't scale linearly; \
                 add incrementally when {direction}"
            )),
            Self::Thickener => Some(format!(
                "thickener — when {direction}, adjust by ratio \
                 but verify consistency"
            )),
        }
    }
}

/// An entry in the ingredient ontology for scaling classification.
struct OntologyEntry {
    /// Phrases to match (lowercase, matched as whole words/phrases).
    phrases: &'static [&'static str],
    category: ScalingCategory,
}

const ONTOLOGY: &[OntologyEntry] = &[
    OntologyEntry {
        phrases: &[
            "baking soda",
            "bicarbonate of soda",
            "baking powder",
            "yeast",
            "active dry yeast",
            "instant yeast",
            "cream of tartar",
        ],
        category: ScalingCategory::Leavening,
    },
    OntologyEntry {
        phrases: &[
            "salt",
            "kosher salt",
            "sea salt",
            "flaky salt",
            "table salt",
            "soy sauce",
            "fish sauce",
            "tamari",
            "miso",
            "miso paste",
            "oyster sauce",
            "worcestershire sauce",
            "anchovy paste",
        ],
        category: ScalingCategory::Salt,
    },
    OntologyEntry {
        phrases: &[
            "cayenne",
            "cayenne pepper",
            "chili flakes",
            "red pepper flakes",
            "crushed red pepper",
            "chipotle",
            "habanero",
            "ghost pepper",
            "szechuan peppercorn",
            "sichuan peppercorn",
            "cloves",
            "ground cloves",
            "star anise",
            "cinnamon",
            "ground cinnamon",
            "nutmeg",
            "ground nutmeg",
            "allspice",
            "ground allspice",
            "cardamom",
            "ground cardamom",
            "cumin",
            "ground cumin",
            "coriander",
            "ground coriander",
            "turmeric",
            "ground turmeric",
            "paprika",
            "smoked paprika",
            "garam masala",
            "curry powder",
            "five spice",
            "chinese five spice",
            "vanilla extract",
            "almond extract",
            "peppercorn",
            "black pepper",
            "white pepper",
        ],
        category: ScalingCategory::Spice,
    },
    OntologyEntry {
        phrases: &[
            "cornstarch",
            "corn starch",
            "arrowroot",
            "arrowroot powder",
            "tapioca starch",
            "potato starch",
            "xanthan gum",
            "gelatin",
        ],
        category: ScalingCategory::Thickener,
    },
];

/// Classify an ingredient name for scaling behavior.
///
/// Uses word-boundary phrase matching against an ontology of known
/// non-linear ingredients. Returns `Linear` for anything not matched.
pub fn classify_ingredient(name: &str) -> ScalingCategory {
    let normalized = name.trim().to_lowercase();

    // Try exact match first (most common case)
    for entry in ONTOLOGY {
        for &phrase in entry.phrases {
            if normalized == phrase {
                return entry.category;
            }
        }
    }

    // Try phrase-contained match with word boundaries
    for entry in ONTOLOGY {
        for &phrase in entry.phrases {
            if contains_phrase(&normalized, phrase) {
                return entry.category;
            }
        }
    }

    ScalingCategory::Linear
}

/// Check if `haystack` contains `phrase` at a word boundary.
///
/// Avoids false positives like "salted butter" matching "salt".
fn contains_phrase(haystack: &str, phrase: &str) -> bool {
    if let Some(pos) = haystack.find(phrase) {
        let before_ok = pos == 0 || !haystack.as_bytes()[pos - 1].is_ascii_alphanumeric();
        let end = pos + phrase.len();
        let after_ok = end >= haystack.len() || !haystack.as_bytes()[end].is_ascii_alphanumeric();
        before_ok && after_ok
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_leavening() {
        assert_eq!(
            classify_ingredient("baking soda"),
            ScalingCategory::Leavening
        );
        assert_eq!(
            classify_ingredient("Baking Powder"),
            ScalingCategory::Leavening
        );
        assert_eq!(
            classify_ingredient("active dry yeast"),
            ScalingCategory::Leavening
        );
    }

    #[test]
    fn classify_salt() {
        assert_eq!(classify_ingredient("salt"), ScalingCategory::Salt);
        assert_eq!(classify_ingredient("kosher salt"), ScalingCategory::Salt);
        assert_eq!(classify_ingredient("soy sauce"), ScalingCategory::Salt);
        assert_eq!(classify_ingredient("fish sauce"), ScalingCategory::Salt);
    }

    #[test]
    fn classify_salt_no_false_positives() {
        // "salted butter" should NOT match "salt" — word boundary check
        assert_eq!(
            classify_ingredient("salted butter"),
            ScalingCategory::Linear
        );
        assert_eq!(
            classify_ingredient("unsalted butter"),
            ScalingCategory::Linear
        );
    }

    #[test]
    fn classify_spice() {
        assert_eq!(classify_ingredient("cumin"), ScalingCategory::Spice);
        assert_eq!(
            classify_ingredient("smoked paprika"),
            ScalingCategory::Spice
        );
        assert_eq!(
            classify_ingredient("cayenne pepper"),
            ScalingCategory::Spice
        );
        assert_eq!(
            classify_ingredient("vanilla extract"),
            ScalingCategory::Spice
        );
    }

    #[test]
    fn classify_thickener() {
        assert_eq!(
            classify_ingredient("cornstarch"),
            ScalingCategory::Thickener
        );
        assert_eq!(
            classify_ingredient("arrowroot powder"),
            ScalingCategory::Thickener
        );
    }

    #[test]
    fn classify_linear_default() {
        assert_eq!(classify_ingredient("chicken"), ScalingCategory::Linear);
        assert_eq!(classify_ingredient("olive oil"), ScalingCategory::Linear);
        assert_eq!(classify_ingredient("flour"), ScalingCategory::Linear);
        assert_eq!(classify_ingredient("sugar"), ScalingCategory::Linear);
        assert_eq!(classify_ingredient("butter"), ScalingCategory::Linear);
    }

    #[test]
    fn classify_case_insensitive() {
        assert_eq!(
            classify_ingredient("BAKING SODA"),
            ScalingCategory::Leavening
        );
        assert_eq!(classify_ingredient("SOY SAUCE"), ScalingCategory::Salt);
    }

    #[test]
    fn warning_messages() {
        assert!(ScalingCategory::Linear.warning(2.0).is_none());
        assert!(ScalingCategory::Leavening.warning(2.0).is_some());
        assert!(ScalingCategory::Salt.warning(0.5).is_some());
        // No warning at 1x
        assert!(ScalingCategory::Leavening.warning(1.0).is_none());
    }
}
