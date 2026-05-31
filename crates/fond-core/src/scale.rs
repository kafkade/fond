use fond_domain::Recipe;
use serde::Serialize;

use crate::ingredient_class::{ScalingCategory, classify_ingredient};
use crate::quantity::{format_quantity, parse_quantity, parse_servings};

/// How to scale a recipe.
#[derive(Debug, Clone)]
pub enum ScaleFactor {
    /// Multiply all quantities by this factor (e.g., 2.0 for doubling).
    Multiplier(f64),
    /// Scale to a target number of servings.
    ToServings(u32),
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
}

/// Scale a recipe by a given factor.
///
/// Returns an owned `ScaledRecipe` with scaled quantities and any
/// warnings about non-linear ingredients. Times are never modified.
pub fn scale_recipe(recipe: &Recipe, factor: ScaleFactor) -> Result<ScaledRecipe, ScaleError> {
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

        // Parse and scale the quantity
        let (scaled_qty_str, qty_warning) = match &ing.quantity {
            Some(qty_str) => match parse_quantity(qty_str) {
                Some(parsed) => {
                    let scaled_value = parsed.value * multiplier;
                    let formatted = format_quantity(scaled_value);
                    (Some(formatted), None)
                }
                None => {
                    // Unparseable quantity — pass through with warning
                    (
                        Some(qty_str.clone()),
                        Some(format!(
                            "could not parse quantity '{}' — showing original",
                            qty_str
                        )),
                    )
                }
            },
            None => {
                // No quantity (e.g., bare "salt", "pepper to taste")
                (None, None)
            }
        };

        // Generate non-linear scaling warning
        let category_warning = category.warning(multiplier);

        // Combine warnings
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

        // For quantity-less non-linear ingredients, still warn (but not at 1x)
        if ing.quantity.is_none()
            && category != ScalingCategory::Linear
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
            });
            continue;
        }

        ingredients.push(ScaledIngredient {
            name: ing.name.clone(),
            original_quantity: ing.quantity.clone(),
            scaled_quantity: scaled_qty_str,
            unit: ing.unit.clone(),
            note: ing.note.clone(),
            optional: ing.optional,
            category,
            warning: combined_warning,
        });
    }

    // Compute scaled servings display
    let scaled_servings = recipe.servings.as_deref().and_then(|s| {
        parse_servings(s).map(|orig| {
            let scaled = orig * multiplier;
            format_quantity(scaled)
        })
    });

    Ok(ScaledRecipe {
        slug: recipe.slug.clone(),
        title: recipe.title.clone(),
        scale_factor: multiplier,
        original_servings: recipe.servings.clone(),
        scaled_servings,
        // Times are NEVER scaled
        prep_time: recipe.prep_time.clone(),
        cook_time: recipe.cook_time.clone(),
        total_time: recipe.total_time.clone(),
        ingredients,
        warnings,
        tags: recipe.tags.clone(),
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
}
