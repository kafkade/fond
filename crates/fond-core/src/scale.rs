use fond_domain::Recipe;
use serde::Serialize;

use crate::ingredient_class::{
    ScalingCategory, adjust_quantity, classify_ingredient, cook_time_multiplier,
};
use crate::quantity::{format_quantity, parse_quantity, parse_servings};

/// How to scale a recipe.
#[derive(Debug, Clone)]
pub enum ScaleFactor {
    /// Multiply all quantities by this factor (e.g., 2.0 for doubling).
    Multiplier(f64),
    /// Scale to a target number of servings.
    ToServings(u32),
}

/// Options controlling scaling behavior.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScaleOptions {
    /// When `true`, apply the deterministic non-linear adjustment rules
    /// (sub-linear leavening, to-taste seasoning bands, pan-coating fat notes,
    /// and cook-time suggestions). When `false` (the default), scaling is purely
    /// linear with non-linear *warnings* only.
    pub rules: bool,
}

/// Error type for scaling operations.
#[derive(Debug, thiserror::Error)]
pub enum ScaleError {
    #[error("scale factor must be positive (got {0})")]
    InvalidFactor(f64),

    #[error("target servings must be positive (got {0})")]
    InvalidServings(u32),

    #[error(
        "recipe has no servings metadata — add `>> servings: N` to the .cook file to use --servings"
    )]
    NoServingsMetadata,

    #[error("could not parse servings value '{0}' as a number")]
    UnparseableServings(String),
}

