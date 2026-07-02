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
    /// Eggs — scale linearly, but classified for baker's-percentage reasoning.
    Egg,
    /// Liquids (water, milk, stock) — scale linearly; relevant to hydration.
    Liquid,
    /// Flour and other bulk dry structure — scale linearly; baker's-percentage base.
    Flour,
    /// Fats (butter, oil) — bulk fat scales linearly, but pan-coating fat is invariant.
    Fat,
}

/// The exponent used for sub-linear leavening scaling (`base × factor^EXPONENT`).
///
/// Chosen so that doubling yields ≈×1.68 and tripling ≈×2.28 — large batches
/// need proportionally less leavening to avoid over-rising and collapse.
pub const LEAVENING_EXPONENT: f64 = 0.75;

/// Lower fraction of the linear value used for the "to-taste" seasoning band.
///
/// Seasoning (salt, strong spice) is rendered as a range `[linear × BAND_LOW .. linear]`
/// with a recommendation to start low and adjust to taste.
pub const SEASONING_BAND_LOW: f64 = 0.85;

impl ScalingCategory {
    /// Warning message for this category, if any.
    ///
    /// Used by the default (non-rules) linear scaling path.
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
            Self::Linear | Self::Egg | Self::Liquid | Self::Flour | Self::Fat => None,
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

    /// Whether this category is adjusted (non-linearly) by the rules engine.
    ///
    /// Categories that scale linearly return `false` even though they carry a
    /// classification for explanations and future baker's-percentage work.
    pub fn is_rule_adjusted(&self) -> bool {
        matches!(self, Self::Leavening | Self::Salt | Self::Spice | Self::Fat)
    }
}

/// A deterministic, explainable non-linear adjustment for one ingredient line.
///
/// Produced only by the rules engine. `primary` is the recommended (adjusted)
/// quantity shown to the cook; `linear` preserves the pure-linear value so the
/// adjustment is always reversible; `explanation` states the rule and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Adjustment {
    /// The recommended quantity after applying the rule (may be a range/band).
    pub primary: String,
    /// The pure-linear value this line would have had (reversible reference).
    pub linear: String,
    /// Human-readable explanation of the rule applied.
    pub explanation: String,
}

/// Compute a non-linear adjustment for a parsed quantity, if the category and
/// scale factor call for one.
///
/// Returns `None` when no adjustment applies (linear categories, 1× scaling, or
/// rules that only trigger in one direction). The caller supplies a `format`
/// closure (typically [`crate::quantity::format_quantity`]) so this module stays
/// free of formatting policy.
pub fn adjust_quantity(
    category: ScalingCategory,
    base_value: f64,
    multiplier: f64,
    format: impl Fn(f64) -> String,
) -> Option<Adjustment> {
    // No adjustment at 1× — nothing changes.
    if (multiplier - 1.0).abs() < f64::EPSILON {
        return None;
    }

    let linear_value = base_value * multiplier;
    let linear = format(linear_value);

    match category {
        ScalingCategory::Leavening => {
            // Sub-linear only when scaling up; scaling down stays linear.
            if multiplier <= 1.0 {
                return None;
            }
            let adjusted = base_value * multiplier.powf(LEAVENING_EXPONENT);
            let primary = format(adjusted);
            let explanation = format!(
                "leavening scaled sub-linearly (×{eff} instead of ×{lin}) — \
                 large batches need proportionally less leavening (rise ∝ factor^{exp}) \
                 to avoid over-rising then collapsing; linear would be {linear}",
                eff = trim_factor(multiplier.powf(LEAVENING_EXPONENT)),
                lin = trim_factor(multiplier),
                exp = LEAVENING_EXPONENT,
                linear = linear,
            );
            Some(Adjustment {
                primary,
                linear,
                explanation,
            })
        }
        ScalingCategory::Salt | ScalingCategory::Spice => {
            // To-taste band around the linear value.
            let low = format(linear_value * SEASONING_BAND_LOW);
            let band = format!("{low} to {linear}");
            let kind = if category == ScalingCategory::Salt {
                "salt/seasoning"
            } else {
                "spice"
            };
            let explanation = format!(
                "{kind} shown as a to-taste band ({band}) — flavor perception isn't \
                 linear, so start at the low end ({low}) and adjust up; linear would be {linear}"
            );
            Some(Adjustment {
                primary: band,
                linear,
                explanation,
            })
        }
        ScalingCategory::Fat => {
            // Bulk fat scales linearly; annotate pan-coating invariance.
            let explanation = format!(
                "fat scaled linearly to {linear} — but if any of this is for greasing/\
                 coating the pan, that portion doesn't need to scale with batch size"
            );
            Some(Adjustment {
                primary: linear.clone(),
                linear,
                explanation,
            })
        }
        // Linear-scaling categories: no non-linear adjustment.
        ScalingCategory::Linear
        | ScalingCategory::Thickener
        | ScalingCategory::Egg
        | ScalingCategory::Liquid
        | ScalingCategory::Flour => None,
    }
}

/// Format a multiplier for display in explanations (e.g. `1.68`, `2`).
fn trim_factor(f: f64) -> String {
    if (f - f.round()).abs() < 0.005 {
        format!("{}", f.round() as i64)
    } else {
        format!("{f:.2}")
    }
}

