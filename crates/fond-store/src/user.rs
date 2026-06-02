use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::{FondDb, StoreError};

/// A user profile record from the database.
#[derive(Debug, Serialize)]
pub struct UserRecord {
    pub id: i64,
    pub name: String,
    pub is_active: bool,
    pub allergens: Vec<String>,
    pub dietary_prefs: Vec<String>,
    pub created_at: String,
}

/// A detected allergen flag for a recipe ingredient.
#[derive(Debug, Serialize)]
pub struct AllergenFlag {
    pub ingredient: String,
    pub allergen: String,
}

/// Repository for user profile operations.
pub struct UserRepository<'a> {
    db: &'a FondDb,
}

impl<'a> UserRepository<'a> {
    pub fn new(db: &'a FondDb) -> Self {
        Self { db }
    }

    /// Create a new user profile.
    pub fn add(
        &self,
        name: &str,
        allergens: &[String],
        dietary_prefs: &[String],
    ) -> Result<i64, StoreError> {
        let conn = self.db.conn();

        conn.execute("INSERT INTO users (name) VALUES (?1)", params![name])
            .map_err(|e| StoreError::Database {
                message: format!("failed to create user '{name}': {e}"),
            })?;
        let user_id = conn.last_insert_rowid();

        self.set_allergens(user_id, allergens)?;
        self.set_dietary_prefs(user_id, dietary_prefs)?;

        Ok(user_id)
    }