/// A scaled ingredient with original and new quantities.
#[derive(Debug, Clone, Serialize)]
pub struct ScaledIngredient {
    pub name: String,
    pub original_quantity: Option<String>,
    pub scaled_quantity: Option<String>,
    pub unit: Option<String>,
    pub note: Option<String>,
    pub optional: bool,
    pub category: ScalingCategory,
    pub warning: Option<String>,
    /// The pure-linear value for this line, preserved when a non-linear rule
    /// adjusted `scaled_quantity`. `None` unless rules mode changed the value —
    /// makes every adjustment reversible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linear_quantity: Option<String>,
    /// Explanation of the non-linear rule applied to this line, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}

/// A scaling warning about a non-linear or unscalable ingredient.
#[derive(Debug, Clone, Serialize)]
pub struct ScalingWarning {
    pub ingredient: String,
    pub message: String,
}

/// The result of scaling a recipe — an owned, serializable DTO.
#[derive(Debug, Clone, Serialize)]
pub struct ScaledRecipe {
    pub slug: String,
    pub title: String,
    pub scale_factor: f64,
    pub original_servings: Option<String>,
    pub scaled_servings: Option<String>,
    pub prep_time: Option<String>,
    pub cook_time: Option<String>,
    pub total_time: Option<String>,
    pub ingredients: Vec<ScaledIngredient>,
    pub warnings: Vec<ScalingWarning>,
    pub tags: Vec<String>,
    /// Whether the non-linear rules engine was applied (`--rules`).
    pub rules_applied: bool,
    /// A suggested cook-time adjustment (rules mode only). Advisory — the recipe's
    /// stated times are never rewritten. `None` when no cook time is known or
    /// scaling is 1×.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_suggestion: Option<String>,
    /// A pan/equipment capacity note (rules mode only), if the scale factor is
    /// large enough to risk exceeding the original vessel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pan_note: Option<String>,
}

/// Scale a recipe by a given factor using the default (linear) behavior.
///
/// Equivalent to [`scale_recipe_with`] with default options. Kept for callers
/// that only need pure-linear scaling with non-linear *warnings*.
pub fn scale_recipe(recipe: &Recipe, factor: ScaleFactor) -> Result<ScaledRecipe, ScaleError> {
    scale_recipe_with(recipe, factor, ScaleOptions::default())
}

/// Scale a recipe by a given factor with explicit options.
///
/// Returns an owned `ScaledRecipe`. In the default (linear) mode, all quantities
/// scale linearly and non-linear ingredients only produce *warnings* — the
/// recipe's stated times are never modified.
///
/// When `options.rules` is set, the deterministic non-linear engine adjusts
/// leavening (sub-linearly), renders salt/spice as to-taste bands, annotates
/// pan-coating fat, and adds advisory cook-time / pan-capacity suggestions. Every
/// adjusted line keeps its pure-linear value (`linear_quantity`) so the change is
/// reversible, plus an `explanation`.
pub fn scale_recipe_with(
    recipe: &Recipe,
    factor: ScaleFactor,
    options: ScaleOptions,
) -> Result<ScaledRecipe, ScaleError> {
    let multiplier = match factor {
        ScaleFactor::Multiplier(m) => {
            if m <= 0.0 || !m.is_finite() {
                return Err(ScaleError::InvalidFactor(m));
            }
            m
        }
        ScaleFactor::ToServings(target) => {
            if target == 0 {
                return Err(ScaleError::InvalidServings(0));
            }
            let servings_str = recipe
                .servings
                .as_deref()
                .ok_or(ScaleError::NoServingsMetadata)?;
            let original = parse_servings(servings_str)
                .ok_or_else(|| ScaleError::UnparseableServings(servings_str.to_string()))?;
            target as f64 / original
        }
    };

    let mut warnings = Vec::new();
    let mut ingredients = Vec::new();

    for ing in &recipe.ingredients {
        let category = classify_ingredient(&ing.name);

        // Parse and linearly scale the quantity.
        let parsed = ing.quantity.as_deref().and_then(parse_quantity);
        let (linear_scaled_str, qty_warning) = match &ing.quantity {
            Some(qty_str) => match &parsed {
                Some(p) => (Some(format_quantity(p.value * multiplier)), None),
                None => (
                    // Unparseable quantity — pass through with warning.
                    Some(qty_str.clone()),
                    Some(format!(
                        "could not parse quantity '{}' — showing original",
                        qty_str
                    )),
                ),
            },
            None => (None, None),
        };

        // Apply the non-linear rules engine (rules mode + parseable quantity only).
        let mut scaled_quantity = linear_scaled_str.clone();
        let mut linear_quantity: Option<String> = None;
        let mut explanation: Option<String> = None;
        let mut rule_adjusted = false;

        if options.rules
            && let Some(p) = &parsed
            && let Some(adj) = adjust_quantity(category, p.value, multiplier, format_quantity)
        {
            scaled_quantity = Some(adj.primary);
            linear_quantity = Some(adj.linear);
            explanation = Some(adj.explanation);
            rule_adjusted = true;
        }

        // Non-linear *warning* (default mechanism). Suppressed for lines the
        // rules engine already adjusted, since the explanation supersedes it.
        let category_warning = if rule_adjusted {
            None
        } else {
            category.warning(multiplier)
        };

        // Line-level warning combines any parse warning with the category warning.
        let combined_warning = match (&qty_warning, &category_warning) {
            (Some(q), Some(c)) => Some(format!("{q}; {c}")),
            (Some(q), None) => Some(q.clone()),
            (None, Some(c)) => Some(c.clone()),
            (None, None) => None,
        };

        if let Some(ref w) = category_warning {
            warnings.push(ScalingWarning {
                ingredient: ing.name.clone(),
                message: w.clone(),
            });
        }
        if let Some(ref w) = qty_warning {
            warnings.push(ScalingWarning {
                ingredient: ing.name.clone(),
                message: w.clone(),
            });
        }

        // For quantity-less ingredients whose category carries a non-linear
        // *warning* (leavening/salt/spice/thickener), still warn (but not at 1x).
        // Categories that scale linearly (incl. Fat/Flour/Egg/Liquid) are quiet.
        if ing.quantity.is_none()
            && category.warning(multiplier).is_some()
            && (multiplier - 1.0).abs() > f64::EPSILON
        {
            let soft_warning = format!("not scaled (no quantity); adjust {} carefully", ing.name);
            warnings.push(ScalingWarning {
                ingredient: ing.name.clone(),
                message: soft_warning.clone(),
            });
            ingredients.push(ScaledIngredient {
                name: ing.name.clone(),
                original_quantity: None,
                scaled_quantity: None,
                unit: ing.unit.clone(),
                note: ing.note.clone(),
                optional: ing.optional,
                category,
                warning: Some(soft_warning),
                linear_quantity: None,
                explanation: None,
            });
            continue;
        }

        ingredients.push(ScaledIngredient {
            name: ing.name.clone(),
            original_quantity: ing.quantity.clone(),
            scaled_quantity,
            unit: ing.unit.clone(),
            note: ing.note.clone(),
            optional: ing.optional,
            category,
            warning: combined_warning,
            linear_quantity,
            explanation,
        });
    }

    // Compute scaled servings display.
    let scaled_servings = recipe.servings.as_deref().and_then(|s| {
        parse_servings(s).map(|orig| {
            let scaled = orig * multiplier;
            format_quantity(scaled)
        })
    });

    // Advisory cook-time and pan-capacity suggestions (rules mode only).
    let is_scaling = (multiplier - 1.0).abs() > f64::EPSILON;
    let time_suggestion = if options.rules && is_scaling {
        recipe.cook_time.as_deref().map(|ct| {
            let tm = cook_time_multiplier(multiplier);
            format!(
                "cook time is NOT auto-scaled — recipe says {ct}. As a rough guide, \
                 time changes by ~×{tm:.2} (heat transfer scales by ~volume^⅔, not \
                 linearly). Check for doneness rather than trusting the clock."
            )
        })
    } else {
        None
    };

    let pan_note = if options.rules && multiplier > 1.5 {
        Some(format!(
            "batch is ×{}: confirm your pan/pot can hold it. Prefer a wider vessel \
             (same depth) — a deeper batch changes cook time and browning, and an \
             overcrowded sauté steams instead of searing.",
            format_quantity(multiplier)
        ))
    } else {
        None
    };

    Ok(ScaledRecipe {
        slug: recipe.slug.clone(),
        title: recipe.title.clone(),
        scale_factor: multiplier,
        original_servings: recipe.servings.clone(),
        scaled_servings,
        // Times are NEVER scaled.
        prep_time: recipe.prep_time.clone(),
        cook_time: recipe.cook_time.clone(),
        total_time: recipe.total_time.clone(),
        ingredients,
        warnings,
        tags: recipe.tags.clone(),
        rules_applied: options.rules,
        time_suggestion,
        pan_note,
    })
}

/// Parse the `--to` argument into a multiplier.
///
/// Accepts "2x", "2X", "2", "0.5x", "1/2x", etc.
pub fn parse_scale_arg(s: &str) -> Option<f64> {
    let trimmed = s.trim().trim_end_matches(['x', 'X']);
    parse_quantity(trimmed).map(|q| q.value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fond_domain::{Recipe, RecipeIngredient};

    fn test_recipe() -> Recipe {
        Recipe {
            slug: "test".to_string(),
            title: "Test Recipe".to_string(),
            source: None,
            source_url: None,
            description: None,
            recipe_yield: None,
            prep_time: Some("10 minutes".to_string()),
            cook_time: Some("30 minutes".to_string()),
            total_time: Some("40 minutes".to_string()),
            servings: Some("4".to_string()),
            ingredients: vec![
                RecipeIngredient {
                    name: "chicken".to_string(),
                    quantity: Some("2".to_string()),
                    unit: Some("lbs".to_string()),
                    note: None,
                    optional: false,
                },
                RecipeIngredient {
                    name: "soy sauce".to_string(),
                    quantity: Some("1/4".to_string()),
                    unit: Some("cup".to_string()),
                    note: None,
                    optional: false,
                },
                RecipeIngredient {
                    name: "baking powder".to_string(),
                    quantity: Some("1".to_string()),
                    unit: Some("tsp".to_string()),
                    note: None,
                    optional: false,
                },
                RecipeIngredient {
                    name: "salt".to_string(),
                    quantity: None,
                    unit: None,
                    note: None,
                    optional: false,
                },
            ],
            steps: vec![],
            cookware: vec![],
            tags: vec!["asian".to_string()],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            raw_source: None,
        }
    }

    fn ing(name: &str, qty: Option<&str>, unit: Option<&str>) -> RecipeIngredient {
        RecipeIngredient {
            name: name.to_string(),
            quantity: qty.map(str::to_string),
            unit: unit.map(str::to_string),
            note: None,
            optional: false,
        }
    }

    fn recipe_with(
        title: &str,
        servings: Option<&str>,
        cook_time: Option<&str>,
        ingredients: Vec<RecipeIngredient>,
    ) -> Recipe {
        Recipe {
            slug: "fixture".to_string(),
            title: title.to_string(),
            source: None,
            source_url: None,
            description: None,
            recipe_yield: None,
            prep_time: None,
            cook_time: cook_time.map(str::to_string),
            total_time: None,
            servings: servings.map(str::to_string),
            ingredients,
            steps: vec![],
            cookware: vec![],
            tags: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            raw_source: None,
        }
    }

    fn find<'a>(scaled: &'a ScaledRecipe, name: &str) -> &'a ScaledIngredient {
        scaled
            .ingredients
            .iter()
            .find(|i| i.name == name)
            .unwrap_or_else(|| panic!("ingredient '{name}' not found"))
    }

