use std::collections::BTreeMap;

use rusqlite::params;
use serde::Serialize;

use crate::db::FondDb;
use crate::error::StoreError;
use crate::pantry::{normalize_for_matching, phrase_matches, to_words};

// ═══════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════

/// A single item on a consolidated grocery list (from a meal plan).
///
/// Separate type from `GroceryItem` to preserve backward compatibility.
/// The key difference is `from_recipes` (Vec) instead of `from_recipe` (String).
#[derive(Debug, Clone, Serialize)]
pub struct ConsolidatedGroceryItem {
    pub name: String,
    pub quantity: Option<String>,
    pub unit: Option<String>,
    pub note: Option<String>,
    pub category: String,
    pub from_recipes: Vec<String>,
    pub optional: bool,
    pub pantry_covered: bool,
    pub matched_pantry_item: Option<String>,
}

/// A consolidated grocery list from a meal plan.
#[derive(Debug, Clone, Serialize)]
pub struct ConsolidatedGroceryList {
    pub plan_name: String,
    pub recipe_count: usize,
    pub recipe_slugs: Vec<String>,
    pub total_ingredients: usize,
    pub consolidated_items: usize,
    pub pantry_covered_count: usize,
    pub items_to_buy: usize,
    pub items: Vec<ConsolidatedGroceryItem>,
    pub categories: Vec<String>,
}

/// Raw ingredient with recipe source tracking (for multi-recipe aggregation).
struct SourcedIngredient {
    name: String,
    quantity: String,
    unit: String,
    note: String,
    optional: bool,
    from_recipe: String,
}

/// A single item on the grocery list.
#[derive(Debug, Clone, Serialize)]
pub struct GroceryItem {
    pub name: String,
    pub quantity: Option<String>,
    pub unit: Option<String>,
    pub note: Option<String>,
    pub category: String,
    pub from_recipe: String,
    pub optional: bool,
    pub pantry_covered: bool,
    pub matched_pantry_item: Option<String>,
}

/// The complete grocery list generated from a recipe.
#[derive(Debug, Clone, Serialize)]
pub struct GroceryList {
    pub recipe_slug: String,
    pub recipe_title: String,
    pub total_recipe_ingredients: usize,
    pub pantry_covered_count: usize,
    pub items_to_buy: usize,
    pub items: Vec<GroceryItem>,
    pub categories: Vec<String>,
}

/// Ordered list of grocery aisle categories (stable output contract).
const CATEGORY_ORDER: &[&str] = &[
    "Produce",
    "Meat & Seafood",
    "Dairy & Eggs",
    "Bakery & Bread",
    "Grains & Pasta",
    "Canned & Jarred",
    "Oils & Vinegars",
    "Spices & Seasonings",
    "Condiments & Sauces",
    "Baking",
    "Frozen",
    "Beverages",
    "Other",
];

// ═══════════════════════════════════════════════════════════════════
// Ingredient categorizer (keyword-based fallback)
// ═══════════════════════════════════════════════════════════════════

