use rusqlite::params;
use serde::Serialize;

use crate::db::FondDb;
use crate::error::StoreError;

/// A pantry item record from the database.
#[derive(Debug, Clone, Serialize)]
pub struct PantryItem {
    pub id: i64,
    pub name: String,
    pub present: bool,
    pub quantity: Option<String>,
    pub unit: Option<String>,
    pub expiry: Option<String>,
    pub par_level: Option<String>,
}

/// Per-ingredient coverage detail for `pantry check`.
#[derive(Debug, Clone, Serialize)]
pub struct IngredientCoverage {
    pub ingredient: String,
    pub matched: bool,
    pub matched_pantry_item: Option<String>,
    pub optional: bool,
}

/// Coverage result for a recipe's pantry check.
#[derive(Debug, Clone, Serialize)]
pub struct PantryCoverage {
    pub recipe_slug: String,
    pub recipe_title: String,
    pub total_ingredients: usize,
    pub matched_count: usize,
    pub missing_count: usize,
    pub coverage_pct: f64,
    pub ingredients: Vec<IngredientCoverage>,
}

/// Repository for pantry persistence operations.
pub struct PantryRepository<'a> {
    db: &'a FondDb,
}

impl<'a> PantryRepository<'a> {
    pub fn new(db: &'a FondDb) -> Self {
        Self { db }
    }

    /// Add items to the pantry (mark as present).
    ///
    /// If an item already exists, it is re-marked as present without
    /// overwriting optional metadata (quantity, unit, expiry, par_level).
    pub fn add_items(&self, names: &[&str]) -> Result<Vec<String>, StoreError> {
        let conn = self.db.conn();
        let mut added = Vec::new();

        for name in names {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                continue;
            }

            conn.execute(
                "INSERT INTO pantry_items (name, present)
                 VALUES (?1, 1)
                 ON CONFLICT(name) DO UPDATE SET
                   present = 1,
                   updated_at = datetime('now')",
                params![trimmed],
            )?;

            added.push(trimmed.to_string());
        }

