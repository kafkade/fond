use rusqlite::params;
use serde::Serialize;

use crate::db::FondDb;
use crate::error::StoreError;
use crate::pantry::normalize_for_matching;

// ═══════════════════════════════════════════════════════════════════
// Embedded USDA data
// ═══════════════════════════════════════════════════════════════════

const USDA_CSV: &str = include_str!("../../../data/usda/usda_nutrition_subset.csv");

// ═══════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════

/// Nutrition values (per serving or total).
#[derive(Debug, Clone, Serialize)]
pub struct NutritionValues {
    pub kcal: f64,
    pub protein_g: f64,
    pub fat_g: f64,
    pub carb_g: f64,
    pub fiber_g: Option<f64>,
    pub sugar_g: Option<f64>,
    pub sodium_mg: Option<f64>,
}

impl NutritionValues {
    fn zero() -> Self {
        Self {
            kcal: 0.0,
            protein_g: 0.0,
            fat_g: 0.0,
            carb_g: 0.0,
            fiber_g: Some(0.0),
            sugar_g: Some(0.0),
            sodium_mg: Some(0.0),
        }
    }

    fn add(&mut self, other: &NutritionValues, scale: f64) {
        self.kcal += other.kcal * scale;
        self.protein_g += other.protein_g * scale;
        self.fat_g += other.fat_g * scale;
        self.carb_g += other.carb_g * scale;
        if let (Some(a), Some(b)) = (&mut self.fiber_g, other.fiber_g) {
            *a += b * scale;
        } else {
            self.fiber_g = None;
        }
        if let (Some(a), Some(b)) = (&mut self.sugar_g, other.sugar_g) {
            *a += b * scale;
        } else {
            self.sugar_g = None;
        }
        if let (Some(a), Some(b)) = (&mut self.sodium_mg, other.sodium_mg) {
            *a += b * scale;
        } else {
            self.sodium_mg = None;
        }
    }

    /// Round values for display (kcal to 10, macros to 1g, sodium to 10mg).
    pub fn rounded(&self) -> NutritionValues {
        NutritionValues {
            kcal: (self.kcal / 10.0).round() * 10.0,
            protein_g: self.protein_g.round(),
            fat_g: self.fat_g.round(),
            carb_g: self.carb_g.round(),
            fiber_g: self.fiber_g.map(|v| v.round()),
            sugar_g: self.sugar_g.map(|v| v.round()),
            sodium_mg: self.sodium_mg.map(|v| (v / 10.0).round() * 10.0),
        }
    }
}

/// Match confidence for ingredient → USDA food mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MatchConfidence {
    /// Exact or near-exact normalized match.
    High,
    /// Good word overlap (≥ 0.6).
    Medium,
}

/// A matched ingredient with nutrition data.
#[derive(Debug, Clone, Serialize)]
pub struct IngredientNutritionMatch {
    pub ingredient_name: String,
    pub usda_description: String,
    pub confidence: MatchConfidence,
    pub grams: f64,
    pub per_100g: NutritionValues,
    pub contribution: NutritionValues,
}

/// Reason an ingredient could not be included in the estimate.
#[derive(Debug, Clone, Serialize)]
pub struct UnmatchedIngredient {
    pub name: String,
    pub reason: UnmatchedReason,
}

/// Why an ingredient was not included.
#[derive(Debug, Clone, Serialize)]
pub enum UnmatchedReason {
    /// No USDA food match found.
    NoFoodMatch,
    /// Unit could not be converted to grams.
    UnconvertibleUnit,
    /// No quantity specified.
    MissingQuantity,
    /// Ingredient is optional and excluded by default.
    Optional,
}

impl std::fmt::Display for UnmatchedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoFoodMatch => write!(f, "no USDA match"),
            Self::UnconvertibleUnit => write!(f, "unit not convertible to grams"),
            Self::MissingQuantity => write!(f, "no quantity"),
            Self::Optional => write!(f, "optional ingredient"),
        }
    }
}

