use rusqlite::params;

use crate::db::FondDb;
use crate::error::StoreError;

/// Summary of a recipe for list views.
#[derive(Debug)]
pub struct RecipeSummary {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub source: String,
    pub tags: Vec<String>,
}

/// A full recipe record from the database (indexed projection).
#[derive(Debug)]
pub struct RecipeRecord {
    pub id: i64,
    pub file_path: String,
    pub slug: String,
    pub title: String,
    pub source: String,
    pub source_url: String,
    pub description: String,
    pub recipe_yield: String,
    pub prep_time: String,
    pub cook_time: String,
    pub total_time: String,
    pub servings: String,
    pub content_hash: String,
    pub raw_source: String,
    pub created_at: String,
    pub updated_at: String,
}

/// FTS5 search result.
#[derive(Debug)]
pub struct SearchResult {
    pub recipe_id: i64,
    pub title: String,
    pub slug: String,
    pub rank: f64,
}

/// Repository for recipe persistence operations.
pub struct RecipeRepository<'a> {
    db: &'a FondDb,
}

impl<'a> RecipeRepository<'a> {
    pub fn new(db: &'a FondDb) -> Self {
        Self { db }
    }

    /// Insert or update a recipe by file_path (the stable reindex key).
    ///
    /// Returns the SQLite rowid of the upserted recipe.
    pub fn upsert_recipe(
        &self,
        file_path: &str,
        recipe: &fond_domain::Recipe,
        content_hash: &str,
    ) -> Result<i64, StoreError> {
        let conn = self.db.conn();

        // Check if recipe already exists by file_path
        let existing_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM recipes WHERE file_path = ?1",
                params![file_path],
                |row| row.get(0),
            )
            .ok();

        let recipe_id = if let Some(id) = existing_id {
            // Update existing recipe
            conn.execute(
                "UPDATE recipes SET slug = ?1, title = ?2, source = ?3, source_url = ?4,
                 description = ?5, recipe_yield = ?6, prep_time = ?7, cook_time = ?8,
                 total_time = ?9, servings = ?10, content_hash = ?11, raw_source = ?12,
                 updated_at = datetime('now')
                 WHERE id = ?13",
                params![
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
                    id,
                ],
            )?;

            // Delete child rows for rebuild
            conn.execute(
                "DELETE FROM recipe_ingredients WHERE recipe_id = ?1",
                params![id],
            )?;
            conn.execute("DELETE FROM steps WHERE recipe_id = ?1", params![id])?;
            conn.execute("DELETE FROM cookware WHERE recipe_id = ?1", params![id])?;
            conn.execute("DELETE FROM tags WHERE recipe_id = ?1", params![id])?;
            conn.execute("DELETE FROM recipe_fts WHERE rowid = ?1", params![id])?;

            id
        } else {
            // Insert new recipe
            conn.execute(
                "INSERT INTO recipes (file_path, slug, title, source, source_url,
                 description, recipe_yield, prep_time, cook_time, total_time,
                 servings, content_hash, raw_source)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
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
            conn.last_insert_rowid()
        };

        // Insert child rows
        for (i, ing) in recipe.ingredients.iter().enumerate() {
            conn.execute(
                "INSERT INTO recipe_ingredients (recipe_id, name, quantity, unit, note, optional, sort_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
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
            conn.execute(
                "INSERT INTO steps (recipe_id, section, body, sort_order)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    recipe_id,
                    step.section.as_deref().unwrap_or(""),
                    step.body,
                    step.order as i32,
                ],
            )?;
        }

        for cw in &recipe.cookware {
            conn.execute(
                "INSERT INTO cookware (recipe_id, name, quantity)
                 VALUES (?1, ?2, ?3)",
                params![recipe_id, cw.name, cw.quantity.as_deref().unwrap_or(""),],
            )?;
        }

        for tag in &recipe.tags {
            conn.execute(
                "INSERT OR IGNORE INTO tags (name, recipe_id) VALUES (?1, ?2)",
                params![tag, recipe_id],
            )?;
        }

        // FTS5 index
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

