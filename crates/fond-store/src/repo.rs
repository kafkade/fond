use rusqlite::params;
use serde::Serialize;

use crate::db::FondDb;
use crate::error::StoreError;
use fond_domain::{RecipeFilter, parse_time_minutes};

/// Summary of a recipe for list views.
#[derive(Debug, Serialize)]
pub struct RecipeSummary {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub source: String,
    pub tags: Vec<String>,
    pub total_time: String,
    pub total_time_minutes: Option<u32>,
}

/// A full recipe record from the database (indexed projection).
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub recipe_id: i64,
    pub title: String,
    pub slug: String,
    pub source: String,
    pub tags: Vec<String>,
    pub rank: f64,
}

/// A tag with its recipe count.
#[derive(Debug, Serialize)]
pub struct TagCount {
    pub name: String,
    pub count: i64,
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

        let total_time_minutes = compute_total_time_minutes(recipe);

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
                 total_time_minutes = ?13, updated_at = datetime('now')
                 WHERE id = ?14",
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
                    total_time_minutes,
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
                 servings, content_hash, raw_source, total_time_minutes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
                    total_time_minutes,
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
        self.list_recipes_filtered(&RecipeFilter::default())
    }

    /// List recipes filtered by tag, max time, and/or source.
    pub fn list_recipes_filtered(
        &self,
        filter: &RecipeFilter,
    ) -> Result<Vec<RecipeSummary>, StoreError> {
        let conn = self.db.conn();

        let (where_clause, bind_values) = build_filter_where(filter, 1);

        let sql = format!(
            "SELECT r.id, r.slug, r.title, r.source, r.total_time, r.total_time_minutes
             FROM recipes r
             {where_clause}
             ORDER BY r.title"
        );

        let mut stmt = conn.prepare(&sql)?;

        let recipes: Vec<RecipeSummary> = stmt
            .query_map(rusqlite::params_from_iter(bind_values.iter()), |row| {
                let id: i64 = row.get(0)?;
                let total_time_minutes: Option<u32> = row.get(5)?;
                Ok((
                    id,
                    RecipeSummary {
                        id,
                        slug: row.get(1)?,
                        title: row.get(2)?,
                        source: row.get(3)?,
                        tags: Vec::new(),
                        total_time: row.get(4)?,
                        total_time_minutes,
                    },
                ))
            })?
            .filter_map(|r| r.ok())
            .map(|(id, mut summary)| {
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

    /// Full-text search across recipes (unfiltered).
    pub fn search(&self, query: &str) -> Result<Vec<SearchResult>, StoreError> {
        self.search_filtered(query, &RecipeFilter::default())
    }

    /// Full-text search with filters.
    pub fn search_filtered(
        &self,
        query: &str,
        filter: &RecipeFilter,
    ) -> Result<Vec<SearchResult>, StoreError> {
        let conn = self.db.conn();

        let (filter_where, filter_values) = build_filter_where(filter, 2);

        // Build the WHERE clause: FTS MATCH + any filters
        // The filter_where already starts with "WHERE ..." if non-empty,
        // so we need to integrate it with the FTS MATCH.
        let sql = if filter_where.is_empty() {
            "SELECT f.rowid, r.title, r.slug, r.source, rank
             FROM recipe_fts f
             JOIN recipes r ON r.id = f.rowid
             WHERE recipe_fts MATCH ?1
             ORDER BY rank"
                .to_string()
        } else {
            // Replace "WHERE" with "AND" in filter clause since we already have WHERE
            let filter_and = filter_where.replacen("WHERE", "AND", 1);
            format!(
                "SELECT f.rowid, r.title, r.slug, r.source, rank
                 FROM recipe_fts f
                 JOIN recipes r ON r.id = f.rowid
                 WHERE recipe_fts MATCH ?1
                 {filter_and}
                 ORDER BY rank"
            )
        };

        // Build parameter list: FTS query first, then filter params
        let mut all_params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        all_params.push(Box::new(query.to_string()));
        for v in &filter_values {
            all_params.push(Box::new(v.clone()));
        }

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            all_params.iter().map(|b| b.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let results: Vec<SearchResult> = stmt
            .query_map(param_refs.as_slice(), |row| {
                let recipe_id: i64 = row.get(0)?;
                Ok((
                    recipe_id,
                    SearchResult {
                        recipe_id,
                        title: row.get(1)?,
                        slug: row.get(2)?,
                        source: row.get(3)?,
                        tags: Vec::new(),
                        rank: row.get(4)?,
                    },
                ))
            })?
            .filter_map(|r| r.ok())
            .map(|(id, mut result)| {
                if let Ok(mut tag_stmt) =
                    conn.prepare_cached("SELECT name FROM tags WHERE recipe_id = ?1 ORDER BY name")
                    && let Ok(tags) = tag_stmt
                        .query_map(params![id], |row| row.get::<_, String>(0))
                        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
                {
                    result.tags = tags;
                }
                result
            })
            .collect();

        Ok(results)
    }

    /// List all distinct tags with their recipe counts.
    pub fn list_tags(&self) -> Result<Vec<TagCount>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT name, COUNT(*) as cnt FROM tags GROUP BY name ORDER BY cnt DESC, name",
        )?;

        let tags = stmt
            .query_map([], |row| {
                Ok(TagCount {
                    name: row.get(0)?,
                    count: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(tags)
    }

    /// Get the tags for a specific recipe by slug.
    pub fn get_tags_for_slug(&self, slug: &str) -> Result<Option<(i64, Vec<String>)>, StoreError> {
        let conn = self.db.conn();

        let row: Option<i64> = conn
            .query_row(
                "SELECT id FROM recipes WHERE slug = ?1",
                params![slug],
                |row| row.get(0),
            )
            .ok();

        let Some(recipe_id) = row else {
            return Ok(None);
        };

        let mut stmt = conn.prepare("SELECT name FROM tags WHERE recipe_id = ?1 ORDER BY name")?;
        let tags: Vec<String> = stmt
            .query_map(params![recipe_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some((recipe_id, tags)))
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

    /// Delete a single recipe by slug.
    ///
    /// Returns the file_path of the deleted recipe (for callers that
    /// need to remove the file from disk), or `None` if the slug was
    /// not found.
    pub fn delete_recipe_by_slug(&self, slug: &str) -> Result<Option<String>, StoreError> {
        let conn = self.db.conn();

        // Look up the recipe first to get id and file_path
        let row: Option<(i64, String)> = conn
            .query_row(
                "SELECT id, file_path FROM recipes WHERE slug = ?1",
                params![slug],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let Some((id, file_path)) = row else {
            return Ok(None);
        };

        // Delete FTS entry (not covered by CASCADE)
        conn.execute("DELETE FROM recipe_fts WHERE rowid = ?1", params![id])?;
        // Delete recipe (CASCADE handles child tables)
        conn.execute("DELETE FROM recipes WHERE id = ?1", params![id])?;

        Ok(Some(file_path))
    }
}

/// Compute total_time_minutes from the recipe's time fields.
///
/// Tries `total_time` first, then falls back to `prep_time + cook_time`.
fn compute_total_time_minutes(recipe: &fond_domain::Recipe) -> Option<u32> {
    // Try total_time first
    if let Some(ref total) = recipe.total_time
        && let Some(mins) = parse_time_minutes(total)
    {
        return Some(mins);
    }

    // Fall back to prep + cook sum
    let prep = recipe
        .prep_time
        .as_deref()
        .and_then(parse_time_minutes)
        .unwrap_or(0);
    let cook = recipe
        .cook_time
        .as_deref()
        .and_then(parse_time_minutes)
        .unwrap_or(0);

    if prep > 0 || cook > 0 {
        Some(prep + cook)
    } else {
        None
    }
}

/// Build a SQL WHERE clause and parameter values from a `RecipeFilter`.
///
/// `param_start` is the first `?N` placeholder index to use.
/// Returns `("WHERE ...", vec_of_string_params)` or `("", vec![])` if
/// no filters are active.
fn build_filter_where(filter: &RecipeFilter, param_start: usize) -> (String, Vec<String>) {
    if filter.is_empty() {
        return (String::new(), Vec::new());
    }

    let mut conditions: Vec<String> = Vec::new();
    let mut values: Vec<String> = Vec::new();
    let mut param_idx = param_start;

    // Tag filter (AND semantics: recipe must have ALL listed tags)
    if !filter.tags.is_empty() {
        let deduped: Vec<&String> = {
            let mut seen = std::collections::HashSet::new();
            filter
                .tags
                .iter()
                .filter(|t| seen.insert(t.as_str()))
                .collect()
        };
        let placeholders: Vec<String> = deduped
            .iter()
            .map(|_| {
                let p = format!("?{param_idx}");
                param_idx += 1;
                p
            })
            .collect();
        let count = deduped.len();
        conditions.push(format!(
            "r.id IN (SELECT recipe_id FROM tags WHERE name IN ({}) GROUP BY recipe_id HAVING COUNT(DISTINCT name) = {})",
            placeholders.join(", "),
            count
        ));
        for tag in deduped {
            values.push(tag.clone());
        }
    }

    // Max time filter
    if let Some(max) = filter.max_time_minutes {
        conditions.push(format!(
            "r.total_time_minutes IS NOT NULL AND r.total_time_minutes <= ?{param_idx}"
        ));
        values.push(max.to_string());
        param_idx += 1;
    }

    // Source filter (case-insensitive substring match)
    if let Some(ref source) = filter.source {
        // Escape SQL LIKE wildcards in user input
        let escaped = source.replace('%', "\\%").replace('_', "\\_");
        conditions.push(format!("r.source LIKE ?{param_idx} ESCAPE '\\'"));
        values.push(format!("%{escaped}%"));
        let _ = param_idx; // suppress unused warning
    }

    let where_clause = format!("WHERE {}", conditions.join(" AND "));
    (where_clause, values)
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