/// Full nutrition estimate for a recipe.
#[derive(Debug, Clone, Serialize)]
pub struct RecipeNutrition {
    pub recipe_slug: String,
    pub servings: Option<u32>,
    pub total: NutritionValues,
    pub per_serving: Option<NutritionValues>,
    pub ingredient_count: usize,
    pub matched_count: usize,
    pub coverage_pct: f64,
    pub matched: Vec<IngredientNutritionMatch>,
    pub unmatched: Vec<UnmatchedIngredient>,
    pub disclaimer: String,
}

pub const NUTRITION_DISCLAIMER: &str =
    "Estimates based on USDA FoodData Central. Not for medical use.";

// ═══════════════════════════════════════════════════════════════════
// Unit → grams conversion
// ═══════════════════════════════════════════════════════════════════

/// Convert a quantity + unit to grams for a given ingredient.
///
/// Only converts:
/// - Mass units (g, kg, oz, lb) — always safe
/// - Curated ingredient-specific counts (egg, clove, etc.)
/// - Volume units for liquids only (cup, tbsp, tsp for water-density items)
///
/// Returns `None` for ambiguous or unknown conversions.
fn quantity_to_grams(quantity: f64, unit: &str, ingredient_name: &str) -> Option<f64> {
    let unit_lower = unit.to_lowercase();
    let unit_trimmed = unit_lower.trim();

    // Mass units — always safe
    match unit_trimmed {
        "g" | "gram" | "grams" => return Some(quantity),
        "kg" | "kilogram" | "kilograms" => return Some(quantity * 1000.0),
        "oz" | "ounce" | "ounces" => return Some(quantity * 28.3495),
        "lb" | "lbs" | "pound" | "pounds" => return Some(quantity * 453.592),
        _ => {}
    }

    // Curated count-based conversions
    let norm = normalize_for_matching(ingredient_name);
    if unit_trimmed.is_empty()
        || unit_trimmed == "whole"
        || unit_trimmed == "large"
        || unit_trimmed == "medium"
        || unit_trimmed == "small"
    {
        return count_to_grams(&norm, quantity);
    }

    if (unit_trimmed == "clove" || unit_trimmed == "cloves") && norm.contains("garlic") {
        return Some(quantity * 3.0);
    }

    // Volume units — only for liquids and a small curated set
    if matches!(unit_trimmed, "cup" | "cups") {
        return cup_to_grams(&norm, quantity);
    }
    if matches!(unit_trimmed, "tbsp" | "tablespoon" | "tablespoons") {
        return tbsp_to_grams(&norm, quantity);
    }
    if matches!(unit_trimmed, "tsp" | "teaspoon" | "teaspoons") {
        return tsp_to_grams(&norm, quantity);
    }
    if matches!(unit_trimmed, "ml" | "milliliter" | "milliliters") && is_liquid_ingredient(&norm) {
        return Some(quantity);
    }
    if matches!(unit_trimmed, "l" | "liter" | "liters" | "litre" | "litres")
        && is_liquid_ingredient(&norm)
    {
        return Some(quantity * 1000.0);
    }

    None
}

/// Convert count-based quantities to grams for common ingredients.
fn count_to_grams(norm_name: &str, quantity: f64) -> Option<f64> {
    // Average weights for common countable ingredients
    if norm_name.contains("egg") {
        return Some(quantity * 50.0);
    }
    if norm_name.contains("onion") {
        return Some(quantity * 150.0);
    }
    if norm_name.contains("tomato") && !norm_name.contains("paste") && !norm_name.contains("sauce")
    {
        return Some(quantity * 150.0);
    }
    if norm_name.contains("potato") {
        return Some(quantity * 170.0);
    }
    if norm_name.contains("lemon") || norm_name.contains("lime") {
        return Some(quantity * 65.0);
    }
    if norm_name.contains("orange") {
        return Some(quantity * 130.0);
    }
    if norm_name.contains("banana") {
        return Some(quantity * 120.0);
    }
    if norm_name.contains("apple") {
        return Some(quantity * 180.0);
    }
    if norm_name.contains("avocado") {
        return Some(quantity * 150.0);
    }
    if norm_name.contains("bell pepper")
        || norm_name.contains("pepper")
            && !norm_name.contains("black")
            && !norm_name.contains("cayenne")
    {
        return Some(quantity * 120.0);
    }
    if norm_name.contains("carrot") {
        return Some(quantity * 60.0);
    }
    if norm_name.contains("celery") {
        // per stalk
        return Some(quantity * 40.0);
    }
    if norm_name.contains("garlic") && !norm_name.contains("powder") {
        // whole head not matched here, cloves handled separately
        return Some(quantity * 3.0);
    }
    None
}