/// Categorize an ingredient into a grocery aisle/section.
///
/// This is a keyword-based fallback. When a canonical ingredient
/// database is available, it should be checked first and this
/// function used only when no canonical match exists.
fn categorize_ingredient(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();

    // Helper: check if any word in the ingredient matches a keyword
    let has_word = |kw: &str| words.contains(&kw);
    let contains = |kw: &str| lower.contains(kw);

    // ── Multi-word / compound matches first (most specific) ──

    // Canned & Jarred (check before Meat to catch "chicken broth" etc.)
    if contains("canned")
        || contains("can of")
        || contains("diced tomatoes")
        || contains("tomato paste")
        || contains("tomato sauce")
        || contains("coconut milk")
        || contains("broth")
        || contains("stock")
        || has_word("olives")
        || has_word("capers")
        || contains("chickpeas")
        || contains("lentils")
        || contains("black beans")
        || contains("kidney beans")
    {
        return "Canned & Jarred";
    }

    // Condiments & Sauces (before Meat to catch "fish sauce" etc.)
    if contains("sauce")
        || contains("soy sauce")
        || contains("fish sauce")
        || contains("hot sauce")
        || contains("worcestershire")
        || has_word("ketchup")
        || has_word("mustard")
        || has_word("mayo")
        || has_word("mayonnaise")
        || has_word("sriracha")
        || has_word("salsa")
        || has_word("pesto")
        || contains("miso")
        || contains("tahini")
    {
        return "Condiments & Sauces";
    }

    // Baking (before Spices to catch "vanilla extract" etc.)
    if has_word("flour")
        || has_word("sugar")
        || contains("baking soda")
        || contains("baking powder")
        || has_word("yeast")
        || contains("vanilla")
        || has_word("cocoa")
        || has_word("chocolate")
        || contains("cornstarch")
        || contains("corn starch")
        || has_word("honey")
        || contains("maple syrup")
        || has_word("molasses")
        || contains("powdered sugar")
        || contains("brown sugar")
        || contains("confectioner")
    {
        return "Baking";
    }

    // ── Single-word categories ──

    // Spices & Seasonings
    if has_word("salt")
        || has_word("paprika")
        || has_word("cumin")
        || has_word("cinnamon")
        || has_word("oregano")
        || has_word("thyme")
        || has_word("rosemary")
        || has_word("basil")
        || has_word("parsley")
        || has_word("cilantro")
        || has_word("dill")
        || has_word("sage")
        || has_word("turmeric")
        || has_word("coriander")
        || has_word("cardamom")
        || has_word("nutmeg")
        || has_word("cloves")
        || has_word("cayenne")
        || has_word("chili")
        || has_word("ginger")
        || contains("spice")
        || contains("seasoning")
        || contains("herb")
        || has_word("bay")
        || has_word("saffron")
        || has_word("fennel")
        || has_word("allspice")
        || has_word("anise")
        || has_word("tarragon")
        || has_word("marjoram")
        || (has_word("pepper") && !has_word("bell"))
    {
        return "Spices & Seasonings";
    }

    // Meat & Seafood
    if has_word("chicken")
        || has_word("beef")
        || has_word("pork")
        || has_word("lamb")
        || has_word("turkey")
        || has_word("duck")
        || has_word("bacon")
        || has_word("sausage")
        || has_word("ham")
        || has_word("steak")
        || contains("ground meat")
        || has_word("shrimp")
        || has_word("salmon")
        || has_word("tuna")
        || has_word("cod")
        || has_word("tilapia")
        || has_word("crab")
        || has_word("lobster")
        || has_word("scallops")
        || has_word("mussels")
        || has_word("clams")
        || has_word("anchovies")
        || has_word("prosciutto")
        || has_word("pancetta")
    {
        return "Meat & Seafood";
    }

    // Dairy & Eggs
    if has_word("milk")
        || has_word("cream")
        || has_word("butter")
        || has_word("cheese")
        || has_word("yogurt")
        || has_word("egg")
        || has_word("eggs")
        || contains("sour cream")
        || contains("cream cheese")
        || contains("half-and-half")
        || has_word("ghee")
        || has_word("ricotta")
        || has_word("mozzarella")
        || has_word("parmesan")
        || has_word("cheddar")
        || has_word("feta")
        || has_word("gouda")
        || has_word("brie")
        || has_word("mascarpone")
    {
        return "Dairy & Eggs";
    }

    // Produce (vegetables, fruits, fresh herbs are caught by spices above)
    if has_word("onion")
        || has_word("onions")
        || has_word("garlic")
        || has_word("tomato")
        || has_word("tomatoes")
        || has_word("potato")
        || has_word("potatoes")
        || has_word("carrot")
        || has_word("carrots")
        || has_word("celery")
        || has_word("bell")
        || has_word("pepper") // already caught by spice, but listed for clarity
        || has_word("broccoli")
        || has_word("spinach")
        || has_word("kale")
        || has_word("lettuce")
        || has_word("cabbage")
        || has_word("zucchini")
        || has_word("squash")
        || has_word("mushroom")
        || has_word("mushrooms")
        || has_word("avocado")
        || has_word("cucumber")
        || has_word("corn")
        || has_word("peas")
        || has_word("beans") // fresh beans
        || has_word("asparagus")
        || has_word("eggplant")
        || has_word("jalapeño")
        || has_word("jalapeno")
        || has_word("scallion")
        || has_word("scallions")
        || has_word("shallot")
        || has_word("shallots")
        || has_word("leek")
        || has_word("leeks")
        || has_word("lemon")
        || has_word("lime")
        || has_word("orange")
        || has_word("apple")
        || has_word("banana")
        || has_word("berries")
        || has_word("strawberries")
        || has_word("blueberries")
        || has_word("raspberries")
        || has_word("mango")
        || has_word("pineapple")
        || has_word("grapes")
        || has_word("pear")
        || has_word("peach")
        || has_word("radish")
        || has_word("beet")
        || has_word("beets")
        || has_word("turnip")
        || has_word("sweet")
    {
        return "Produce";
    }

    // Grains & Pasta
    if has_word("rice")
        || has_word("pasta")
        || has_word("spaghetti")
        || has_word("penne")
        || has_word("linguine")
        || has_word("fettuccine")
        || has_word("noodles")
        || has_word("quinoa")
        || has_word("couscous")
        || has_word("barley")
        || has_word("oats")
        || has_word("cereal")
        || has_word("tortillas")
        || has_word("tortilla")
    {
        return "Grains & Pasta";
    }

    // Bakery & Bread
    if has_word("bread")
        || has_word("baguette")
        || has_word("rolls")
        || has_word("pita")
        || has_word("naan")
        || has_word("croissant")
        || has_word("buns")
        || contains("pie crust")
        || contains("puff pastry")
    {
        return "Bakery & Bread";
    }

    // Oils & Vinegars
    if contains("oil")
        || contains("vinegar")
        || contains("olive oil")
        || contains("sesame oil")
        || contains("vegetable oil")
        || contains("canola oil")
        || contains("coconut oil")
    {
        return "Oils & Vinegars";
    }

    // Frozen
    if contains("frozen") {
        return "Frozen";
    }

    // Beverages
    if has_word("wine")
        || has_word("beer")
        || has_word("juice")
        || has_word("coffee")
        || has_word("tea")
    {
        return "Beverages";
    }

    "Other"
}

// ═══════════════════════════════════════════════════════════════════
// Quantity parsing and aggregation
// ═══════════════════════════════════════════════════════════════════

/// Parse a quantity string into a numeric value.
///
/// Handles integers, decimals, ASCII fractions (1/2), mixed fractions
/// (1 1/2), and common Unicode fractions (½, ¼, ¾, ⅓, ⅔, ⅛).
pub(crate) fn parse_quantity(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Try simple numeric parse first
    if let Ok(n) = s.parse::<f64>() {
        return Some(n);
    }

    // Try Unicode fraction alone
    if let Some(v) = unicode_fraction_value(s) {
        return Some(v);
    }

    // Try ASCII fraction (e.g. "1/2", "3/4")
    if let Some(v) = parse_ascii_fraction(s) {
        return Some(v);
    }

    // Try mixed number: "1 1/2" or "1 ½"
    let parts: Vec<&str> = s.splitn(2, ' ').collect();
    if parts.len() == 2
        && let Ok(whole) = parts[0].parse::<f64>()
    {
        if let Some(frac) = parse_ascii_fraction(parts[1]) {
            return Some(whole + frac);
        }
        if let Some(frac) = unicode_fraction_value(parts[1]) {
            return Some(whole + frac);
        }
    }

    None
}

