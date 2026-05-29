use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::db::FondDb;
use crate::error::StoreError;

/// Result of a reindex operation.
#[derive(Debug)]
pub struct ReindexReport {
    /// Number of recipes successfully indexed.
    pub indexed: usize,
    /// Files that failed to parse, with error descriptions.
    pub errors: Vec<(String, String)>,
}

/// Rebuild the derived recipe index from `.cook` files on disk.
///
/// This is an atomic operation: all files are parsed first, then
/// a single transaction clears the old index and inserts the new one.
/// Invalid files are skipped with error reporting — they do not
/// prevent valid files from being indexed.
///
/// Only derived tables are touched (recipes, ingredients, steps,
/// cookware, tags, FTS). Overlay tables (users, etc.) are preserved.
pub fn reindex(db: &FondDb, recipes_dir: &Path) -> Result<ReindexReport, StoreError> {
    // Phase 1: parse all .cook files (outside transaction)
    let mut parsed = Vec::new();
    let mut errors = Vec::new();

    if recipes_dir.exists() {
        for entry in fs::read_dir(recipes_dir)?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("cook") {
                let file_name = path.file_name().unwrap().to_str().unwrap().to_string();
                let file_stem = path.file_stem().unwrap().to_str().unwrap().to_string();
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        let hash = content_hash(&content);
                        match fond_domain::parse_cook(&content, &file_stem) {
                            Ok(recipe) => {
                                parsed.push((file_name, recipe, hash));
                            }
                            Err(e) => {
                                errors.push((file_name, e.to_string()));
                            }
                        }
                    }
                    Err(e) => {
                        errors.push((file_name, format!("failed to read: {e}")));
                    }
                }
            }
        }
    }

    // Deterministic ordering
    parsed.sort_by(|a, b| a.0.cmp(&b.0));

    // Phase 2: atomic rebuild inside transaction
    let conn = db.conn();
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| StoreError::Database {
            message: format!("failed to begin transaction: {e}"),
        })?;

    // Clear derived tables (preserves overlay tables like users)
    tx.execute_batch(
        "DELETE FROM recipe_fts;
         DELETE FROM tags;
         DELETE FROM cookware;
         DELETE FROM steps;
         DELETE FROM recipe_ingredients;
         DELETE FROM recipes;",
    )
    .map_err(|e| StoreError::Database {
        message: format!("failed to clear derived tables: {e}"),
    })?;

    // Create a temporary FondDb-like wrapper for the transaction
    // We can't use RecipeRepository directly since it needs &FondDb,
    // so we inline the insert logic here using the transaction.
    for (file_path, recipe, hash) in &parsed {
        insert_recipe_in_tx(&tx, file_path, recipe, hash)?;
    }

    tx.commit().map_err(|e| StoreError::Database {
        message: format!("failed to commit reindex: {e}"),
    })?;

    Ok(ReindexReport {
        indexed: parsed.len(),
        errors,
    })
}

/// Insert a recipe and its child rows using a transaction reference.
fn insert_recipe_in_tx(
    tx: &rusqlite::Transaction<'_>,
    file_path: &str,
    recipe: &fond_domain::Recipe,
    content_hash: &str,
) -> Result<i64, StoreError> {
    tx.execute(
        "INSERT INTO recipes (file_path, slug, title, source, source_url,
         description, recipe_yield, prep_time, cook_time, total_time,
         servings, content_hash, raw_source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        rusqlite::params![
            file_path,
            recipe.slug,
            recipe.title,
            recipe.source.as_deref().unwrap_or(""),
            recipe.source_url.as_deref().unwrap_or(""),
            recipe.description.as_deref().unwrap_or(""),
            recipe.recipe_yield.as_deref().unwrap_or(""),
            recipe.prep_time.as_deref().unwrap_or(""),
            recipe.cook_time.as_deref().unwrap_or(""),
            recipe.total_time.as_deref().unwrap_or(""),
            recipe.servings.as_deref().unwrap_or(""),
            content_hash,
            recipe.raw_source.as_deref().unwrap_or(""),
        ],
    )?;
    let recipe_id = tx.last_insert_rowid();

    for (i, ing) in recipe.ingredients.iter().enumerate() {
        tx.execute(
            "INSERT INTO recipe_ingredients (recipe_id, name, quantity, unit, note, optional, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                recipe_id,
                ing.name,
                ing.quantity.as_deref().unwrap_or(""),
                ing.unit.as_deref().unwrap_or(""),
                ing.note.as_deref().unwrap_or(""),
                ing.optional as i32,
                i as i32,
            ],
        )?;
    }

    for step in &recipe.steps {
        tx.execute(
            "INSERT INTO steps (recipe_id, section, body, sort_order)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                recipe_id,
                step.section.as_deref().unwrap_or(""),
                step.body,
                step.order as i32,
            ],
        )?;
    }

    for cw in &recipe.cookware {
        tx.execute(
            "INSERT INTO cookware (recipe_id, name, quantity)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![recipe_id, cw.name, cw.quantity.as_deref().unwrap_or("")],
        )?;
    }

    for tag in &recipe.tags {
        tx.execute(
            "INSERT OR IGNORE INTO tags (name, recipe_id) VALUES (?1, ?2)",
            rusqlite::params![tag, recipe_id],
        )?;
    }

    // FTS5
    let ingredients_text: String = recipe
        .ingredients
        .iter()
        .map(|i| i.name.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let steps_text: String = recipe
        .steps
        .iter()
        .map(|s| s.body.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let tags_text: String = recipe.tags.join(" ");

    tx.execute(
        "INSERT INTO recipe_fts (rowid, title, ingredients_text, steps_text, tags_text)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            recipe_id,
            recipe.title,
            ingredients_text,
            steps_text,
            tags_text
        ],
    )?;

    Ok(recipe_id)
}

fn content_hash(content: &str) -> String {
    let mut h = DefaultHasher::new();
    content.hash(&mut h);
    format!("{:016x}", h.finish())
}