/// Cup → grams for curated ingredients.
fn cup_to_grams(norm_name: &str, cups: f64) -> Option<f64> {
    let g_per_cup = if norm_name.contains("flour") && !norm_name.contains("almond") {
        125.0
    } else if norm_name.contains("almond flour") || norm_name.contains("almond meal") {
        96.0
    } else if norm_name.contains("sugar") && norm_name.contains("brown") {
        220.0
    } else if norm_name.contains("sugar") && norm_name.contains("powdered") {
        120.0
    } else if norm_name.contains("sugar") {
        200.0
    } else if norm_name.contains("butter") {
        227.0
    } else if norm_name.contains("rice") {
        185.0
    } else if norm_name.contains("oat") {
        80.0
    } else if norm_name.contains("honey") || norm_name.contains("maple syrup") {
        340.0
    } else if is_liquid_ingredient(norm_name) {
        237.0
    } else {
        return None; // ambiguous — skip
    };
    Some(cups * g_per_cup)
}

/// Tbsp → grams for curated ingredients.
fn tbsp_to_grams(norm_name: &str, tbsp: f64) -> Option<f64> {
    let g_per_tbsp = if norm_name.contains("butter") {
        14.0
    } else if norm_name.contains("flour") {
        8.0
    } else if norm_name.contains("sugar") {
        12.5
    } else if norm_name.contains("honey") || norm_name.contains("maple syrup") {
        21.0
    } else if is_liquid_ingredient(norm_name)
        || norm_name.contains("oil")
        || norm_name.contains("vinegar")
        || norm_name.contains("sauce")
    {
        15.0
    } else if norm_name.contains("salt")
        || norm_name.contains("spice")
        || norm_name.contains("powder")
        || norm_name.contains("cinnamon")
        || norm_name.contains("cumin")
        || norm_name.contains("paprika")
    {
        9.0
    } else {
        return None;
    };
    Some(tbsp * g_per_tbsp)
}

/// Tsp → grams for curated ingredients.
fn tsp_to_grams(norm_name: &str, tsp: f64) -> Option<f64> {
    let g_per_tsp = if norm_name.contains("salt") {
        6.0
    } else if norm_name.contains("sugar") {
        4.2
    } else if norm_name.contains("baking powder") || norm_name.contains("baking soda") {
        4.6
    } else if norm_name.contains("vanilla") {
        4.2
    } else if is_liquid_ingredient(norm_name)
        || norm_name.contains("oil")
        || norm_name.contains("vinegar")
        || norm_name.contains("sauce")
    {
        5.0
    } else if norm_name.contains("powder")
        || norm_name.contains("spice")
        || norm_name.contains("cinnamon")
        || norm_name.contains("cumin")
        || norm_name.contains("paprika")
        || norm_name.contains("pepper")
        || norm_name.contains("oregano")
        || norm_name.contains("thyme")
    {
        3.0
    } else {
        return None;
    };
    Some(tsp * g_per_tsp)
}

/// Check if an ingredient is a liquid (safe to assume water density for volume).
fn is_liquid_ingredient(norm_name: &str) -> bool {
    const LIQUID_KEYWORDS: &[&str] = &[
        "water",
        "milk",
        "cream",
        "broth",
        "stock",
        "juice",
        "wine",
        "beer",
        "vinegar",
        "oil",
        "sauce",
        "soy sauce",
        "fish sauce",
        "coconut milk",
        "buttermilk",
        "yogurt",
    ];
    LIQUID_KEYWORDS.iter().any(|kw| norm_name.contains(kw))
}

// ═══════════════════════════════════════════════════════════════════
// Repository
// ═══════════════════════════════════════════════════════════════════

/// Repository for USDA nutrition data and recipe nutrition estimation.
pub struct NutritionRepository<'a> {
    db: &'a FondDb,
}

impl<'a> NutritionRepository<'a> {
    pub fn new(db: &'a FondDb) -> Self {
        Self { db }
    }