/// Suggested (non-authoritative) cook-time multiplier for a given scale factor.
///
/// Cook/bake time scales roughly with volume^(2/3) (surface-area driven heat
/// transfer), not linearly. This is a *suggestion* only — fond never rewrites
/// the recipe's stated time.
pub fn cook_time_multiplier(multiplier: f64) -> f64 {
    multiplier.powf(2.0 / 3.0)
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
    OntologyEntry {
        phrases: &[
            "egg",
            "eggs",
            "egg white",
            "egg whites",
            "egg yolk",
            "egg yolks",
            "large egg",
            "large eggs",
        ],
        category: ScalingCategory::Egg,
    },
    OntologyEntry {
        phrases: &[
            "water",
            "milk",
            "whole milk",
            "buttermilk",
            "cream",
            "heavy cream",
            "stock",
            "chicken stock",
            "beef stock",
            "vegetable stock",
            "broth",
            "chicken broth",
            "wine",
            "white wine",
            "red wine",
        ],
        category: ScalingCategory::Liquid,
    },
    OntologyEntry {
        phrases: &[
            "flour",
            "all-purpose flour",
            "all purpose flour",
            "bread flour",
            "cake flour",
            "whole wheat flour",
            "self-rising flour",
            "self rising flour",
        ],
        category: ScalingCategory::Flour,
    },
    OntologyEntry {
        phrases: &[
            "butter",
            "unsalted butter",
            "salted butter",
            "oil",
            "olive oil",
            "vegetable oil",
            "canola oil",
            "coconut oil",
            "shortening",
            "lard",
            "ghee",
        ],
        category: ScalingCategory::Fat,
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
        // "salted butter" is now classified as Fat (its dominant role),
        // and importantly NOT as Salt — the word-boundary check still holds.
        assert_eq!(classify_ingredient("salted butter"), ScalingCategory::Fat);
        assert_eq!(classify_ingredient("unsalted butter"), ScalingCategory::Fat);
        // A genuinely unrelated word containing "salt" stays Linear.
        assert_eq!(classify_ingredient("salsa verde"), ScalingCategory::Linear);
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
        assert_eq!(classify_ingredient("onion"), ScalingCategory::Linear);
        assert_eq!(classify_ingredient("sugar"), ScalingCategory::Linear);
        assert_eq!(classify_ingredient("carrot"), ScalingCategory::Linear);
    }

    #[test]
    fn classify_new_categories() {
        assert_eq!(classify_ingredient("large eggs"), ScalingCategory::Egg);
        assert_eq!(classify_ingredient("egg yolk"), ScalingCategory::Egg);
        assert_eq!(classify_ingredient("whole milk"), ScalingCategory::Liquid);
        assert_eq!(
            classify_ingredient("chicken stock"),
            ScalingCategory::Liquid
        );
        assert_eq!(
            classify_ingredient("all-purpose flour"),
            ScalingCategory::Flour
        );
        assert_eq!(classify_ingredient("bread flour"), ScalingCategory::Flour);
        assert_eq!(classify_ingredient("olive oil"), ScalingCategory::Fat);
        assert_eq!(classify_ingredient("unsalted butter"), ScalingCategory::Fat);
    }

    #[test]
    fn adjust_leavening_sublinear_up() {
        // 2× leavening → base × 2^0.75 ≈ 1.6818×
        let adj = adjust_quantity(
            ScalingCategory::Leavening,
            1.0,
            2.0,
            crate::quantity::format_quantity,
        )
        .expect("leavening should adjust when scaling up");
        // 2^0.75 ≈ 1.6818 → formats to a mixed fraction near 1 2/3
        assert_eq!(adj.linear, "2");
        assert!(
            adj.explanation.contains("sub-linear"),
            "explanation should mention sub-linear: {}",
            adj.explanation
        );
        assert!(adj.explanation.contains("linear would be 2"));
    }

    #[test]
    fn adjust_leavening_linear_when_scaling_down() {
        // Scaling down → leavening stays linear (no adjustment).
        let adj = adjust_quantity(
            ScalingCategory::Leavening,
            1.0,
            0.5,
            crate::quantity::format_quantity,
        );
        assert!(adj.is_none());
    }

    #[test]
    fn adjust_salt_band() {
        let adj = adjust_quantity(
            ScalingCategory::Salt,
            1.0,
            2.0,
            crate::quantity::format_quantity,
        )
        .expect("salt should produce a band");
        // Band low = 2 × 0.85 = 1.7 → "1 2/3" or similar; primary contains "to"
        assert!(
            adj.primary.contains("to"),
            "band should be a range: {}",
            adj.primary
        );
        assert_eq!(adj.linear, "2");
    }

    #[test]
    fn adjust_none_at_1x() {
        assert!(
            adjust_quantity(
                ScalingCategory::Leavening,
                1.0,
                1.0,
                crate::quantity::format_quantity
            )
            .is_none()
        );
    }

    #[test]
    fn adjust_linear_categories_none() {
        for cat in [
            ScalingCategory::Linear,
            ScalingCategory::Egg,
            ScalingCategory::Liquid,
            ScalingCategory::Flour,
            ScalingCategory::Thickener,
        ] {
            assert!(
                adjust_quantity(cat, 1.0, 2.0, crate::quantity::format_quantity).is_none(),
                "{cat:?} should not be rule-adjusted"
            );
        }
    }

    #[test]
    fn cook_time_multiplier_sublinear() {
        // 2× volume → ~1.587× time (2^(2/3))
        let m = cook_time_multiplier(2.0);
        assert!((m - 1.5874).abs() < 0.001, "got {m}");
        // 1× → 1×
        assert!((cook_time_multiplier(1.0) - 1.0).abs() < 1e-9);
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