fn unicode_fraction_value(s: &str) -> Option<f64> {
    match s {
        "½" => Some(0.5),
        "¼" => Some(0.25),
        "¾" => Some(0.75),
        "⅓" => Some(1.0 / 3.0),
        "⅔" => Some(2.0 / 3.0),
        "⅛" => Some(0.125),
        "⅜" => Some(0.375),
        "⅝" => Some(0.625),
        "⅞" => Some(0.875),
        _ => None,
    }
}

fn parse_ascii_fraction(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.splitn(2, '/').collect();
    if parts.len() == 2 {
        let num = parts[0].trim().parse::<f64>().ok()?;
        let den = parts[1].trim().parse::<f64>().ok()?;
        if den != 0.0 {
            return Some(num / den);
        }
    }
    None
}

/// Format a numeric quantity back to a clean string.
///
/// Prefers whole numbers and common fractions over decimals.
fn format_quantity(value: f64) -> String {
    // Check if it's a whole number
    if (value - value.round()).abs() < 0.001 && value >= 0.0 {
        return format!("{}", value.round() as i64);
    }

    // Check common fractions
    let whole = value.floor() as i64;
    let frac = value - whole as f64;

    let frac_str = if (frac - 0.5).abs() < 0.01 {
        Some("1/2")
    } else if (frac - 0.25).abs() < 0.01 {
        Some("1/4")
    } else if (frac - 0.75).abs() < 0.01 {
        Some("3/4")
    } else if (frac - 1.0 / 3.0).abs() < 0.02 {
        Some("1/3")
    } else if (frac - 2.0 / 3.0).abs() < 0.02 {
        Some("2/3")
    } else {
        None
    };

    if let Some(f) = frac_str {
        if whole > 0 {
            format!("{whole} {f}")
        } else {
            f.to_string()
        }
    } else {
        // Fall back to decimal, but trim trailing zeros
        let formatted = format!("{value:.2}");
        let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
        trimmed.to_string()
    }
}

// ═══════════════════════════════════════════════════════════════════
// Pantry match for grocery (stricter than coverage check)
// ═══════════════════════════════════════════════════════════════════

/// Find a pantry match for grocery subtraction (stricter than coverage).
///
/// Only matches when:
/// - Exact normalized match (pantry == ingredient)
/// - Pantry phrase appears in ingredient (e.g., "olive oil" in "extra-virgin olive oil")
///
/// Does NOT match ingredient-in-pantry direction to avoid false positives
/// like "chicken" matching "chicken stock" (which would incorrectly remove
/// chicken from the grocery list).
fn find_grocery_pantry_match(
    ingredient_name: &str,
    pantry_names: &[String],
) -> (bool, Option<String>) {
    let norm_ing = normalize_for_matching(ingredient_name);
    let ing_words = to_words(&norm_ing);

    if ing_words.is_empty() {
        return (false, None);
    }

    // Exact match first
    for pantry_name in pantry_names {
        let norm_pantry = normalize_for_matching(pantry_name);
        if norm_pantry == norm_ing {
            return (true, Some(pantry_name.clone()));
        }
    }

    // Pantry phrase contained in ingredient (pantry ⊂ ingredient only)
    for pantry_name in pantry_names {
        let norm_pantry = normalize_for_matching(pantry_name);
        let pantry_words = to_words(&norm_pantry);

        if pantry_words.is_empty() {
            continue;
        }

        if phrase_matches(&pantry_words, &ing_words) {
            return (true, Some(pantry_name.clone()));
        }
    }

    (false, None)
}

// ═══════════════════════════════════════════════════════════════════
// Repository
// ═══════════════════════════════════════════════════════════════════

/// Repository for grocery list operations.
pub struct GroceryRepository<'a> {
    db: &'a FondDb,
}

/// Raw ingredient row from the database before aggregation.
struct RawIngredient {
    name: String,
    quantity: String,
    unit: String,
    note: String,
    optional: bool,
}

impl<'a> GroceryRepository<'a> {
    pub fn new(db: &'a FondDb) -> Self {
        Self { db }
    }