    /// Seed the nutrition_facts table from the embedded USDA CSV.
    ///
    /// Idempotent — only inserts if the table is empty.
    pub fn seed_nutrition_facts(&self) -> Result<(), StoreError> {
        let conn = self.db.conn();

        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM nutrition_facts", [], |row| row.get(0))?;

        if count > 0 {
            return Ok(());
        }

        let tx = conn.unchecked_transaction()?;

        for line in USDA_CSV.lines().skip(1) {
            if let Some(record) = parse_csv_line(line) {
                let normalized = normalize_for_matching(&record.description);
                tx.execute(
                    "INSERT OR IGNORE INTO nutrition_facts
                     (fdc_id, description, normalized_description, category,
                      kcal, protein_g, fat_g, carb_g, fiber_g, sugar_g, sodium_mg)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        record.fdc_id,
                        record.description,
                        normalized,
                        record.category,
                        record.kcal,
                        record.protein_g,
                        record.fat_g,
                        record.carb_g,
                        record.fiber_g,
                        record.sugar_g,
                        record.sodium_mg,
                    ],
                )?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// Find the best USDA food match for an ingredient name.
    pub fn find_best_match(
        &self,
        ingredient_name: &str,
    ) -> Result<Option<(NutritionFactRow, MatchConfidence)>, StoreError> {
        let conn = self.db.conn();
        let norm = normalize_for_matching(ingredient_name);

        if norm.is_empty() {
            return Ok(None);
        }

        // 1. Try exact normalized match
        let exact: Option<NutritionFactRow> = conn
            .query_row(
                "SELECT fdc_id, description, category, kcal, protein_g, fat_g, carb_g,
                        fiber_g, sugar_g, sodium_mg
                 FROM nutrition_facts WHERE normalized_description = ?1 LIMIT 1",
                params![norm],
                |row| {
                    Ok(NutritionFactRow {
                        fdc_id: row.get(0)?,
                        description: row.get(1)?,
                        category: row.get(2)?,
                        kcal: row.get(3)?,
                        protein_g: row.get(4)?,
                        fat_g: row.get(5)?,
                        carb_g: row.get(6)?,
                        fiber_g: row.get(7)?,
                        sugar_g: row.get(8)?,
                        sodium_mg: row.get(9)?,
                    })
                },
            )
            .ok();

        if let Some(row) = exact {
            return Ok(Some((row, MatchConfidence::High)));
        }

        // 2. Fuzzy matching — load candidates and score by word overlap
        let norm_words: Vec<&str> = norm.split_whitespace().collect();
        if norm_words.is_empty() {
            return Ok(None);
        }

        // Query candidates that contain the primary keyword
        let primary_word = find_primary_keyword(&norm_words);
        let pattern = format!("%{primary_word}%");

        let mut stmt = conn.prepare(
            "SELECT fdc_id, description, normalized_description, category,
                    kcal, protein_g, fat_g, carb_g, fiber_g, sugar_g, sodium_mg
             FROM nutrition_facts
             WHERE normalized_description LIKE ?1
             LIMIT 500",
        )?;

        let candidates: Vec<(NutritionFactRow, String)> = stmt
            .query_map(params![pattern], |row| {
                Ok((
                    NutritionFactRow {
                        fdc_id: row.get(0)?,
                        description: row.get(1)?,
                        category: row.get(3)?,
                        kcal: row.get(4)?,
                        protein_g: row.get(5)?,
                        fat_g: row.get(6)?,
                        carb_g: row.get(7)?,
                        fiber_g: row.get(8)?,
                        sugar_g: row.get(9)?,
                        sodium_mg: row.get(10)?,
                    },
                    row.get::<_, String>(2)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        if candidates.is_empty() {
            return Ok(None);
        }

        // Score each candidate
        let mut best_score = 0.0f64;
        let mut best_match: Option<NutritionFactRow> = None;

        for (row, norm_desc) in &candidates {
            let desc_words: Vec<&str> = norm_desc.split_whitespace().collect();
            let score = word_overlap_score(&norm_words, &desc_words);

            if score > best_score {
                best_score = score;
                best_match = Some(row.clone());
            }
        }

        if let Some(m) = best_match {
            let confidence = if best_score >= 0.6 {
                MatchConfidence::Medium
            } else {
                return Ok(None); // Below threshold — skip
            };
            return Ok(Some((m, confidence)));
        }

        Ok(None)
    }

    /// Estimate nutrition for a recipe.
    pub fn estimate_recipe_nutrition(
        &self,
        recipe_slug: &str,
    ) -> Result<Option<RecipeNutrition>, StoreError> {
        let conn = self.db.conn();

        // Look up recipe
        let recipe_row: Option<(i64, Option<String>)> = conn
            .query_row(
                "SELECT id, servings FROM recipes WHERE slug = ?1",
                params![recipe_slug],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let Some((recipe_id, servings_str)) = recipe_row else {
            return Ok(None);
        };

        // Parse servings
        let servings = servings_str.as_deref().and_then(parse_servings);

        // Get ingredients
        let mut ing_stmt = conn.prepare(
            "SELECT name, quantity, unit, optional FROM recipe_ingredients
             WHERE recipe_id = ?1 ORDER BY sort_order",
        )?;

        struct RawIng {
            name: String,
            quantity: String,
            unit: String,
            optional: bool,
        }

        let ingredients: Vec<RawIng> = ing_stmt
            .query_map(params![recipe_id], |row| {
                Ok(RawIng {
                    name: row.get(0)?,
                    quantity: row.get(1)?,
                    unit: row.get(2)?,
                    optional: row.get::<_, i32>(3)? != 0,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        let ingredient_count = ingredients.len();
        let mut matched_items = Vec::new();
        let mut unmatched_items = Vec::new();
        let mut total = NutritionValues::zero();

        for ing in &ingredients {
            // Skip optional ingredients
            if ing.optional {
                unmatched_items.push(UnmatchedIngredient {
                    name: ing.name.clone(),
                    reason: UnmatchedReason::Optional,
                });
                continue;
            }

            // Parse quantity
            let qty = if ing.quantity.is_empty() {
                unmatched_items.push(UnmatchedIngredient {
                    name: ing.name.clone(),
                    reason: UnmatchedReason::MissingQuantity,
                });
                continue;
            } else {
                match crate::grocery::parse_quantity(&ing.quantity) {
                    Some(v) => v,
                    None => {
                        unmatched_items.push(UnmatchedIngredient {
                            name: ing.name.clone(),
                            reason: UnmatchedReason::MissingQuantity,
                        });
                        continue;
                    }
                }
            };

            // Convert to grams
            let grams = match quantity_to_grams(qty, &ing.unit, &ing.name) {
                Some(g) => g,
                None => {
                    unmatched_items.push(UnmatchedIngredient {
                        name: ing.name.clone(),
                        reason: UnmatchedReason::UnconvertibleUnit,
                    });
                    continue;
                }
            };

            // Find USDA match
            let usda_match = self.find_best_match(&ing.name)?;
            let Some((usda_row, confidence)) = usda_match else {
                unmatched_items.push(UnmatchedIngredient {
                    name: ing.name.clone(),
                    reason: UnmatchedReason::NoFoodMatch,
                });
                continue;
            };

            let per_100g = NutritionValues {
                kcal: usda_row.kcal,
                protein_g: usda_row.protein_g.unwrap_or(0.0),
                fat_g: usda_row.fat_g.unwrap_or(0.0),
                carb_g: usda_row.carb_g.unwrap_or(0.0),
                fiber_g: usda_row.fiber_g,
                sugar_g: usda_row.sugar_g,
                sodium_mg: usda_row.sodium_mg,
            };

            // Scale: grams / 100 = multiplier for per-100g values
            let scale = grams / 100.0;
            let mut contribution = NutritionValues::zero();
            contribution.add(&per_100g, scale);

            total.add(&per_100g, scale);

            matched_items.push(IngredientNutritionMatch {
                ingredient_name: ing.name.clone(),
                usda_description: usda_row.description.clone(),
                confidence,
                grams,
                per_100g,
                contribution,
            });
        }

        let matched_count = matched_items.len();
        let non_optional_count = ingredients.iter().filter(|i| !i.optional).count();
        let coverage_pct = if non_optional_count > 0 {
            (matched_count as f64 / non_optional_count as f64) * 100.0
        } else {
            0.0
        };

        let per_serving = servings.map(|s| {
            let divisor = s as f64;
            NutritionValues {
                kcal: total.kcal / divisor,
                protein_g: total.protein_g / divisor,
                fat_g: total.fat_g / divisor,
                carb_g: total.carb_g / divisor,
                fiber_g: total.fiber_g.map(|v| v / divisor),
                sugar_g: total.sugar_g.map(|v| v / divisor),
                sodium_mg: total.sodium_mg.map(|v| v / divisor),
            }
        });

        Ok(Some(RecipeNutrition {
            recipe_slug: recipe_slug.to_string(),
            servings,
            total: total.rounded(),
            per_serving: per_serving.map(|v| v.rounded()),
            ingredient_count,
            matched_count,
            coverage_pct: (coverage_pct * 10.0).round() / 10.0,
            matched: matched_items,
            unmatched: unmatched_items,
            disclaimer: NUTRITION_DISCLAIMER.to_string(),
        }))
    }
}

// ═══════════════════════════════════════════════════════════════════
// Internal helpers
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct NutritionFactRow {
    pub fdc_id: i64,
    pub description: String,
    pub category: String,
    pub kcal: f64,
    pub protein_g: Option<f64>,
    pub fat_g: Option<f64>,
    pub carb_g: Option<f64>,
    pub fiber_g: Option<f64>,
    pub sugar_g: Option<f64>,
    pub sodium_mg: Option<f64>,
}

struct CsvRecord {
    fdc_id: i64,
    description: String,
    category: String,
    kcal: f64,
    protein_g: Option<f64>,
    fat_g: Option<f64>,
    carb_g: Option<f64>,
    fiber_g: Option<f64>,
    sugar_g: Option<f64>,
    sodium_mg: Option<f64>,
}

/// Parse a CSV line, handling quoted fields.
fn parse_csv_line(line: &str) -> Option<CsvRecord> {
    let fields = split_csv_fields(line);
    if fields.len() < 11 {
        return None;
    }

    let fdc_id: i64 = fields[0].parse().ok()?;
    let description = fields[1].to_string();
    let category = fields[2].to_string();
    // fields[3] = data_type (skip)
    let kcal: f64 = fields[4].parse().ok()?;
    let protein_g: Option<f64> = parse_opt_f64(fields[5]);
    let fat_g: Option<f64> = parse_opt_f64(fields[6]);
    let carb_g: Option<f64> = parse_opt_f64(fields[7]);
    let fiber_g: Option<f64> = parse_opt_f64(fields[8]);
    let sugar_g: Option<f64> = parse_opt_f64(fields[9]);
    let sodium_mg: Option<f64> = parse_opt_f64(fields[10]);

    Some(CsvRecord {
        fdc_id,
        description,
        category,
        kcal,
        protein_g,
        fat_g,
        carb_g,
        fiber_g,
        sugar_g,
        sodium_mg,
    })
}

/// Split a CSV line respecting quoted fields.
fn split_csv_fields(line: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    let bytes = line.as_bytes();

    for i in 0..bytes.len() {
        if bytes[i] == b'"' {
            in_quotes = !in_quotes;
        } else if bytes[i] == b',' && !in_quotes {
            let field = &line[start..i];
            fields.push(field.trim_matches('"'));
            start = i + 1;
        }
    }
    // Last field
    if start <= line.len() {
        fields.push(line[start..].trim_matches('"'));
    }

    fields
}

fn parse_opt_f64(s: &str) -> Option<f64> {
    if s.is_empty() { None } else { s.parse().ok() }
}

/// Find the most "meaningful" keyword from ingredient words.
///
/// Skips generic modifiers and returns the first noun-like word.
fn find_primary_keyword<'a>(words: &[&'a str]) -> &'a str {
    const SKIP_WORDS: &[&str] = &[
        "fresh", "dried", "ground", "whole", "raw", "cooked", "organic", "large", "small",
        "medium", "extra", "virgin", "light", "heavy", "all", "purpose", "unsalted", "salted",
        "boneless", "skinless",
    ];

    for word in words {
        if !SKIP_WORDS.contains(word) && word.len() > 2 {
            return word;
        }
    }
    words.first().copied().unwrap_or("")
}

/// Score word overlap between ingredient and USDA description.
///
/// Returns a score between 0.0 and 1.0.
fn word_overlap_score(ingredient_words: &[&str], usda_words: &[&str]) -> f64 {
    if ingredient_words.is_empty() || usda_words.is_empty() {
        return 0.0;
    }

    // Count how many ingredient words appear in USDA description
    let matched = ingredient_words
        .iter()
        .filter(|w| usda_words.contains(w))
        .count();

    // Score relative to ingredient word count (we care that all ingredient
    // words match, not that all USDA words match — USDA descriptions are verbose)
    matched as f64 / ingredient_words.len() as f64
}

/// Parse a servings string like "4", "4-6", "serves 4" into a number.
fn parse_servings(s: &str) -> Option<u32> {
    let s = s
        .to_lowercase()
        .replace("serves", "")
        .replace("servings", "")
        .replace("serving", "")
        .trim()
        .to_string();

    // Handle ranges — take the first number
    let first_part = s.split(['-', '–', ' ']).next()?;
    first_part.trim().parse().ok()
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // --- CSV parsing ---

    #[test]
    fn parse_csv_simple() {
        let line = r#"167590,"Andrea's, Gluten Free Soft Dinner Roll",Baked Products,sr_legacy,257.0,5.65,8.2,40.24,2.9,2.98,544.0"#;
        let record = parse_csv_line(line).unwrap();
        assert_eq!(record.fdc_id, 167590);
        assert_eq!(record.description, "Andrea's, Gluten Free Soft Dinner Roll");
        assert_eq!(record.category, "Baked Products");
        assert!((record.kcal - 257.0).abs() < 0.01);
    }

    #[test]
    fn parse_csv_with_empty_fields() {
        let line = "123,Foo,Bar,sr_legacy,100.0,1.0,2.0,3.0,,,";
        let record = parse_csv_line(line).unwrap();
        assert!(record.fiber_g.is_none());
        assert!(record.sugar_g.is_none());
        assert!(record.sodium_mg.is_none());
    }

    // --- Unit conversions ---

    #[test]
    fn mass_units() {
        assert!((quantity_to_grams(1.0, "g", "flour").unwrap() - 1.0).abs() < 0.01);
        assert!((quantity_to_grams(1.0, "kg", "flour").unwrap() - 1000.0).abs() < 0.01);
        assert!((quantity_to_grams(1.0, "oz", "flour").unwrap() - 28.3495).abs() < 0.1);
        assert!((quantity_to_grams(1.0, "lb", "flour").unwrap() - 453.592).abs() < 0.1);
    }

    #[test]
    fn count_based_egg() {
        let g = quantity_to_grams(2.0, "", "eggs").unwrap();
        assert!((g - 100.0).abs() < 0.01);
    }

    #[test]
    fn cup_flour() {
        let g = quantity_to_grams(1.0, "cup", "all-purpose flour").unwrap();
        assert!((g - 125.0).abs() < 0.01);
    }

    #[test]
    fn cup_liquid() {
        let g = quantity_to_grams(1.0, "cup", "milk").unwrap();
        assert!((g - 237.0).abs() < 0.01);
    }

    #[test]
    fn ambiguous_unit_returns_none() {
        assert!(quantity_to_grams(1.0, "bunch", "parsley").is_none());
        assert!(quantity_to_grams(1.0, "pinch", "salt").is_none());
    }

    #[test]
    fn tsp_salt() {
        let g = quantity_to_grams(1.0, "tsp", "salt").unwrap();
        assert!((g - 6.0).abs() < 0.01);
    }

    #[test]
    fn tbsp_oil() {
        let g = quantity_to_grams(1.0, "tbsp", "olive oil").unwrap();
        assert!((g - 15.0).abs() < 0.01);
    }

    #[test]
    fn cloves_garlic() {
        let g = quantity_to_grams(3.0, "cloves", "garlic").unwrap();
        assert!((g - 9.0).abs() < 0.01);
    }

    // --- Servings parsing ---

    #[test]
    fn parse_servings_simple() {
        assert_eq!(parse_servings("4"), Some(4));
        assert_eq!(parse_servings("6"), Some(6));
    }

    #[test]
    fn parse_servings_range() {
        assert_eq!(parse_servings("4-6"), Some(4));
        assert_eq!(parse_servings("4–6"), Some(4));
    }

    #[test]
    fn parse_servings_text() {
        assert_eq!(parse_servings("serves 4"), Some(4));
        assert_eq!(parse_servings("Serves 6"), Some(6));
    }

    #[test]
    fn parse_servings_invalid() {
        assert_eq!(parse_servings("a few"), None);
        assert_eq!(parse_servings(""), None);
    }

    // --- Word overlap scoring ---

    #[test]
    fn word_overlap_identical() {
        let score = word_overlap_score(&["chicken", "thigh"], &["chicken", "thigh"]);
        assert!((score - 1.0).abs() < 0.01);
    }

    #[test]
    fn word_overlap_partial() {
        let score = word_overlap_score(
            &["chicken"],
            &["chicken", "broilers", "fryers", "thigh", "meat"],
        );
        assert!((score - 1.0).abs() < 0.01); // "chicken" found in USDA
    }

    #[test]
    fn word_overlap_none() {
        let score = word_overlap_score(&["tofu"], &["chicken", "thigh"]);
        assert!((score - 0.0).abs() < 0.01);
    }

    // --- Primary keyword ---

    #[test]
    fn primary_keyword_skips_modifiers() {
        assert_eq!(
            find_primary_keyword(&["fresh", "chicken", "breast"]),
            "chicken"
        );
        assert_eq!(find_primary_keyword(&["large", "egg"]), "egg");
        assert_eq!(find_primary_keyword(&["all", "purpose", "flour"]), "flour");
    }

    // --- Seed and match (integration) ---

    #[test]
    fn seed_and_count() {
        let db = FondDb::open_memory().unwrap();
        let repo = NutritionRepository::new(&db);
        repo.seed_nutrition_facts().unwrap();

        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM nutrition_facts", [], |r| r.get(0))
            .unwrap();
        assert!(count > 7000, "expected >7000 rows, got {count}");
    }

    #[test]
    fn seed_is_idempotent() {
        let db = FondDb::open_memory().unwrap();
        let repo = NutritionRepository::new(&db);
        repo.seed_nutrition_facts().unwrap();
        repo.seed_nutrition_facts().unwrap(); // should not error

        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM nutrition_facts", [], |r| r.get(0))
            .unwrap();
        assert!(count > 7000);
    }

    #[test]
    fn find_match_exact() {
        let db = FondDb::open_memory().unwrap();
        let repo = NutritionRepository::new(&db);
        repo.seed_nutrition_facts().unwrap();

        // "Salt, table" is a common USDA entry
        let result = repo.find_best_match("salt").unwrap();
        assert!(result.is_some(), "should match 'salt'");
        let (row, _confidence) = result.unwrap();
        let desc_lower = row.description.to_lowercase();
        assert!(desc_lower.contains("salt"), "matched: {}", row.description);
    }

    #[test]
    fn find_match_olive_oil() {
        let db = FondDb::open_memory().unwrap();
        let repo = NutritionRepository::new(&db);
        repo.seed_nutrition_facts().unwrap();

        let result = repo.find_best_match("olive oil").unwrap();
        assert!(result.is_some(), "should match 'olive oil'");
        let (row, _) = result.unwrap();
        let desc_lower = row.description.to_lowercase();
        assert!(desc_lower.contains("olive") && desc_lower.contains("oil"));
    }

    #[test]
    fn find_match_no_match() {
        let db = FondDb::open_memory().unwrap();
        let repo = NutritionRepository::new(&db);
        repo.seed_nutrition_facts().unwrap();

        let result = repo.find_best_match("xyzzy impossible food").unwrap();
        assert!(result.is_none());
    }

    // --- Rounding ---

    #[test]
    fn rounded_values() {
        let v = NutritionValues {
            kcal: 423.7,
            protein_g: 12.4,
            fat_g: 8.6,
            carb_g: 55.2,
            fiber_g: Some(3.7),
            sugar_g: Some(9.3),
            sodium_mg: Some(847.0),
        };
        let r = v.rounded();
        assert!((r.kcal - 420.0).abs() < 0.01);
        assert!((r.protein_g - 12.0).abs() < 0.01);
        assert!((r.sodium_mg.unwrap() - 850.0).abs() < 0.01);
    }
}
