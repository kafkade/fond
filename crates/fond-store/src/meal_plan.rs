use chrono::Datelike;
use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::{FondDb, StoreError};

// ═══════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════

/// A meal plan record from the database.
#[derive(Debug, Serialize)]
pub struct MealPlanRecord {
    pub id: i64,
    pub name: String,
    pub start_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub entries: Vec<MealPlanEntryRecord>,
}

/// A single entry in a meal plan.
#[derive(Debug, Clone, Serialize)]
pub struct MealPlanEntryRecord {
    pub id: i64,
    pub plan_date: String,
    pub meal: String,
    pub recipe_slug: String,
    /// Resolved recipe title (None if recipe no longer exists).
    pub recipe_title: Option<String>,
}

/// Summary of a meal plan (for list views, without entries).
#[derive(Debug, Serialize)]
pub struct MealPlanSummary {
    pub id: i64,
    pub name: String,
    pub start_date: Option<String>,
    pub entry_count: i64,
    pub created_at: String,
}

/// Recognized meal slots.
const VALID_MEALS: &[&str] = &["breakfast", "lunch", "dinner", "snack"];

/// Weekday names in order (Monday-first).
const WEEKDAYS: &[&str] = &[
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

// ═══════════════════════════════════════════════════════════════════
// Repository
// ═══════════════════════════════════════════════════════════════════

/// Repository for meal plan operations.
pub struct MealPlanRepository<'a> {
    db: &'a FondDb,
}

impl<'a> MealPlanRepository<'a> {
    pub fn new(db: &'a FondDb) -> Self {
        Self { db }
    }

    /// Create a new meal plan (or return existing by name).
    pub fn get_or_create(&self, name: &str) -> Result<i64, StoreError> {
        let conn = self.db.conn();

        // Check if it already exists
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM meal_plans WHERE LOWER(name) = LOWER(?1)",
                params![name],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(id) = existing {
            return Ok(id);
        }

        conn.execute("INSERT INTO meal_plans (name) VALUES (?1)", params![name])?;
        Ok(conn.last_insert_rowid())
    }

    /// Add a recipe to a meal plan.
    ///
    /// `plan_date` should be an ISO date (YYYY-MM-DD).
    /// `meal` should be one of: breakfast, lunch, dinner, snack.
    /// `recipe_slug` is the recipe's slug identifier.
    pub fn add_entry(
        &self,
        plan_id: i64,
        plan_date: &str,
        meal: &str,
        recipe_slug: &str,
    ) -> Result<i64, StoreError> {
        let meal_lower = meal.to_lowercase();
        if !VALID_MEALS.contains(&meal_lower.as_str()) {
            return Err(StoreError::Database {
                message: format!(
                    "invalid meal '{}' — expected one of: {}",
                    meal,
                    VALID_MEALS.join(", ")
                ),
            });
        }

        let conn = self.db.conn();

        // Verify the recipe exists
        let recipe_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM recipes WHERE slug = ?1",
                params![recipe_slug],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)?;

        if !recipe_exists {
            return Err(StoreError::Database {
                message: format!("recipe not found: '{recipe_slug}'"),
            });
        }

        conn.execute(
            "INSERT OR IGNORE INTO meal_plan_entries (meal_plan_id, plan_date, meal, recipe_slug)
             VALUES (?1, ?2, ?3, ?4)",
            params![plan_id, plan_date, meal_lower, recipe_slug],
        )
        .map_err(|e| StoreError::Database {
            message: format!("failed to add plan entry: {e}"),
        })?;