    /// Generate a grocery list from a recipe, subtracting pantry items.
    ///
    /// If `include_pantry` is true, pantry-covered items are included
    /// (marked with `pantry_covered = true`) instead of being filtered out.
    pub fn from_recipe(
        &self,
        recipe_slug: &str,
        include_pantry: bool,
    ) -> Result<Option<GroceryList>, StoreError> {
        let conn = self.db.conn();

        // Get recipe info
        let recipe_row: Option<(i64, String)> = conn
            .query_row(
                "SELECT id, title FROM recipes WHERE slug = ?1",
                params![recipe_slug],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let Some((recipe_id, recipe_title)) = recipe_row else {
            return Ok(None);
        };

        // Get recipe ingredients
        let mut ing_stmt = conn.prepare(
            "SELECT name, quantity, unit, note, optional FROM recipe_ingredients
             WHERE recipe_id = ?1 ORDER BY sort_order",
        )?;
        let raw_ingredients: Vec<RawIngredient> = ing_stmt
            .query_map(params![recipe_id], |row| {
                Ok(RawIngredient {
                    name: row.get(0)?,
                    quantity: row.get(1)?,
                    unit: row.get(2)?,
                    note: row.get(3)?,
                    optional: row.get::<_, i32>(4)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let total_recipe_ingredients = raw_ingredients.len();

        // Get present pantry items
        let mut pantry_stmt = conn.prepare("SELECT name FROM pantry_items WHERE present = 1")?;
        let pantry_names: Vec<String> = pantry_stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        // Aggregate ingredients by normalized name + unit
        let aggregated = aggregate_ingredients(&raw_ingredients);

        // Build grocery items, checking pantry
        let mut items = Vec::new();
        let mut pantry_covered_count = 0;

        for agg in &aggregated {
            let (matched, matched_item) = find_grocery_pantry_match(&agg.name, &pantry_names);

            if matched {
                pantry_covered_count += 1;
            }

            if matched && !include_pantry {
                continue;
            }

            let category = categorize_ingredient(&agg.name).to_string();

            items.push(GroceryItem {
                name: agg.name.clone(),
                quantity: if agg.quantity.is_empty() {
                    None
                } else {
                    Some(agg.quantity.clone())
                },
                unit: if agg.unit.is_empty() {
                    None
                } else {
                    Some(agg.unit.clone())
                },
                note: if agg.note.is_empty() {
                    None
                } else {
                    Some(agg.note.clone())
                },
                category,
                from_recipe: recipe_slug.to_string(),
                optional: agg.optional,
                pantry_covered: matched,
                matched_pantry_item: matched_item,
            });
        }

        // Sort by category order, then by name within each category
        items.sort_by(|a, b| {
            let a_idx = category_index(&a.category);
            let b_idx = category_index(&b.category);
            a_idx.cmp(&b_idx).then(a.name.cmp(&b.name))
        });

        // Collect distinct categories in order
        let mut categories = Vec::new();
        let mut seen_cats = std::collections::HashSet::new();
        for item in &items {
            if seen_cats.insert(item.category.clone()) {
                categories.push(item.category.clone());
            }
        }

        let items_to_buy = items.iter().filter(|i| !i.pantry_covered).count();

        Ok(Some(GroceryList {
            recipe_slug: recipe_slug.to_string(),
            recipe_title,
            total_recipe_ingredients,
            pantry_covered_count,
            items_to_buy,
            items,
            categories,
        }))
    }

    /// Generate a consolidated grocery list from a meal plan.
    ///
    /// Aggregates ingredients across all recipes in the plan,
    /// combining duplicates by normalized name + unit.
    pub fn from_plan(
        &self,
        plan_name: &str,
        include_pantry: bool,
    ) -> Result<Option<ConsolidatedGroceryList>, StoreError> {
        let conn = self.db.conn();

        // Get plan ID
        let plan_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM meal_plans WHERE LOWER(name) = LOWER(?1)",
                params![plan_name],
                |row| row.get(0),
            )
            .ok();

        let Some(plan_id) = plan_id else {
            return Ok(None);
        };

        // Get distinct recipe slugs from the plan
        let mut slug_stmt = conn.prepare(
            "SELECT DISTINCT recipe_slug FROM meal_plan_entries WHERE meal_plan_id = ?1",
        )?;
        let recipe_slugs: Vec<String> = slug_stmt
            .query_map(params![plan_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        if recipe_slugs.is_empty() {
            return Ok(Some(ConsolidatedGroceryList {
                plan_name: plan_name.to_string(),
                recipe_count: 0,
                recipe_slugs: vec![],
                total_ingredients: 0,
                consolidated_items: 0,
                pantry_covered_count: 0,
                items_to_buy: 0,
                items: vec![],
                categories: vec![],
            }));
        }

        // Gather all ingredients from all plan recipes with source tracking
        let mut all_ingredients: Vec<SourcedIngredient> = Vec::new();
        let mut total_ingredients = 0;

        for slug in &recipe_slugs {
            let recipe_row: Option<i64> = conn
                .query_row(
                    "SELECT id FROM recipes WHERE slug = ?1",
                    params![slug],
                    |row| row.get(0),
                )
                .ok();

            let Some(recipe_id) = recipe_row else {
                continue; // Recipe may have been deleted
            };

            let mut ing_stmt = conn.prepare(
                "SELECT name, quantity, unit, note, optional FROM recipe_ingredients
                 WHERE recipe_id = ?1 ORDER BY sort_order",
            )?;

            let ingredients: Vec<SourcedIngredient> = ing_stmt
                .query_map(params![recipe_id], |row| {
                    Ok(SourcedIngredient {
                        name: row.get(0)?,
                        quantity: row.get(1)?,
                        unit: row.get(2)?,
                        note: row.get(3)?,
                        optional: row.get::<_, i32>(4)? != 0,
                        from_recipe: slug.clone(),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;

            total_ingredients += ingredients.len();
            all_ingredients.extend(ingredients);
        }

        // Get present pantry items
        let mut pantry_stmt = conn.prepare("SELECT name FROM pantry_items WHERE present = 1")?;
        let pantry_names: Vec<String> = pantry_stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        // Aggregate across all recipes
        let aggregated = aggregate_sourced_ingredients(&all_ingredients);

        // Build consolidated grocery items
        let mut items = Vec::new();
        let mut pantry_covered_count = 0;

        for agg in &aggregated {
            let (matched, matched_item) = find_grocery_pantry_match(&agg.name, &pantry_names);

            if matched {
                pantry_covered_count += 1;
            }

            if matched && !include_pantry {
                continue;
            }

            let category = categorize_ingredient(&agg.name).to_string();

            items.push(ConsolidatedGroceryItem {
                name: agg.name.clone(),
                quantity: if agg.quantity.is_empty() {
                    None
                } else {
                    Some(agg.quantity.clone())
                },
                unit: if agg.unit.is_empty() {
                    None
                } else {
                    Some(agg.unit.clone())
                },
                note: if agg.note.is_empty() {
                    None
                } else {
                    Some(agg.note.clone())
                },
                category,
                from_recipes: agg.from_recipes.clone(),
                optional: agg.optional,
                pantry_covered: matched,
                matched_pantry_item: matched_item,
            });
        }

        // Sort by category order, then by name
        items.sort_by(|a, b| {
            let a_idx = category_index(&a.category);
            let b_idx = category_index(&b.category);
            a_idx.cmp(&b_idx).then(a.name.cmp(&b.name))
        });

        // Collect distinct categories in order
        let mut categories = Vec::new();
        let mut seen_cats = std::collections::HashSet::new();
        for item in &items {
            if seen_cats.insert(item.category.clone()) {
                categories.push(item.category.clone());
            }
        }

        let items_to_buy = items.iter().filter(|i| !i.pantry_covered).count();

        Ok(Some(ConsolidatedGroceryList {
            plan_name: plan_name.to_string(),
            recipe_count: recipe_slugs.len(),
            recipe_slugs,
            total_ingredients,
            consolidated_items: items.len() + pantry_covered_count
                - items.iter().filter(|i| i.pantry_covered).count(),
            pantry_covered_count,
            items_to_buy,
            items,
            categories,
        }))
    }
}

/// Get the sort index for a category name.
fn category_index(cat: &str) -> usize {
    CATEGORY_ORDER
        .iter()
        .position(|c| *c == cat)
        .unwrap_or(CATEGORY_ORDER.len())
}

/// Aggregated ingredient after merging duplicates.
struct AggregatedIngredient {
    name: String,
    quantity: String,
    unit: String,
    note: String,
    optional: bool,
}

/// Aggregate ingredients that share the same normalized name and unit.
///
/// When the same ingredient appears multiple times with compatible units,
/// quantities are summed. Different units are kept as separate entries.
fn aggregate_ingredients(ingredients: &[RawIngredient]) -> Vec<AggregatedIngredient> {
    // Key: (normalized_name, unit_lower)
    let mut groups: BTreeMap<(String, String), Vec<&RawIngredient>> = BTreeMap::new();

    for ing in ingredients {
        let norm_name = normalize_for_matching(&ing.name);
        let unit_lower = ing.unit.trim().to_lowercase();
        groups.entry((norm_name, unit_lower)).or_default().push(ing);
    }

    let mut result = Vec::new();

    for ((_norm_name, _unit), group) in &groups {
        // Use the first ingredient's display name (preserves original casing)
        let display_name = &group[0].name;
        let display_unit = &group[0].unit;

        // Try to aggregate quantities
        let quantities: Vec<(&str, Option<f64>)> = group
            .iter()
            .map(|i| (i.quantity.as_str(), parse_quantity(&i.quantity)))
            .collect();

        let all_numeric = quantities
            .iter()
            .all(|(raw, parsed)| raw.is_empty() || parsed.is_some());

        let quantity = if group.len() == 1 {
            group[0].quantity.clone()
        } else if all_numeric {
            let total: f64 = quantities.iter().filter_map(|(_, parsed)| *parsed).sum();
            if total > 0.0 {
                format_quantity(total)
            } else {
                String::new()
            }
        } else {
            // Can't aggregate — join non-empty quantities
            let parts: Vec<&str> = group
                .iter()
                .map(|i| i.quantity.as_str())
                .filter(|q| !q.is_empty())
                .collect();
            parts.join(" + ")
        };

        // Merge notes
        let notes: Vec<&str> = group
            .iter()
            .map(|i| i.note.as_str())
            .filter(|n| !n.is_empty())
            .collect();
        let note = notes.join("; ");

        // Optional only if ALL entries for this ingredient are optional
        let optional = group.iter().all(|i| i.optional);

        result.push(AggregatedIngredient {
            name: display_name.clone(),
            quantity,
            unit: display_unit.clone(),
            note,
            optional,
        });
    }

    result
}

/// Aggregated ingredient with multi-recipe source tracking.
struct AggregatedSourcedIngredient {
    name: String,
    quantity: String,
    unit: String,
    note: String,
    optional: bool,
    from_recipes: Vec<String>,
}

/// Aggregate ingredients from multiple recipes, tracking source recipes.
fn aggregate_sourced_ingredients(
    ingredients: &[SourcedIngredient],
) -> Vec<AggregatedSourcedIngredient> {
    // Key: (normalized_name, unit_lower)
    let mut groups: BTreeMap<(String, String), Vec<&SourcedIngredient>> = BTreeMap::new();

    for ing in ingredients {
        let norm_name = normalize_for_matching(&ing.name);
        let unit_lower = ing.unit.trim().to_lowercase();
        groups.entry((norm_name, unit_lower)).or_default().push(ing);
    }

    let mut result = Vec::new();

    for ((_norm_name, _unit), group) in &groups {
        let display_name = &group[0].name;
        let display_unit = &group[0].unit;

        // Try to aggregate quantities
        let quantities: Vec<(&str, Option<f64>)> = group
            .iter()
            .map(|i| (i.quantity.as_str(), parse_quantity(&i.quantity)))
            .collect();

        let all_numeric = quantities
            .iter()
            .all(|(raw, parsed)| raw.is_empty() || parsed.is_some());

        let quantity = if group.len() == 1 {
            group[0].quantity.clone()
        } else if all_numeric {
            let total: f64 = quantities.iter().filter_map(|(_, parsed)| *parsed).sum();
            if total > 0.0 {
                format_quantity(total)
            } else {
                String::new()
            }
        } else {
            let parts: Vec<&str> = group
                .iter()
                .map(|i| i.quantity.as_str())
                .filter(|q| !q.is_empty())
                .collect();
            parts.join(" + ")
        };

        // Merge notes
        let notes: Vec<&str> = group
            .iter()
            .map(|i| i.note.as_str())
            .filter(|n| !n.is_empty())
            .collect();
        let note = notes.join("; ");

        let optional = group.iter().all(|i| i.optional);

        // Collect distinct source recipes
        let mut from_recipes: Vec<String> = group.iter().map(|i| i.from_recipe.clone()).collect();
        from_recipes.sort();
        from_recipes.dedup();

        result.push(AggregatedSourcedIngredient {
            name: display_name.clone(),
            quantity,
            unit: display_unit.clone(),
            note,
            optional,
            from_recipes,
        });
    }

    result
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // --- categorize_ingredient ---

    #[test]
    fn categorize_produce() {
        assert_eq!(categorize_ingredient("onion"), "Produce");
        assert_eq!(categorize_ingredient("garlic"), "Produce");
        assert_eq!(categorize_ingredient("lemon"), "Produce");
        assert_eq!(categorize_ingredient("carrots"), "Produce");
        assert_eq!(categorize_ingredient("bell pepper"), "Produce");
    }

    #[test]
    fn categorize_meat() {
        assert_eq!(categorize_ingredient("chicken thighs"), "Meat & Seafood");
        assert_eq!(categorize_ingredient("ground beef"), "Meat & Seafood");
        assert_eq!(categorize_ingredient("salmon fillet"), "Meat & Seafood");
        assert_eq!(categorize_ingredient("bacon"), "Meat & Seafood");
    }

    #[test]
    fn categorize_dairy() {
        assert_eq!(categorize_ingredient("butter"), "Dairy & Eggs");
        assert_eq!(categorize_ingredient("eggs"), "Dairy & Eggs");
        assert_eq!(categorize_ingredient("parmesan cheese"), "Dairy & Eggs");
        assert_eq!(categorize_ingredient("heavy cream"), "Dairy & Eggs");
    }

    #[test]
    fn categorize_spices() {
        assert_eq!(categorize_ingredient("salt"), "Spices & Seasonings");
        assert_eq!(categorize_ingredient("black pepper"), "Spices & Seasonings");
        assert_eq!(categorize_ingredient("cumin"), "Spices & Seasonings");
        assert_eq!(
            categorize_ingredient("dried oregano"),
            "Spices & Seasonings"
        );
    }

    #[test]
    fn categorize_grains() {
        assert_eq!(categorize_ingredient("rice"), "Grains & Pasta");
        assert_eq!(categorize_ingredient("spaghetti"), "Grains & Pasta");
        assert_eq!(categorize_ingredient("quinoa"), "Grains & Pasta");
    }

    #[test]
    fn categorize_oils() {
        assert_eq!(categorize_ingredient("olive oil"), "Oils & Vinegars");
        assert_eq!(categorize_ingredient("balsamic vinegar"), "Oils & Vinegars");
    }

    #[test]
    fn categorize_baking() {
        assert_eq!(categorize_ingredient("flour"), "Baking");
        assert_eq!(categorize_ingredient("sugar"), "Baking");
        assert_eq!(categorize_ingredient("baking powder"), "Baking");
        assert_eq!(categorize_ingredient("vanilla extract"), "Baking");
    }

    #[test]
    fn categorize_condiments() {
        assert_eq!(categorize_ingredient("soy sauce"), "Condiments & Sauces");
        assert_eq!(categorize_ingredient("ketchup"), "Condiments & Sauces");
        assert_eq!(categorize_ingredient("fish sauce"), "Condiments & Sauces");
    }

    #[test]
    fn categorize_canned() {
        assert_eq!(categorize_ingredient("chicken broth"), "Canned & Jarred");
        assert_eq!(categorize_ingredient("tomato paste"), "Canned & Jarred");
        assert_eq!(categorize_ingredient("coconut milk"), "Canned & Jarred");
        assert_eq!(categorize_ingredient("canned chickpeas"), "Canned & Jarred");
    }

    #[test]
    fn categorize_unknown() {
        assert_eq!(categorize_ingredient("wonton wrappers"), "Other");
        assert_eq!(categorize_ingredient("panko"), "Other");
    }

    // --- parse_quantity ---

    #[test]
    fn parse_integer() {
        assert_eq!(parse_quantity("2"), Some(2.0));
    }

    #[test]
    fn parse_decimal() {
        assert_eq!(parse_quantity("1.5"), Some(1.5));
    }

    #[test]
    fn parse_unicode_fraction() {
        assert_eq!(parse_quantity("½"), Some(0.5));
        assert_eq!(parse_quantity("¼"), Some(0.25));
        assert_eq!(parse_quantity("¾"), Some(0.75));
    }

    #[test]
    fn parse_ascii_fraction_test() {
        assert_eq!(parse_quantity("1/2"), Some(0.5));
        assert_eq!(parse_quantity("3/4"), Some(0.75));
    }

    #[test]
    fn parse_mixed_fraction() {
        assert_eq!(parse_quantity("1 1/2"), Some(1.5));
        assert_eq!(parse_quantity("2 1/4"), Some(2.25));
    }

    #[test]
    fn parse_mixed_unicode_fraction() {
        assert_eq!(parse_quantity("1 ½"), Some(1.5));
    }

    #[test]
    fn parse_empty() {
        assert_eq!(parse_quantity(""), None);
    }

    #[test]
    fn parse_unparseable() {
        assert_eq!(parse_quantity("a pinch"), None);
    }

    // --- format_quantity ---

    #[test]
    fn format_whole_number() {
        assert_eq!(format_quantity(3.0), "3");
    }

    #[test]
    fn format_common_fraction() {
        assert_eq!(format_quantity(0.5), "1/2");
        assert_eq!(format_quantity(0.25), "1/4");
        assert_eq!(format_quantity(0.75), "3/4");
    }

    #[test]
    fn format_mixed_number() {
        assert_eq!(format_quantity(1.5), "1 1/2");
        assert_eq!(format_quantity(2.25), "2 1/4");
    }

    #[test]
    fn format_uncommon_decimal() {
        assert_eq!(format_quantity(1.7), "1.7");
    }

    // --- aggregate_ingredients ---

    #[test]
    fn aggregate_same_ingredient_same_unit() {
        let ingredients = vec![
            RawIngredient {
                name: "flour".to_string(),
                quantity: "1".to_string(),
                unit: "cup".to_string(),
                note: "".to_string(),
                optional: false,
            },
            RawIngredient {
                name: "flour".to_string(),
                quantity: "1/2".to_string(),
                unit: "cup".to_string(),
                note: "".to_string(),
                optional: false,
            },
        ];

        let result = aggregate_ingredients(&ingredients);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "flour");
        assert_eq!(result[0].quantity, "1 1/2");
        assert_eq!(result[0].unit, "cup");
    }

    #[test]
    fn aggregate_same_ingredient_different_unit() {
        let ingredients = vec![
            RawIngredient {
                name: "butter".to_string(),
                quantity: "2".to_string(),
                unit: "tbsp".to_string(),
                note: "".to_string(),
                optional: false,
            },
            RawIngredient {
                name: "butter".to_string(),
                quantity: "1".to_string(),
                unit: "cup".to_string(),
                note: "".to_string(),
                optional: false,
            },
        ];

        let result = aggregate_ingredients(&ingredients);
        // Different units → kept separate
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn aggregate_no_duplicates() {
        let ingredients = vec![
            RawIngredient {
                name: "salt".to_string(),
                quantity: "1".to_string(),
                unit: "tsp".to_string(),
                note: "".to_string(),
                optional: false,
            },
            RawIngredient {
                name: "pepper".to_string(),
                quantity: "1/2".to_string(),
                unit: "tsp".to_string(),
                note: "".to_string(),
                optional: false,
            },
        ];

        let result = aggregate_ingredients(&ingredients);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn aggregate_merges_notes() {
        let ingredients = vec![
            RawIngredient {
                name: "onion".to_string(),
                quantity: "1".to_string(),
                unit: "".to_string(),
                note: "diced".to_string(),
                optional: false,
            },
            RawIngredient {
                name: "onion".to_string(),
                quantity: "1".to_string(),
                unit: "".to_string(),
                note: "for garnish".to_string(),
                optional: false,
            },
        ];

        let result = aggregate_ingredients(&ingredients);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].quantity, "2");
        assert_eq!(result[0].note, "diced; for garnish");
    }

    #[test]
    fn aggregate_optional_only_when_all_optional() {
        let ingredients = vec![
            RawIngredient {
                name: "cilantro".to_string(),
                quantity: "".to_string(),
                unit: "".to_string(),
                note: "".to_string(),
                optional: true,
            },
            RawIngredient {
                name: "cilantro".to_string(),
                quantity: "".to_string(),
                unit: "".to_string(),
                note: "".to_string(),
                optional: false,
            },
        ];

        let result = aggregate_ingredients(&ingredients);
        assert_eq!(result.len(), 1);
        assert!(
            !result[0].optional,
            "not optional when any entry is required"
        );
    }

    // --- find_grocery_pantry_match (stricter than coverage) ---

    #[test]
    fn grocery_match_exact() {
        let pantry = vec!["olive oil".to_string()];
        let (matched, item) = find_grocery_pantry_match("olive oil", &pantry);
        assert!(matched);
        assert_eq!(item.unwrap(), "olive oil");
    }

    #[test]
    fn grocery_match_pantry_in_ingredient() {
        // Pantry has "olive oil", recipe wants "extra-virgin olive oil" → match
        let pantry = vec!["olive oil".to_string()];
        let (matched, _) = find_grocery_pantry_match("extra-virgin olive oil", &pantry);
        assert!(matched);
    }

    #[test]
    fn grocery_no_match_ingredient_in_pantry() {
        // Pantry has "chicken stock", recipe wants "chicken" → should NOT match
        // (stricter than pantry coverage to avoid removing chicken from list)
        let pantry = vec!["chicken stock".to_string()];
        let (matched, _) = find_grocery_pantry_match("chicken", &pantry);
        assert!(!matched, "should not match ingredient-in-pantry direction");
    }

    #[test]
    fn grocery_no_match_tomato_vs_tomato_paste() {
        let pantry = vec!["tomato paste".to_string()];
        let (matched, _) = find_grocery_pantry_match("tomato", &pantry);
        assert!(!matched);
    }

    #[test]
    fn grocery_match_with_prep_modifiers() {
        let pantry = vec!["garlic".to_string()];
        let (matched, _) = find_grocery_pantry_match("garlic, minced", &pantry);
        assert!(matched);
    }

    // --- category_index ---

    #[test]
    fn category_order_is_stable() {
        assert!(category_index("Produce") < category_index("Meat & Seafood"));
        assert!(category_index("Dairy & Eggs") < category_index("Grains & Pasta"));
        assert!(category_index("Other") > category_index("Frozen"));
    }

    // --- aggregate_sourced_ingredients ---

    #[test]
    fn sourced_aggregate_same_ingredient_sums_quantity() {
        let ingredients = vec![
            SourcedIngredient {
                name: "flour".to_string(),
                quantity: "1".to_string(),
                unit: "cup".to_string(),
                note: "".to_string(),
                optional: false,
                from_recipe: "pancakes".to_string(),
            },
            SourcedIngredient {
                name: "flour".to_string(),
                quantity: "2".to_string(),
                unit: "cup".to_string(),
                note: "".to_string(),
                optional: false,
                from_recipe: "bread".to_string(),
            },
        ];

        let result = aggregate_sourced_ingredients(&ingredients);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "flour");
        assert_eq!(result[0].quantity, "3");
        assert_eq!(result[0].unit, "cup");
        assert_eq!(result[0].from_recipes, vec!["bread", "pancakes"]);
    }

    #[test]
    fn sourced_aggregate_tracks_multiple_recipes() {
        let ingredients = vec![
            SourcedIngredient {
                name: "salt".to_string(),
                quantity: "1".to_string(),
                unit: "tsp".to_string(),
                note: "".to_string(),
                optional: false,
                from_recipe: "adobo".to_string(),
            },
            SourcedIngredient {
                name: "salt".to_string(),
                quantity: "1/2".to_string(),
                unit: "tsp".to_string(),
                note: "".to_string(),
                optional: false,
                from_recipe: "pasta".to_string(),
            },
            SourcedIngredient {
                name: "salt".to_string(),
                quantity: "1".to_string(),
                unit: "tsp".to_string(),
                note: "".to_string(),
                optional: false,
                from_recipe: "soup".to_string(),
            },
        ];

        let result = aggregate_sourced_ingredients(&ingredients);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].from_recipes, vec!["adobo", "pasta", "soup"]);
        assert_eq!(result[0].quantity, "2 1/2");
    }

    #[test]
    fn sourced_aggregate_different_units_separate() {
        let ingredients = vec![
            SourcedIngredient {
                name: "butter".to_string(),
                quantity: "2".to_string(),
                unit: "tbsp".to_string(),
                note: "".to_string(),
                optional: false,
                from_recipe: "recipe-a".to_string(),
            },
            SourcedIngredient {
                name: "butter".to_string(),
                quantity: "1".to_string(),
                unit: "cup".to_string(),
                note: "".to_string(),
                optional: false,
                from_recipe: "recipe-b".to_string(),
            },
        ];

        let result = aggregate_sourced_ingredients(&ingredients);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn sourced_aggregate_deduplicates_same_recipe() {
        let ingredients = vec![
            SourcedIngredient {
                name: "garlic".to_string(),
                quantity: "3".to_string(),
                unit: "cloves".to_string(),
                note: "".to_string(),
                optional: false,
                from_recipe: "stew".to_string(),
            },
            SourcedIngredient {
                name: "garlic".to_string(),
                quantity: "2".to_string(),
                unit: "cloves".to_string(),
                note: "".to_string(),
                optional: false,
                from_recipe: "stew".to_string(),
            },
        ];

        let result = aggregate_sourced_ingredients(&ingredients);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].quantity, "5");
        // Same recipe should appear once (dedup)
        assert_eq!(result[0].from_recipes, vec!["stew"]);
    }

    #[test]
    fn sourced_aggregate_merges_notes_from_recipes() {
        let ingredients = vec![
            SourcedIngredient {
                name: "onion".to_string(),
                quantity: "1".to_string(),
                unit: "".to_string(),
                note: "diced".to_string(),
                optional: false,
                from_recipe: "soup".to_string(),
            },
            SourcedIngredient {
                name: "onion".to_string(),
                quantity: "1".to_string(),
                unit: "".to_string(),
                note: "sliced".to_string(),
                optional: false,
                from_recipe: "salad".to_string(),
            },
        ];

        let result = aggregate_sourced_ingredients(&ingredients);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].quantity, "2");
        assert_eq!(result[0].note, "diced; sliced");
    }

    #[test]
    fn sourced_aggregate_optional_when_all_optional() {
        let required = vec![
            SourcedIngredient {
                name: "cilantro".to_string(),
                quantity: "".to_string(),
                unit: "".to_string(),
                note: "".to_string(),
                optional: true,
                from_recipe: "a".to_string(),
            },
            SourcedIngredient {
                name: "cilantro".to_string(),
                quantity: "".to_string(),
                unit: "".to_string(),
                note: "".to_string(),
                optional: false,
                from_recipe: "b".to_string(),
            },
        ];

        let result = aggregate_sourced_ingredients(&required);
        assert!(!result[0].optional, "not optional if any entry is required");

        let all_opt = vec![
            SourcedIngredient {
                name: "cilantro".to_string(),
                quantity: "".to_string(),
                unit: "".to_string(),
                note: "".to_string(),
                optional: true,
                from_recipe: "a".to_string(),
            },
            SourcedIngredient {
                name: "cilantro".to_string(),
                quantity: "".to_string(),
                unit: "".to_string(),
                note: "".to_string(),
                optional: true,
                from_recipe: "b".to_string(),
            },
        ];

        let result = aggregate_sourced_ingredients(&all_opt);
        assert!(result[0].optional, "optional when all entries optional");
    }
}