    /// List all active users.
    pub fn list(&self) -> Result<Vec<UserRecord>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, is_active, created_at
             FROM users
             WHERE is_active = 1
             ORDER BY name",
        )?;

        let user_rows: Vec<(i64, String, bool, String)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut users = Vec::with_capacity(user_rows.len());
        for (id, name, is_active, created_at) in user_rows {
            let allergens = self.get_allergens(id)?;
            let dietary_prefs = self.get_dietary_prefs(id)?;
            users.push(UserRecord {
                id,
                name,
                is_active,
                allergens,
                dietary_prefs,
                created_at,
            });
        }
        Ok(users)
    }

    /// Get a user by name (case-insensitive).
    pub fn get_by_name(&self, name: &str) -> Result<Option<UserRecord>, StoreError> {
        let conn = self.db.conn();
        let base = conn
            .query_row(
                "SELECT id, name, is_active, created_at
                 FROM users
                 WHERE LOWER(name) = LOWER(?1) AND is_active = 1",
                params![name],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, bool>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;

        match base {
            Some((id, name, is_active, created_at)) => {
                let allergens = self.get_allergens(id)?;
                let dietary_prefs = self.get_dietary_prefs(id)?;
                Ok(Some(UserRecord {
                    id,
                    name,
                    is_active,
                    allergens,
                    dietary_prefs,
                    created_at,
                }))
            }
            None => Ok(None),
        }
    }

    /// Get the current user (from app_settings).
    pub fn get_current_user(&self) -> Result<Option<UserRecord>, StoreError> {
        let conn = self.db.conn();
        let user_id: Option<i64> = conn
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'current_user_id'",
                [],
                |row| row.get(0),
            )
            .optional()?;

        match user_id {
            Some(id) => self.get_by_id(id),
            None => Ok(None),
        }
    }

    /// Get a user by ID.
    pub fn get_by_id(&self, id: i64) -> Result<Option<UserRecord>, StoreError> {
        let conn = self.db.conn();
        let base = conn
            .query_row(
                "SELECT id, name, is_active, created_at
                 FROM users
                 WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, bool>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;

        match base {
            Some((id, name, is_active, created_at)) => {
                let allergens = self.get_allergens(id)?;
                let dietary_prefs = self.get_dietary_prefs(id)?;
                Ok(Some(UserRecord {
                    id,
                    name,
                    is_active,
                    allergens,
                    dietary_prefs,
                    created_at,
                }))
            }
            None => Ok(None),
        }
    }

    /// Get the current user's ID (from app_settings).
    pub fn get_current_user_id(&self) -> Result<Option<i64>, StoreError> {
        let conn = self.db.conn();
        let id: Option<i64> = conn
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'current_user_id'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id)
    }

    /// Switch the current active user.
    pub fn set_current_user(&self, user_id: i64) -> Result<(), StoreError> {
        let conn = self.db.conn();
        conn.execute(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('current_user_id', ?1)",
            params![user_id.to_string()],
        )?;
        Ok(())
    }

    /// Soft-delete a user (set is_active = 0). Preserves overlay data.
    pub fn deactivate(&self, user_id: i64) -> Result<bool, StoreError> {
        let conn = self.db.conn();
        let affected = conn.execute(
            "UPDATE users SET is_active = 0 WHERE id = ?1 AND is_active = 1",
            params![user_id],
        )?;
        Ok(affected > 0)
    }

    /// Replace a user's allergens.
    pub fn set_allergens(&self, user_id: i64, allergens: &[String]) -> Result<(), StoreError> {
        let conn = self.db.conn();
        conn.execute(
            "DELETE FROM user_allergens WHERE user_id = ?1",
            params![user_id],
        )?;
        for allergen in allergens {
            conn.execute(
                "INSERT INTO user_allergens (user_id, allergen) VALUES (?1, ?2)",
                params![user_id, allergen],
            )?;
        }
        Ok(())
    }

    /// Replace a user's dietary preferences.
    pub fn set_dietary_prefs(&self, user_id: i64, prefs: &[String]) -> Result<(), StoreError> {
        let conn = self.db.conn();
        conn.execute(
            "DELETE FROM user_dietary_prefs WHERE user_id = ?1",
            params![user_id],
        )?;
        for pref in prefs {
            conn.execute(
                "INSERT INTO user_dietary_prefs (user_id, pref) VALUES (?1, ?2)",
                params![user_id, pref],
            )?;
        }
        Ok(())
    }

    /// Check a recipe's ingredients against allergen mappings.
    ///
    /// Returns detected allergen flags for ingredients that match
    /// known allergen patterns. Uses substring matching against
    /// the `ingredient_allergens` reference table.
    pub fn check_recipe_allergens(&self, recipe_id: i64) -> Result<Vec<AllergenFlag>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT ri.name, ia.allergen
             FROM recipe_ingredients ri
             JOIN ingredient_allergens ia
               ON LOWER(ri.name) LIKE '%' || ia.pattern || '%'
             WHERE ri.recipe_id = ?1
             ORDER BY ia.allergen, ri.name",
        )?;

        let rows = stmt
            .query_map(params![recipe_id], |row| {
                Ok(AllergenFlag {
                    ingredient: row.get(0)?,
                    allergen: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Check recipe allergens filtered to a specific user's allergen set.
    pub fn check_recipe_allergens_for_user(
        &self,
        recipe_id: i64,
        user_id: i64,
    ) -> Result<Vec<AllergenFlag>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT ri.name, ia.allergen
             FROM recipe_ingredients ri
             JOIN ingredient_allergens ia
               ON LOWER(ri.name) LIKE '%' || ia.pattern || '%'
             JOIN user_allergens ua
               ON ua.allergen = ia.allergen AND ua.user_id = ?2
             WHERE ri.recipe_id = ?1
             ORDER BY ia.allergen, ri.name",
        )?;

        let rows = stmt
            .query_map(params![recipe_id, user_id], |row| {
                Ok(AllergenFlag {
                    ingredient: row.get(0)?,
                    allergen: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Filter out recipe IDs that contain any of a user's allergens.
    ///
    /// Returns recipe IDs from the input set that do NOT contain
    /// any allergen-flagged ingredients for the given user.
    pub fn filter_recipes_excluding_allergens(&self, user_id: i64) -> Result<Vec<i64>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT ri.recipe_id
             FROM recipe_ingredients ri
             JOIN ingredient_allergens ia
               ON LOWER(ri.name) LIKE '%' || ia.pattern || '%'
             JOIN user_allergens ua
               ON ua.allergen = ia.allergen AND ua.user_id = ?1",
        )?;

        let flagged: Vec<i64> = stmt
            .query_map(params![user_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(flagged)
    }

    /// Seed the ingredient_allergens reference table with common mappings.
    ///
    /// Uses INSERT OR IGNORE so user customizations are preserved.
    pub fn seed_ingredient_allergens(&self) -> Result<usize, StoreError> {
        let conn = self.db.conn();
        let mappings = ingredient_allergen_seed_data();

        let mut count = 0;
        for (pattern, allergen) in &mappings {
            let affected = conn.execute(
                "INSERT OR IGNORE INTO ingredient_allergens (pattern, allergen) VALUES (?1, ?2)",
                params![pattern, allergen],
            )?;
            count += affected;
        }
        Ok(count)
    }

    // ─── Internal helpers ────────────────────────────────

    fn get_allergens(&self, user_id: i64) -> Result<Vec<String>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn
            .prepare("SELECT allergen FROM user_allergens WHERE user_id = ?1 ORDER BY allergen")?;
        let rows = stmt
            .query_map(params![user_id], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(rows)
    }

    fn get_dietary_prefs(&self, user_id: i64) -> Result<Vec<String>, StoreError> {
        let conn = self.db.conn();
        let mut stmt =
            conn.prepare("SELECT pref FROM user_dietary_prefs WHERE user_id = ?1 ORDER BY pref")?;
        let rows = stmt
            .query_map(params![user_id], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(rows)
    }
}

/// Seed data for ingredient → allergen mappings.
///
/// Uses lowercase patterns for substring matching against ingredient names.
/// Coverage focuses on common ingredients and their derivatives.
fn ingredient_allergen_seed_data() -> Vec<(&'static str, &'static str)> {
    vec![
        // Dairy
        ("milk", "dairy"),
        ("cream", "dairy"),
        ("butter", "dairy"),
        ("cheese", "dairy"),
        ("yogurt", "dairy"),
        ("yoghurt", "dairy"),
        ("whey", "dairy"),
        ("casein", "dairy"),
        ("ghee", "dairy"),
        ("sour cream", "dairy"),
        ("ricotta", "dairy"),
        ("mozzarella", "dairy"),
        ("parmesan", "dairy"),
        ("cheddar", "dairy"),
        ("gruyere", "dairy"),
        ("mascarpone", "dairy"),
        ("brie", "dairy"),
        ("gouda", "dairy"),
        ("feta", "dairy"),
        ("paneer", "dairy"),
        ("custard", "dairy"),
        ("condensed milk", "dairy"),
        ("evaporated milk", "dairy"),
        ("half-and-half", "dairy"),
        ("crème", "dairy"),
        // Egg
        ("egg", "egg"),
        ("mayo", "egg"),
        ("mayonnaise", "egg"),
        ("meringue", "egg"),
        ("aioli", "egg"),
        // Gluten
        ("wheat", "gluten"),
        ("flour", "gluten"),
        ("bread", "gluten"),
        ("pasta", "gluten"),
        ("noodle", "gluten"),
        ("couscous", "gluten"),
        ("barley", "gluten"),
        ("rye", "gluten"),
        ("semolina", "gluten"),
        ("farro", "gluten"),
        ("bulgur", "gluten"),
        ("seitan", "gluten"),
        ("panko", "gluten"),
        ("breadcrumb", "gluten"),
        ("crouton", "gluten"),
        ("tortilla", "gluten"),
        ("pita", "gluten"),
        ("soy sauce", "gluten"),
        // Peanut
        ("peanut", "peanut"),
        // Tree nut
        ("almond", "tree-nut"),
        ("walnut", "tree-nut"),
        ("cashew", "tree-nut"),
        ("pecan", "tree-nut"),
        ("pistachio", "tree-nut"),
        ("hazelnut", "tree-nut"),
        ("macadamia", "tree-nut"),
        ("pine nut", "tree-nut"),
        ("brazil nut", "tree-nut"),
        ("chestnut", "tree-nut"),
        // Soy
        ("soy", "soy"),
        ("tofu", "soy"),
        ("tempeh", "soy"),
        ("edamame", "soy"),
        ("miso", "soy"),
        // Shellfish
        ("shrimp", "shellfish"),
        ("prawn", "shellfish"),
        ("crab", "shellfish"),
        ("lobster", "shellfish"),
        ("crawfish", "shellfish"),
        ("crayfish", "shellfish"),
        ("clam", "shellfish"),
        ("mussel", "shellfish"),
        ("oyster", "shellfish"),
        ("scallop", "shellfish"),
        ("squid", "shellfish"),
        ("calamari", "shellfish"),
        // Fish
        ("anchovy", "fish"),
        ("anchovies", "fish"),
        ("salmon", "fish"),
        ("tuna", "fish"),
        ("cod", "fish"),
        ("tilapia", "fish"),
        ("trout", "fish"),
        ("sardine", "fish"),
        ("mackerel", "fish"),
        ("halibut", "fish"),
        ("swordfish", "fish"),
        ("fish sauce", "fish"),
        ("fish", "fish"),
        // Sesame
        ("sesame", "sesame"),
        ("tahini", "sesame"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> FondDb {
        let db = FondDb::open_memory().unwrap();
        db
    }

    fn insert_recipe_with_ingredients(db: &FondDb, slug: &str, ingredients: &[&str]) -> i64 {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO recipes (slug, title, file_path) VALUES (?1, ?1, ?1 || '.cook')",
            params![slug],
        )
        .unwrap();
        let recipe_id = conn.last_insert_rowid();

        for (i, name) in ingredients.iter().enumerate() {
            conn.execute(
                "INSERT INTO recipe_ingredients (recipe_id, name, sort_order) VALUES (?1, ?2, ?3)",
                params![recipe_id, name, i as i32],
            )
            .unwrap();
        }
        recipe_id
    }

    #[test]
    fn add_and_get_user() {
        let db = setup_db();
        let repo = UserRepository::new(&db);

        let id = repo
            .add(
                "Sam",
                &["peanut".into(), "dairy".into()],
                &["vegetarian".into()],
            )
            .unwrap();
        assert!(id > 0);

        let user = repo.get_by_name("Sam").unwrap().unwrap();
        assert_eq!(user.name, "Sam");
        assert_eq!(user.allergens, vec!["dairy", "peanut"]); // sorted
        assert_eq!(user.dietary_prefs, vec!["vegetarian"]);
        assert!(user.is_active);
    }

    #[test]
    fn add_user_case_insensitive_lookup() {
        let db = setup_db();
        let repo = UserRepository::new(&db);

        repo.add("Alice", &[], &[]).unwrap();

        assert!(repo.get_by_name("alice").unwrap().is_some());
        assert!(repo.get_by_name("ALICE").unwrap().is_some());
    }

    #[test]
    fn add_duplicate_user_fails() {
        let db = setup_db();
        let repo = UserRepository::new(&db);

        repo.add("Sam", &[], &[]).unwrap();
        assert!(repo.add("Sam", &[], &[]).is_err());
    }

    #[test]
    fn list_users() {
        let db = setup_db();
        let repo = UserRepository::new(&db);

        // 'default' user from V005 migration
        repo.add("Alice", &["dairy".into()], &[]).unwrap();
        repo.add("Bob", &[], &["vegan".into()]).unwrap();

        let users = repo.list().unwrap();
        let names: Vec<&str> = users.iter().map(|u| u.name.as_str()).collect();
        assert!(names.contains(&"Alice"));
        assert!(names.contains(&"Bob"));
        assert!(names.contains(&"default"));
    }

    #[test]
    fn deactivate_user() {
        let db = setup_db();
        let repo = UserRepository::new(&db);

        let id = repo.add("Sam", &[], &[]).unwrap();
        assert!(repo.deactivate(id).unwrap());

        // Deactivated users don't appear in get_by_name or list
        assert!(repo.get_by_name("Sam").unwrap().is_none());
        let users = repo.list().unwrap();
        assert!(!users.iter().any(|u| u.name == "Sam"));
    }

    #[test]
    fn deactivate_preserves_data() {
        let db = setup_db();
        let repo = UserRepository::new(&db);

        let id = repo
            .add("Sam", &["dairy".into()], &["vegetarian".into()])
            .unwrap();
        repo.deactivate(id).unwrap();

        // The user still exists by ID (for overlay data preservation)
        let user = repo.get_by_id(id).unwrap().unwrap();
        assert_eq!(user.name, "Sam");
        assert!(!user.is_active);
    }

    #[test]
    fn update_allergens() {
        let db = setup_db();
        let repo = UserRepository::new(&db);

        let id = repo.add("Sam", &["dairy".into()], &[]).unwrap();
        repo.set_allergens(id, &["peanut".into(), "gluten".into()])
            .unwrap();

        let user = repo.get_by_id(id).unwrap().unwrap();
        assert_eq!(user.allergens, vec!["gluten", "peanut"]); // sorted
        assert!(!user.allergens.contains(&"dairy".to_string())); // replaced
    }

    #[test]
    fn update_dietary_prefs() {
        let db = setup_db();
        let repo = UserRepository::new(&db);

        let id = repo.add("Sam", &[], &["vegetarian".into()]).unwrap();
        repo.set_dietary_prefs(id, &["vegan".into()]).unwrap();

        let user = repo.get_by_id(id).unwrap().unwrap();
        assert_eq!(user.dietary_prefs, vec!["vegan"]);
    }

    #[test]
    fn current_user_operations() {
        let db = setup_db();
        let repo = UserRepository::new(&db);

        // Default current user is id 1 ('default' from V005)
        let current_id = repo.get_current_user_id().unwrap().unwrap();
        assert_eq!(current_id, 1);

        let id = repo.add("Sam", &[], &[]).unwrap();
        repo.set_current_user(id).unwrap();

        let current_id = repo.get_current_user_id().unwrap().unwrap();
        assert_eq!(current_id, id);

        let current = repo.get_current_user().unwrap().unwrap();
        assert_eq!(current.name, "Sam");
    }

    #[test]
    fn seed_ingredient_allergens() {
        let db = setup_db();
        let repo = UserRepository::new(&db);

        let count = repo.seed_ingredient_allergens().unwrap();
        assert!(count > 50); // We have ~90 seed entries

        // Idempotent — second call inserts nothing
        let count2 = repo.seed_ingredient_allergens().unwrap();
        assert_eq!(count2, 0);
    }

    #[test]
    fn check_recipe_allergens() {
        let db = setup_db();
        let repo = UserRepository::new(&db);
        repo.seed_ingredient_allergens().unwrap();

        let recipe_id =
            insert_recipe_with_ingredients(&db, "test", &["chicken thighs", "soy sauce", "butter"]);

        let flags = repo.check_recipe_allergens(recipe_id).unwrap();
        let allergens: Vec<&str> = flags.iter().map(|f| f.allergen.as_str()).collect();

        assert!(allergens.contains(&"dairy")); // butter
        assert!(allergens.contains(&"soy")); // soy sauce
        assert!(allergens.contains(&"gluten")); // soy sauce contains "soy sauce" which matches gluten seed
    }

    #[test]
    fn check_recipe_allergens_for_user() {
        let db = setup_db();
        let repo = UserRepository::new(&db);
        repo.seed_ingredient_allergens().unwrap();

        let user_id = repo.add("Sam", &["dairy".into()], &[]).unwrap();
        let recipe_id =
            insert_recipe_with_ingredients(&db, "test", &["chicken", "butter", "soy sauce"]);

        let flags = repo
            .check_recipe_allergens_for_user(recipe_id, user_id)
            .unwrap();

        // Only dairy should be flagged (Sam's allergen), not soy/gluten
        assert!(flags.iter().all(|f| f.allergen == "dairy"));
        assert!(!flags.is_empty());
    }

    #[test]
    fn filter_recipes_excluding_allergens() {
        let db = setup_db();
        let repo = UserRepository::new(&db);
        repo.seed_ingredient_allergens().unwrap();

        let user_id = repo.add("Sam", &["dairy".into()], &[]).unwrap();

        // Recipe with dairy
        let r1 = insert_recipe_with_ingredients(&db, "buttery-chicken", &["chicken", "butter"]);
        // Recipe without dairy
        let _r2 = insert_recipe_with_ingredients(&db, "grilled-chicken", &["chicken", "salt"]);

        let flagged = repo.filter_recipes_excluding_allergens(user_id).unwrap();
        assert!(flagged.contains(&r1));
        assert_eq!(flagged.len(), 1);
    }

    #[test]
    fn no_allergens_no_flags() {
        let db = setup_db();
        let repo = UserRepository::new(&db);
        repo.seed_ingredient_allergens().unwrap();

        let recipe_id =
            insert_recipe_with_ingredients(&db, "simple", &["salt", "pepper", "olive oil"]);

        let flags = repo.check_recipe_allergens(recipe_id).unwrap();
        assert!(flags.is_empty());
    }
}