        Ok(added)
    }

    /// Remove items from the pantry (mark as absent, not delete).
    ///
    /// Preserves the row and any optional metadata for easy re-add.
    pub fn remove_items(&self, names: &[&str]) -> Result<Vec<String>, StoreError> {
        let conn = self.db.conn();
        let mut removed = Vec::new();

        for name in names {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                continue;
            }

            let rows = conn.execute(
                "UPDATE pantry_items SET present = 0, updated_at = datetime('now')
                 WHERE name = ?1 COLLATE NOCASE AND present = 1",
                params![trimmed],
            )?;

            if rows > 0 {
                removed.push(trimmed.to_string());
            }
        }

        Ok(removed)
    }

    /// List pantry items.
    ///
    /// If `show_all` is false, only present items are returned.
    pub fn list_items(&self, show_all: bool) -> Result<Vec<PantryItem>, StoreError> {
        let conn = self.db.conn();

        let sql = if show_all {
            "SELECT id, name, present, quantity, unit, expiry, par_level
             FROM pantry_items ORDER BY name COLLATE NOCASE"
        } else {
            "SELECT id, name, present, quantity, unit, expiry, par_level
             FROM pantry_items WHERE present = 1 ORDER BY name COLLATE NOCASE"
        };

        let mut stmt = conn.prepare(sql)?;
        let items = stmt
            .query_map([], |row| {
                Ok(PantryItem {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    present: row.get::<_, i32>(2)? != 0,
                    quantity: row.get(3)?,
                    unit: row.get(4)?,
                    expiry: row.get(5)?,
                    par_level: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(items)
    }

    /// Check pantry coverage for a recipe.
    ///
    /// Returns per-ingredient match status and overall coverage percentage.
    /// Uses bidirectional word-boundary matching for fuzzy ingredient matching.
    pub fn check_coverage(&self, recipe_slug: &str) -> Result<Option<PantryCoverage>, StoreError> {
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
            "SELECT name, optional FROM recipe_ingredients
             WHERE recipe_id = ?1 ORDER BY sort_order",
        )?;
        let recipe_ingredients: Vec<(String, bool)> = ing_stmt
            .query_map(params![recipe_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)? != 0))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Get present pantry items
        let mut pantry_stmt = conn.prepare("SELECT name FROM pantry_items WHERE present = 1")?;
        let pantry_names: Vec<String> = pantry_stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        // Check each ingredient against the pantry
        let mut ingredients = Vec::new();
        let mut matched_count = 0;

        for (ing_name, optional) in &recipe_ingredients {
            let (matched, matched_item) = find_pantry_match(ing_name, &pantry_names);
            if matched {
                matched_count += 1;
            }
            ingredients.push(IngredientCoverage {
                ingredient: ing_name.clone(),
                matched,
                matched_pantry_item: matched_item,
                optional: *optional,
            });
        }

        let total = recipe_ingredients.len();
        let coverage_pct = if total > 0 {
            (matched_count as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        Ok(Some(PantryCoverage {
            recipe_slug: recipe_slug.to_string(),
            recipe_title,
            total_ingredients: total,
            matched_count,
            missing_count: total - matched_count,
            coverage_pct,
            ingredients,
        }))
    }
}

/// Prep modifiers to strip from ingredient names before matching.
const PREP_MODIFIERS: &[&str] = &[
    "diced",
    "minced",
    "chopped",
    "sliced",
    "grated",
    "shredded",
    "crushed",
    "ground",
    "cubed",
    "halved",
    "quartered",
    "julienned",
    "peeled",
    "deveined",
    "trimmed",
    "pitted",
    "seeded",
    "cored",
    "melted",
    "softened",
    "frozen",
    "thawed",
    "dried",
    "fresh",
    "finely",
    "roughly",
    "thinly",
    "coarsely",
    "cut into strips",
    "cut into pieces",
    "cut into cubes",
    "to taste",
    "for garnish",
    "for serving",
    "large",
    "medium",
    "small",
    "whole",
];

/// Normalize an ingredient or pantry item name for fuzzy matching.
///
/// Lowercases, strips prep modifiers, collapses whitespace, trims.
pub(crate) fn normalize_for_matching(name: &str) -> String {
    let mut s = name.to_lowercase();

    // Remove common prep modifiers
    for modifier in PREP_MODIFIERS {
        // Replace modifier preceded/followed by word boundary or comma
        s = s.replace(modifier, " ");
    }

    // Remove punctuation that isn't useful for matching
    s = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect();

    // Collapse whitespace and trim
    let words: Vec<&str> = s.split_whitespace().collect();
    words.join(" ")
}

/// Split a normalized name into words.
pub(crate) fn to_words(s: &str) -> Vec<&str> {
    s.split_whitespace().collect()
}

/// Check if all words of `phrase` appear in `text` as a contiguous subsequence
/// (word-boundary aware phrase matching).
pub(crate) fn phrase_matches(phrase_words: &[&str], text_words: &[&str]) -> bool {
    if phrase_words.is_empty() {
        return false;
    }
    if phrase_words.len() > text_words.len() {
        return false;
    }

    'outer: for start in 0..=(text_words.len() - phrase_words.len()) {
        for (i, pw) in phrase_words.iter().enumerate() {
            if text_words[start + i] != *pw {
                continue 'outer;
            }
        }
        return true;
    }
    false
}

/// Find the best pantry match for a recipe ingredient.
///
/// Uses bidirectional word-boundary phrase matching:
/// - Pantry "olive oil" matches recipe "extra-virgin olive oil" (pantry ⊂ recipe)
/// - Pantry "chicken thighs" matches recipe "chicken" (recipe ⊂ pantry)
/// - Exact match always wins
///
/// Returns (matched, Option<matched_pantry_item_name>).
pub(crate) fn find_pantry_match(
    ingredient_name: &str,
    pantry_names: &[String],
) -> (bool, Option<String>) {
    let norm_ing = normalize_for_matching(ingredient_name);
    let ing_words = to_words(&norm_ing);

    if ing_words.is_empty() {
        return (false, None);
    }

    // Try exact match first
    for pantry_name in pantry_names {
        let norm_pantry = normalize_for_matching(pantry_name);
        if norm_pantry == norm_ing {
            return (true, Some(pantry_name.clone()));
        }
    }

    // Try phrase matching (bidirectional)
    for pantry_name in pantry_names {
        let norm_pantry = normalize_for_matching(pantry_name);
        let pantry_words = to_words(&norm_pantry);

        if pantry_words.is_empty() {
            continue;
        }

        // pantry phrase appears in ingredient (e.g., "olive oil" in "extra-virgin olive oil")
        if phrase_matches(&pantry_words, &ing_words) {
            return (true, Some(pantry_name.clone()));
        }

        // ingredient phrase appears in pantry (e.g., "chicken" in "chicken thighs")
        if phrase_matches(&ing_words, &pantry_words) {
            return (true, Some(pantry_name.clone()));
        }
    }

    (false, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- normalize_for_matching ---

    #[test]
    fn normalize_basic() {
        assert_eq!(normalize_for_matching("Olive Oil"), "olive oil");
    }

    #[test]
    fn normalize_strips_prep() {
        assert_eq!(normalize_for_matching("garlic, minced"), "garlic");
    }

    #[test]
    fn normalize_strips_multiple_prep() {
        assert_eq!(
            normalize_for_matching("chicken thighs, diced and trimmed"),
            "chicken thighs and"
        );
    }

    #[test]
    fn normalize_collapses_whitespace() {
        assert_eq!(
            normalize_for_matching("  extra   virgin   olive   oil  "),
            "extra virgin olive oil"
        );
    }

    // --- phrase_matches ---

    #[test]
    fn phrase_exact_match() {
        assert!(phrase_matches(&["olive", "oil"], &["olive", "oil"]));
    }

    #[test]
    fn phrase_subset_match() {
        assert!(phrase_matches(
            &["olive", "oil"],
            &["extra", "virgin", "olive", "oil"]
        ));
    }

    #[test]
    fn phrase_no_match() {
        assert!(!phrase_matches(&["olive", "oil"], &["coconut", "oil"]));
    }

    #[test]
    fn phrase_single_word() {
        assert!(phrase_matches(&["chicken"], &["chicken", "thighs"]));
    }

    #[test]
    fn phrase_empty() {
        assert!(!phrase_matches(&[], &["chicken"]));
    }

    // --- find_pantry_match ---

    #[test]
    fn match_exact() {
        let pantry = vec!["olive oil".to_string()];
        let (matched, item) = find_pantry_match("olive oil", &pantry);
        assert!(matched);
        assert_eq!(item.as_deref(), Some("olive oil"));
    }

    #[test]
    fn match_pantry_subset_of_ingredient() {
        let pantry = vec!["olive oil".to_string()];
        let (matched, item) = find_pantry_match("extra-virgin olive oil", &pantry);
        assert!(matched);
        assert_eq!(item.as_deref(), Some("olive oil"));
    }

    #[test]
    fn match_ingredient_subset_of_pantry() {
        let pantry = vec!["chicken thighs".to_string()];
        let (matched, _) = find_pantry_match("chicken", &pantry);
        assert!(matched);
    }

    #[test]
    fn no_match_unrelated() {
        let pantry = vec!["olive oil".to_string()];
        let (matched, _) = find_pantry_match("butter", &pantry);
        assert!(!matched);
    }

    #[test]
    fn no_false_positive_ham_graham() {
        // "ham" should NOT match "graham crackers" — word boundary matching
        let pantry = vec!["ham".to_string()];
        let (matched, _) = find_pantry_match("graham crackers", &pantry);
        assert!(!matched, "ham should not match graham crackers");
    }

    #[test]
    fn match_with_prep_modifiers() {
        let pantry = vec!["garlic".to_string()];
        let (matched, _) = find_pantry_match("garlic, minced", &pantry);
        assert!(matched);
    }

    #[test]
    fn match_case_insensitive() {
        let pantry = vec!["Flour".to_string()];
        let (matched, _) = find_pantry_match("all-purpose flour", &pantry);
        assert!(matched);
    }

    #[test]
    fn match_absent_item_not_in_list() {
        // Only present items are passed to find_pantry_match,
        // so this just tests empty pantry
        let pantry: Vec<String> = vec![];
        let (matched, _) = find_pantry_match("flour", &pantry);
        assert!(!matched);
    }
}