        // Update plan timestamp
        conn.execute(
            "UPDATE meal_plans SET updated_at = datetime('now') WHERE id = ?1",
            params![plan_id],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Remove a specific entry from a meal plan.
    pub fn remove_entry(
        &self,
        plan_id: i64,
        plan_date: &str,
        meal: &str,
        recipe_slug: &str,
    ) -> Result<bool, StoreError> {
        let conn = self.db.conn();
        let affected = conn.execute(
            "DELETE FROM meal_plan_entries
             WHERE meal_plan_id = ?1 AND plan_date = ?2 AND meal = ?3 AND recipe_slug = ?4",
            params![plan_id, plan_date, meal.to_lowercase(), recipe_slug],
        )?;

        if affected > 0 {
            conn.execute(
                "UPDATE meal_plans SET updated_at = datetime('now') WHERE id = ?1",
                params![plan_id],
            )?;
        }

        Ok(affected > 0)
    }

    /// Get a meal plan with all its entries, sorted by date then meal order.
    pub fn get_plan(&self, name: &str) -> Result<Option<MealPlanRecord>, StoreError> {
        let conn = self.db.conn();

        let plan = conn
            .query_row(
                "SELECT id, name, start_date, created_at, updated_at
                 FROM meal_plans WHERE LOWER(name) = LOWER(?1)",
                params![name],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?;

        let Some((id, plan_name, start_date, created_at, updated_at)) = plan else {
            return Ok(None);
        };

        let entries = self.get_entries(id)?;

        Ok(Some(MealPlanRecord {
            id,
            name: plan_name,
            start_date,
            created_at,
            updated_at,
            entries,
        }))
    }

    /// List all meal plans (summary only).
    pub fn list_plans(&self) -> Result<Vec<MealPlanSummary>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT mp.id, mp.name, mp.start_date, mp.created_at,
                    (SELECT COUNT(*) FROM meal_plan_entries WHERE meal_plan_id = mp.id)
             FROM meal_plans mp
             ORDER BY mp.updated_at DESC",
        )?;

        let rows = stmt
            .query_map([], |row| {
                Ok(MealPlanSummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    start_date: row.get(2)?,
                    entry_count: row.get(4)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Delete a meal plan and all its entries.
    pub fn delete_plan(&self, name: &str) -> Result<bool, StoreError> {
        let conn = self.db.conn();
        let affected = conn.execute(
            "DELETE FROM meal_plans WHERE LOWER(name) = LOWER(?1)",
            params![name],
        )?;
        Ok(affected > 0)
    }

    /// Clear all entries from a plan (keep the plan itself).
    pub fn clear_plan(&self, name: &str) -> Result<i64, StoreError> {
        let conn = self.db.conn();

        let plan_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM meal_plans WHERE LOWER(name) = LOWER(?1)",
                params![name],
                |row| row.get(0),
            )
            .optional()?;

        let Some(id) = plan_id else {
            return Err(StoreError::Database {
                message: format!("plan not found: '{name}'"),
            });
        };

        let affected = conn.execute(
            "DELETE FROM meal_plan_entries WHERE meal_plan_id = ?1",
            params![id],
        )?;

        conn.execute(
            "UPDATE meal_plans SET updated_at = datetime('now') WHERE id = ?1",
            params![id],
        )?;

        Ok(affected as i64)
    }

    /// Get all recipe slugs referenced by a meal plan.
    pub fn get_plan_recipe_slugs(&self, name: &str) -> Result<Vec<String>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT mpe.recipe_slug
             FROM meal_plan_entries mpe
             JOIN meal_plans mp ON mp.id = mpe.meal_plan_id
             WHERE LOWER(mp.name) = LOWER(?1)
             ORDER BY mpe.recipe_slug",
        )?;

        let slugs = stmt
            .query_map(params![name], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(slugs)
    }

    // ─── Internal helpers ────────────────────────────────

    fn get_entries(&self, plan_id: i64) -> Result<Vec<MealPlanEntryRecord>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT mpe.id, mpe.plan_date, mpe.meal, mpe.recipe_slug, r.title
             FROM meal_plan_entries mpe
             LEFT JOIN recipes r ON r.slug = mpe.recipe_slug
             WHERE mpe.meal_plan_id = ?1
             ORDER BY mpe.plan_date ASC,
                      CASE mpe.meal
                          WHEN 'breakfast' THEN 1
                          WHEN 'lunch' THEN 2
                          WHEN 'dinner' THEN 3
                          WHEN 'snack' THEN 4
                          ELSE 5
                      END ASC",
        )?;

        let rows = stmt
            .query_map(params![plan_id], |row| {
                Ok(MealPlanEntryRecord {
                    id: row.get(0)?,
                    plan_date: row.get(1)?,
                    meal: row.get(2)?,
                    recipe_slug: row.get(3)?,
                    recipe_title: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }
}

/// Parse a weekday name to its index (0 = Monday).
pub fn weekday_index(day: &str) -> Option<usize> {
    WEEKDAYS.iter().position(|d| d.eq_ignore_ascii_case(day))
}

/// Check if a string is a valid weekday name.
pub fn is_weekday(s: &str) -> bool {
    weekday_index(s).is_some()
}

/// Resolve a weekday name to an ISO date relative to the current week.
///
/// If today is Wednesday 2025-06-04, "monday" resolves to 2025-06-02.
pub fn weekday_to_date(day: &str) -> Option<String> {
    let idx = weekday_index(day)?;
    let today = chrono::Local::now().date_naive();
    let today_weekday = today.weekday().num_days_from_monday() as usize;
    let offset = idx as i64 - today_weekday as i64;
    let target = today + chrono::Duration::days(offset);
    Some(target.format("%Y-%m-%d").to_string())
}

/// Get all weekday dates for the current week (Monday through Sunday).
pub fn current_week_dates() -> Vec<(String, String)> {
    let today = chrono::Local::now().date_naive();
    let monday_offset = today.weekday().num_days_from_monday() as i64;
    let monday = today - chrono::Duration::days(monday_offset);

    (0..7)
        .map(|i| {
            let date = monday + chrono::Duration::days(i);
            let day_name = WEEKDAYS[i as usize].to_string();
            let date_str = date.format("%Y-%m-%d").to_string();
            (day_name, date_str)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> FondDb {
        let db = FondDb::open_memory().unwrap();
        // Insert test recipes
        let conn = db.conn();
        conn.execute(
            "INSERT INTO recipes (slug, title, file_path) VALUES ('chicken-adobo', 'Chicken Adobo', 'chicken-adobo.cook')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO recipes (slug, title, file_path) VALUES ('pasta-primavera', 'Pasta Primavera', 'pasta-primavera.cook')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO recipes (slug, title, file_path) VALUES ('oatmeal', 'Oatmeal', 'oatmeal.cook')",
            [],
        )
        .unwrap();
        db
    }

    #[test]
    fn create_and_get_plan() {
        let db = setup_db();
        let repo = MealPlanRepository::new(&db);

        let id = repo.get_or_create("week").unwrap();
        assert!(id > 0);

        // Idempotent — returns same plan
        let id2 = repo.get_or_create("week").unwrap();
        assert_eq!(id, id2);

        let plan = repo.get_plan("week").unwrap().unwrap();
        assert_eq!(plan.name, "week");
        assert!(plan.entries.is_empty());
    }

    #[test]
    fn add_and_list_entries() {
        let db = setup_db();
        let repo = MealPlanRepository::new(&db);

        let plan_id = repo.get_or_create("week").unwrap();
        repo.add_entry(plan_id, "2025-06-02", "dinner", "chicken-adobo")
            .unwrap();
        repo.add_entry(plan_id, "2025-06-02", "lunch", "pasta-primavera")
            .unwrap();
        repo.add_entry(plan_id, "2025-06-03", "breakfast", "oatmeal")
            .unwrap();

        let plan = repo.get_plan("week").unwrap().unwrap();
        assert_eq!(plan.entries.len(), 3);

        // Entries should be ordered by date, then meal
        assert_eq!(plan.entries[0].meal, "lunch"); // 06-02, lunch before dinner
        assert_eq!(plan.entries[1].meal, "dinner"); // 06-02, dinner
        assert_eq!(plan.entries[2].meal, "breakfast"); // 06-03
    }

    #[test]
    fn add_duplicate_entry_is_ignored() {
        let db = setup_db();
        let repo = MealPlanRepository::new(&db);

        let plan_id = repo.get_or_create("week").unwrap();
        repo.add_entry(plan_id, "2025-06-02", "dinner", "chicken-adobo")
            .unwrap();
        // Same entry again — INSERT OR IGNORE
        repo.add_entry(plan_id, "2025-06-02", "dinner", "chicken-adobo")
            .unwrap();

        let plan = repo.get_plan("week").unwrap().unwrap();
        assert_eq!(plan.entries.len(), 1);
    }

    #[test]
    fn multiple_recipes_per_meal() {
        let db = setup_db();
        let repo = MealPlanRepository::new(&db);

        let plan_id = repo.get_or_create("week").unwrap();
        // Main + side for dinner
        repo.add_entry(plan_id, "2025-06-02", "dinner", "chicken-adobo")
            .unwrap();
        repo.add_entry(plan_id, "2025-06-02", "dinner", "pasta-primavera")
            .unwrap();

        let plan = repo.get_plan("week").unwrap().unwrap();
        assert_eq!(plan.entries.len(), 2);
    }

    #[test]
    fn invalid_meal_rejected() {
        let db = setup_db();
        let repo = MealPlanRepository::new(&db);

        let plan_id = repo.get_or_create("week").unwrap();
        let result = repo.add_entry(plan_id, "2025-06-02", "brunch", "chicken-adobo");
        assert!(result.is_err());
    }

    #[test]
    fn nonexistent_recipe_rejected() {
        let db = setup_db();
        let repo = MealPlanRepository::new(&db);

        let plan_id = repo.get_or_create("week").unwrap();
        let result = repo.add_entry(plan_id, "2025-06-02", "dinner", "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn remove_entry() {
        let db = setup_db();
        let repo = MealPlanRepository::new(&db);

        let plan_id = repo.get_or_create("week").unwrap();
        repo.add_entry(plan_id, "2025-06-02", "dinner", "chicken-adobo")
            .unwrap();

        assert!(
            repo.remove_entry(plan_id, "2025-06-02", "dinner", "chicken-adobo")
                .unwrap()
        );

        let plan = repo.get_plan("week").unwrap().unwrap();
        assert!(plan.entries.is_empty());
    }

    #[test]
    fn remove_nonexistent_entry() {
        let db = setup_db();
        let repo = MealPlanRepository::new(&db);

        let plan_id = repo.get_or_create("week").unwrap();
        assert!(
            !repo
                .remove_entry(plan_id, "2025-06-02", "dinner", "chicken-adobo")
                .unwrap()
        );
    }

    #[test]
    fn clear_plan() {
        let db = setup_db();
        let repo = MealPlanRepository::new(&db);

        let plan_id = repo.get_or_create("week").unwrap();
        repo.add_entry(plan_id, "2025-06-02", "dinner", "chicken-adobo")
            .unwrap();
        repo.add_entry(plan_id, "2025-06-03", "lunch", "pasta-primavera")
            .unwrap();

        let cleared = repo.clear_plan("week").unwrap();
        assert_eq!(cleared, 2);

        // Plan still exists
        let plan = repo.get_plan("week").unwrap().unwrap();
        assert!(plan.entries.is_empty());
    }

    #[test]
    fn delete_plan() {
        let db = setup_db();
        let repo = MealPlanRepository::new(&db);

        repo.get_or_create("week").unwrap();
        assert!(repo.delete_plan("week").unwrap());
        assert!(repo.get_plan("week").unwrap().is_none());
    }

    #[test]
    fn list_plans() {
        let db = setup_db();
        let repo = MealPlanRepository::new(&db);

        repo.get_or_create("week").unwrap();
        repo.get_or_create("party").unwrap();

        let plans = repo.list_plans().unwrap();
        assert_eq!(plans.len(), 2);
    }

    #[test]
    fn get_plan_recipe_slugs() {
        let db = setup_db();
        let repo = MealPlanRepository::new(&db);

        let plan_id = repo.get_or_create("week").unwrap();
        repo.add_entry(plan_id, "2025-06-02", "dinner", "chicken-adobo")
            .unwrap();
        repo.add_entry(plan_id, "2025-06-03", "dinner", "pasta-primavera")
            .unwrap();
        // Same recipe on another day
        repo.add_entry(plan_id, "2025-06-04", "lunch", "chicken-adobo")
            .unwrap();

        let slugs = repo.get_plan_recipe_slugs("week").unwrap();
        assert_eq!(slugs.len(), 2); // distinct
        assert!(slugs.contains(&"chicken-adobo".to_string()));
        assert!(slugs.contains(&"pasta-primavera".to_string()));
    }

    #[test]
    fn entry_resolves_recipe_title() {
        let db = setup_db();
        let repo = MealPlanRepository::new(&db);

        let plan_id = repo.get_or_create("week").unwrap();
        repo.add_entry(plan_id, "2025-06-02", "dinner", "chicken-adobo")
            .unwrap();

        let plan = repo.get_plan("week").unwrap().unwrap();
        assert_eq!(
            plan.entries[0].recipe_title.as_deref(),
            Some("Chicken Adobo")
        );
    }

    #[test]
    fn case_insensitive_plan_name() {
        let db = setup_db();
        let repo = MealPlanRepository::new(&db);

        repo.get_or_create("Week").unwrap();
        let plan = repo.get_plan("week").unwrap();
        assert!(plan.is_some());
    }

    #[test]
    fn weekday_helpers() {
        assert_eq!(weekday_index("monday"), Some(0));
        assert_eq!(weekday_index("sunday"), Some(6));
        assert_eq!(weekday_index("Monday"), Some(0));
        assert_eq!(weekday_index("invalid"), None);

        assert!(is_weekday("tuesday"));
        assert!(!is_weekday("brunch"));

        // weekday_to_date returns a valid ISO date
        let date = weekday_to_date("monday").unwrap();
        assert_eq!(date.len(), 10); // YYYY-MM-DD
    }

    #[test]
    fn current_week_dates_returns_seven_days() {
        let dates = current_week_dates();
        assert_eq!(dates.len(), 7);
        assert_eq!(dates[0].0, "monday");
        assert_eq!(dates[6].0, "sunday");
        // All dates are YYYY-MM-DD format
        for (_, d) in &dates {
            assert_eq!(d.len(), 10);
        }
    }
}