    #[test]
    fn scale_by_multiplier_doubles() {
        let recipe = test_recipe();
        let result = scale_recipe(&recipe, ScaleFactor::Multiplier(2.0)).unwrap();

        assert_eq!(result.scale_factor, 2.0);
        assert_eq!(result.ingredients[0].scaled_quantity.as_deref(), Some("4"));
        assert_eq!(
            result.ingredients[1].scaled_quantity.as_deref(),
            Some("1/2")
        );
        assert_eq!(result.ingredients[2].scaled_quantity.as_deref(), Some("2"));
    }

    #[test]
    fn scale_by_multiplier_halves() {
        let recipe = test_recipe();
        let result = scale_recipe(&recipe, ScaleFactor::Multiplier(0.5)).unwrap();

        assert_eq!(result.ingredients[0].scaled_quantity.as_deref(), Some("1"));
        assert_eq!(
            result.ingredients[1].scaled_quantity.as_deref(),
            Some("1/8")
        );
    }

    #[test]
    fn scale_to_servings() {
        let recipe = test_recipe();
        let result = scale_recipe(&recipe, ScaleFactor::ToServings(8)).unwrap();

        assert_eq!(result.scale_factor, 2.0);
        assert_eq!(result.scaled_servings.as_deref(), Some("8"));
        assert_eq!(result.ingredients[0].scaled_quantity.as_deref(), Some("4"));
    }