        conn.execute(
            "INSERT INTO recipe_fts (rowid, title, ingredients_text, steps_text, tags_text)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                recipe_id,
                recipe.title,
                ingredients_text,
                steps_text,
                tags_text
            ],
        )?;

        Ok(recipe_id)
    }

    /// Get a recipe record by its SQLite id.
    pub fn get_recipe_by_id(&self, id: i64) -> Result<Option<RecipeRecord>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, file_path, slug, title, source, source_url, description,
                    recipe_yield, prep_time, cook_time, total_time, servings,
                    content_hash, raw_source, created_at, updated_at
             FROM recipes WHERE id = ?1",
        )?;

        let record = stmt.query_row(params![id], row_to_record).ok();

        Ok(record)
    }

    /// Get a recipe record by slug.
    pub fn get_recipe_by_slug(&self, slug: &str) -> Result<Option<RecipeRecord>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, file_path, slug, title, source, source_url, description,
                    recipe_yield, prep_time, cook_time, total_time, servings,
                    content_hash, raw_source, created_at, updated_at
             FROM recipes WHERE slug = ?1",
        )?;

        let record = stmt.query_row(params![slug], row_to_record).ok();

        Ok(record)
    }

    /// Get a recipe record by file path.
    pub fn get_recipe_by_path(&self, file_path: &str) -> Result<Option<RecipeRecord>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, file_path, slug, title, source, source_url, description,
                    recipe_yield, prep_time, cook_time, total_time, servings,
                    content_hash, raw_source, created_at, updated_at
             FROM recipes WHERE file_path = ?1",
        )?;

        let record = stmt.query_row(params![file_path], row_to_record).ok();

        Ok(record)
    }

    /// List all recipes (summary view).
    pub fn list_recipes(&self) -> Result<Vec<RecipeSummary>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT r.id, r.slug, r.title, r.source
             FROM recipes r ORDER BY r.title",
        )?;

        let recipes: Vec<RecipeSummary> = stmt
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                Ok((
                    id,
                    RecipeSummary {
                        id,
                        slug: row.get(1)?,
                        title: row.get(2)?,
                        source: row.get(3)?,
                        tags: Vec::new(),
                    },
                ))
            })?
            .filter_map(|r| r.ok())
            .map(|(id, mut summary)| {
                // Fetch tags for each recipe
                if let Ok(mut tag_stmt) =
                    conn.prepare_cached("SELECT name FROM tags WHERE recipe_id = ?1 ORDER BY name")
                    && let Ok(tags) = tag_stmt
                        .query_map(params![id], |row| row.get::<_, String>(0))
                        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
                {
                    summary.tags = tags;
                }
                summary
            })
            .collect();

        Ok(recipes)
    }

    /// Full-text search across recipes.
    pub fn search(&self, query: &str) -> Result<Vec<SearchResult>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT f.rowid, r.title, r.slug, rank
             FROM recipe_fts f
             JOIN recipes r ON r.id = f.rowid
             WHERE recipe_fts MATCH ?1
             ORDER BY rank",
        )?;

        let results = stmt
            .query_map(params![query], |row| {
                Ok(SearchResult {
                    recipe_id: row.get(0)?,
                    title: row.get(1)?,
                    slug: row.get(2)?,
                    rank: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// Delete all derived data (recipe index + FTS). Preserves overlay tables.
    pub fn delete_all_derived(&self) -> Result<(), StoreError> {
        let conn = self.db.conn();
        conn.execute_batch(
            "DELETE FROM recipe_fts;
             DELETE FROM tags;
             DELETE FROM cookware;
             DELETE FROM steps;
             DELETE FROM recipe_ingredients;
             DELETE FROM recipes;",
        )?;
        Ok(())
    }

    /// Count the total number of recipes.
    pub fn count_recipes(&self) -> Result<i64, StoreError> {
        let conn = self.db.conn();
        let count: i64 = conn.query_row("SELECT count(*) FROM recipes", [], |row| row.get(0))?;
        Ok(count)
    }
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecipeRecord> {
    Ok(RecipeRecord {
        id: row.get(0)?,
        file_path: row.get(1)?,
        slug: row.get(2)?,
        title: row.get(3)?,
        source: row.get(4)?,
        source_url: row.get(5)?,
        description: row.get(6)?,
        recipe_yield: row.get(7)?,
        prep_time: row.get(8)?,
        cook_time: row.get(9)?,
        total_time: row.get(10)?,
        servings: row.get(11)?,
        content_hash: row.get(12)?,
        raw_source: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}