    #[test]
    fn scale_preserves_times() {
        let recipe = test_recipe();
        let result = scale_recipe(&recipe, ScaleFactor::Multiplier(3.0)).unwrap();

        assert_eq!(result.prep_time.as_deref(), Some("10 minutes"));
        assert_eq!(result.cook_time.as_deref(), Some("30 minutes"));
        assert_eq!(result.total_time.as_deref(), Some("40 minutes"));
    }

    #[test]
    fn scale_warns_non_linear() {
        let recipe = test_recipe();
        let result = scale_recipe(&recipe, ScaleFactor::Multiplier(2.0)).unwrap();

        // Should warn about soy sauce (salt), baking powder (leavening), and bare salt
        let warned_names: Vec<&str> = result
            .warnings
            .iter()
            .map(|w| w.ingredient.as_str())
            .collect();
        assert!(warned_names.contains(&"soy sauce"));
        assert!(warned_names.contains(&"baking powder"));
        assert!(warned_names.contains(&"salt")); // quantity-less non-linear
    }

    #[test]
    fn scale_no_warnings_at_1x() {
        let recipe = test_recipe();
        let result = scale_recipe(&recipe, ScaleFactor::Multiplier(1.0)).unwrap();
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn scale_error_zero_factor() {
        let recipe = test_recipe();
        assert!(scale_recipe(&recipe, ScaleFactor::Multiplier(0.0)).is_err());
    }

    #[test]
    fn scale_error_negative_factor() {
        let recipe = test_recipe();
        assert!(scale_recipe(&recipe, ScaleFactor::Multiplier(-1.0)).is_err());
    }

    #[test]
    fn scale_error_zero_servings() {
        let recipe = test_recipe();
        assert!(scale_recipe(&recipe, ScaleFactor::ToServings(0)).is_err());
    }

    #[test]
    fn scale_error_no_servings_metadata() {
        let mut recipe = test_recipe();
        recipe.servings = None;
        assert!(scale_recipe(&recipe, ScaleFactor::ToServings(8)).is_err());
    }

    #[test]
    fn scale_quantity_less_linear_no_warning() {
        let recipe = Recipe {
            slug: "test".to_string(),
            title: "Test".to_string(),
            servings: Some("4".to_string()),
            ingredients: vec![RecipeIngredient {
                name: "chicken".to_string(),
                quantity: None,
                unit: None,
                note: None,
                optional: false,
            }],
            steps: vec![],
            cookware: vec![],
            tags: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            source: None,
            source_url: None,
            description: None,
            recipe_yield: None,
            prep_time: None,
            cook_time: None,
            total_time: None,
            raw_source: None,
        };
        let result = scale_recipe(&recipe, ScaleFactor::Multiplier(2.0)).unwrap();
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn parse_scale_arg_variants() {
        assert_eq!(parse_scale_arg("2x"), Some(2.0));
        assert_eq!(parse_scale_arg("2X"), Some(2.0));
        assert_eq!(parse_scale_arg("2"), Some(2.0));
        assert_eq!(parse_scale_arg("0.5x"), Some(0.5));
        assert_eq!(parse_scale_arg("1/2x"), Some(0.5));
        assert_eq!(parse_scale_arg("1.5"), Some(1.5));
        assert!(parse_scale_arg("abc").is_none());
    }

    #[test]
    fn scale_unparseable_quantity_passes_through() {
        let recipe = Recipe {
            slug: "test".to_string(),
            title: "Test".to_string(),
            servings: Some("4".to_string()),
            ingredients: vec![RecipeIngredient {
                name: "garlic".to_string(),
                quantity: Some("a few cloves".to_string()),
                unit: None,
                note: None,
                optional: false,
            }],
            steps: vec![],
            cookware: vec![],
            tags: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            source: None,
            source_url: None,
            description: None,
            recipe_yield: None,
            prep_time: None,
            cook_time: None,
            total_time: None,
            raw_source: None,
        };
        let result = scale_recipe(&recipe, ScaleFactor::Multiplier(2.0)).unwrap();
        assert_eq!(
            result.ingredients[0].scaled_quantity.as_deref(),
            Some("a few cloves")
        );
        assert!(result.warnings.iter().any(|w| w.ingredient == "garlic"));
    }

    // ── Rules mode: classic cases ────────────────────────────────────

    fn cake() -> Recipe {
        recipe_with(
            "Yellow Cake",
            Some("8"),
            Some("30 minutes"),
            vec![
                ing("all-purpose flour", Some("2"), Some("cups")),
                ing("sugar", Some("1"), Some("cup")),
                ing("large eggs", Some("2"), None),
                ing("whole milk", Some("1"), Some("cup")),
                ing("baking powder", Some("2"), Some("tsp")),
                ing("salt", Some("1"), Some("tsp")),
                ing("unsalted butter", Some("1/2"), Some("cup")),
            ],
        )
    }

    #[test]
    fn rules_doubling_cake_leavening_sublinear() {
        let scaled = scale_recipe_with(
            &cake(),
            ScaleFactor::Multiplier(2.0),
            ScaleOptions { rules: true },
        )
        .unwrap();

        assert!(scaled.rules_applied);

        // Leavening: 2 tsp × 2^0.75 ≈ 3.36 tsp — NOT the linear 4 tsp.
        let bp = find(&scaled, "baking powder");
        assert_eq!(bp.category, ScalingCategory::Leavening);
        assert_eq!(bp.linear_quantity.as_deref(), Some("4"));
        assert_ne!(bp.scaled_quantity.as_deref(), Some("4"));
        assert!(bp.scaled_quantity.is_some());
        let expl = bp.explanation.as_deref().expect("leavening explanation");
        assert!(expl.contains("sub-linear"), "explanation: {expl}");

        // Linear structural ingredients double exactly.
        assert_eq!(
            find(&scaled, "all-purpose flour")
                .scaled_quantity
                .as_deref(),
            Some("4")
        );
        assert_eq!(find(&scaled, "sugar").scaled_quantity.as_deref(), Some("2"));
        assert_eq!(
            find(&scaled, "large eggs").scaled_quantity.as_deref(),
            Some("4")
        );
        assert_eq!(
            find(&scaled, "whole milk").scaled_quantity.as_deref(),
            Some("2")
        );
        assert_eq!(find(&scaled, "large eggs").category, ScalingCategory::Egg);
        assert_eq!(
            find(&scaled, "whole milk").category,
            ScalingCategory::Liquid
        );
        assert_eq!(
            find(&scaled, "all-purpose flour").category,
            ScalingCategory::Flour
        );

        // Salt becomes a to-taste band, with the linear value preserved.
        let salt = find(&scaled, "salt");
        assert_eq!(salt.category, ScalingCategory::Salt);
        assert!(salt.scaled_quantity.as_deref().unwrap().contains("to"));
        assert_eq!(salt.linear_quantity.as_deref(), Some("2"));

        // Advisory suggestions present.
        assert!(scaled.time_suggestion.is_some());
        assert!(scaled.pan_note.is_some());
    }

    #[test]
    fn rules_deterministic() {
        let a = scale_recipe_with(
            &cake(),
            ScaleFactor::Multiplier(2.0),
            ScaleOptions { rules: true },
        )
        .unwrap();
        let b = scale_recipe_with(
            &cake(),
            ScaleFactor::Multiplier(2.0),
            ScaleOptions { rules: true },
        )
        .unwrap();
        let ja = serde_json::to_string(&a).unwrap();
        let jb = serde_json::to_string(&b).unwrap();
        assert_eq!(ja, jb);
    }

    #[test]
    fn rules_halving_bread_leavening_stays_linear() {
        let bread = recipe_with(
            "Sandwich Bread",
            Some("2"),
            Some("40 minutes"),
            vec![
                ing("bread flour", Some("4"), Some("cups")),
                ing("water", Some("1.5"), Some("cups")),
                ing("active dry yeast", Some("2"), Some("tsp")),
                ing("salt", Some("2"), Some("tsp")),
            ],
        );
        let scaled = scale_recipe_with(
            &bread,
            ScaleFactor::Multiplier(0.5),
            ScaleOptions { rules: true },
        )
        .unwrap();

        // Scaling DOWN: leavening stays linear (no sub-linear adjustment).
        let yeast = find(&scaled, "active dry yeast");
        assert_eq!(yeast.category, ScalingCategory::Leavening);
        assert_eq!(yeast.scaled_quantity.as_deref(), Some("1"));
        assert!(yeast.linear_quantity.is_none());
        assert!(yeast.explanation.is_none());

        // Salt band still applies (shows a reduced range).
        let salt = find(&scaled, "salt");
        assert!(salt.scaled_quantity.as_deref().unwrap().contains("to"));

        // Scaling down should not raise a pan-capacity note.
        assert!(scaled.pan_note.is_none());
    }

    #[test]
    fn rules_braise_linear_body_with_time_suggestion() {
        let braise = recipe_with(
            "Beef Braise",
            Some("6"),
            Some("3 hours"),
            vec![
                ing("beef chuck", Some("3"), Some("lbs")),
                ing("beef stock", Some("4"), Some("cups")),
                ing("carrot", Some("4"), None),
                ing("salt", Some("1"), Some("tbsp")),
            ],
        );
        let scaled = scale_recipe_with(
            &braise,
            ScaleFactor::Multiplier(2.0),
            ScaleOptions { rules: true },
        )
        .unwrap();

        // Bulk ingredients scale linearly.
        assert_eq!(
            find(&scaled, "beef chuck").scaled_quantity.as_deref(),
            Some("6")
        );
        assert_eq!(
            find(&scaled, "beef stock").scaled_quantity.as_deref(),
            Some("8")
        );
        assert_eq!(
            find(&scaled, "carrot").scaled_quantity.as_deref(),
            Some("8")
        );
        assert_eq!(
            find(&scaled, "beef stock").category,
            ScalingCategory::Liquid
        );

        // Salt band + preserved linear reference.
        let salt = find(&scaled, "salt");
        assert!(salt.scaled_quantity.as_deref().unwrap().contains("to"));
        assert_eq!(salt.linear_quantity.as_deref(), Some("2"));

        // Cook time never rewritten, but an advisory suggestion is attached.
        assert_eq!(scaled.cook_time.as_deref(), Some("3 hours"));
        let ts = scaled.time_suggestion.as_deref().expect("time suggestion");
        assert!(ts.contains("3 hours"));
        assert!(ts.contains("NOT auto-scaled"));
    }

    #[test]
    fn default_mode_unchanged_by_rules_work() {
        // Without rules, non-linear lines keep the linear value + warnings only.
        let scaled = scale_recipe(&cake(), ScaleFactor::Multiplier(2.0)).unwrap();
        assert!(!scaled.rules_applied);
        assert!(scaled.time_suggestion.is_none());
        assert!(scaled.pan_note.is_none());

        let bp = find(&scaled, "baking powder");
        assert_eq!(bp.scaled_quantity.as_deref(), Some("4")); // pure linear
        assert!(bp.linear_quantity.is_none());
        assert!(bp.explanation.is_none());
        // But it still warns.
        assert!(
            scaled
                .warnings
                .iter()
                .any(|w| w.ingredient == "baking powder")
        );
    }

    #[test]
    fn rules_no_op_at_1x() {
        let scaled = scale_recipe_with(
            &cake(),
            ScaleFactor::Multiplier(1.0),
            ScaleOptions { rules: true },
        )
        .unwrap();
        // At 1×, nothing is adjusted and no advisories appear.
        assert!(scaled.time_suggestion.is_none());
        assert!(scaled.pan_note.is_none());
        for ing in &scaled.ingredients {
            assert!(ing.explanation.is_none());
            assert!(ing.linear_quantity.is_none());
        }
    }

    #[test]
    fn default_quantity_less_fat_flour_stay_quiet() {
        // Quantity-less "for greasing/dusting" fat & flour must NOT warn in
        // default mode (they scale linearly) — regression guard for the
        // expanded classifier.
        let recipe = recipe_with(
            "Greased Pan",
            Some("4"),
            None,
            vec![
                ing("butter", None, None),        // for greasing
                ing("flour", None, None),         // for dusting
                ing("baking powder", None, None), // leavening → still warns
            ],
        );
        let scaled = scale_recipe(&recipe, ScaleFactor::Multiplier(2.0)).unwrap();

        let warned: Vec<&str> = scaled
            .warnings
            .iter()
            .map(|w| w.ingredient.as_str())
            .collect();
        assert!(
            !warned.contains(&"butter"),
            "fat should be quiet: {warned:?}"
        );
        assert!(
            !warned.contains(&"flour"),
            "flour should be quiet: {warned:?}"
        );
        // Leavening (a warning category) still surfaces.
        assert!(warned.contains(&"baking powder"));

        // And the quiet lines are still emitted (not dropped).
        assert!(scaled.ingredients.iter().any(|i| i.name == "butter"));
        assert!(scaled.ingredients.iter().any(|i| i.name == "flour"));
    }
}
