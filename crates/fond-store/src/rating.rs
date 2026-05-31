use crate::{FondDb, StoreError};

/// A rating record from the database.
pub struct RatingRecord {
    pub recipe_id: i64,
    pub user_id: Option<i64>,
    pub score: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// Repository for recipe ratings (one per recipe per user, upsert).
pub struct RatingRepository<'a> {
    db: &'a FondDb,
}

impl<'a> RatingRepository<'a> {
    pub fn new(db: &'a FondDb) -> Self {
        Self { db }
    }

    /// Set or update a rating for a recipe. Upserts: one rating per (recipe, user).
    pub fn rate(&self, recipe_id: i64, user_id: Option<i64>, score: i32) -> Result<(), StoreError> {
        if !(1..=5).contains(&score) {
            return Err(StoreError::Database {
                message: format!("rating must be 1-5, got {score}"),
            });
        }

        let conn = self.db.conn();
        conn.execute(
            "INSERT INTO ratings (recipe_id, user_id, score)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(recipe_id, user_id)
             DO UPDATE SET score = ?3, updated_at = datetime('now')",
            rusqlite::params![recipe_id, user_id, score],
        )
        .map_err(|e| StoreError::Database {
            message: format!("failed to save rating: {e}"),
        })?;
        Ok(())
    }

    /// Get the current rating for a recipe by a specific user.
    pub fn get_for_recipe(
        &self,
        recipe_id: i64,
        user_id: Option<i64>,
    ) -> Result<Option<RatingRecord>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn
            .prepare(
                "SELECT recipe_id, user_id, score, created_at, updated_at
                 FROM ratings
                 WHERE recipe_id = ?1 AND user_id IS ?2",
            )
            .map_err(|e| StoreError::Database {
                message: format!("failed to prepare rating query: {e}"),
            })?;

        let result = stmt
            .query_row(rusqlite::params![recipe_id, user_id], |row| {
                Ok(RatingRecord {
                    recipe_id: row.get(0)?,
                    user_id: row.get(1)?,
                    score: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })
            .optional()
            .map_err(|e| StoreError::Database {
                message: format!("failed to query rating: {e}"),
            })?;

        Ok(result)
    }

    /// Get the average rating across all users for a recipe.
    pub fn average_for_recipe(&self, recipe_id: i64) -> Result<Option<f64>, StoreError> {
        let conn = self.db.conn();
        let avg: Option<f64> = conn
            .query_row(
                "SELECT AVG(CAST(score AS REAL)) FROM ratings WHERE recipe_id = ?1",
                [recipe_id],
                |row| row.get(0),
            )
            .map_err(|e| StoreError::Database {
                message: format!("failed to compute average rating: {e}"),
            })?;
        Ok(avg)
    }
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> (FondDb, i64) {
        let db = FondDb::open_memory().unwrap();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO recipes (slug, title, file_path) VALUES ('test', 'Test', 'test.cook')",
            [],
        )
        .unwrap();
        let recipe_id = conn.last_insert_rowid();
        (db, recipe_id)
    }

    #[test]
    fn rate_and_get() {
        let (db, recipe_id) = setup_db();
        let repo = RatingRepository::new(&db);

        repo.rate(recipe_id, Some(1), 4).unwrap();
        let rating = repo.get_for_recipe(recipe_id, Some(1)).unwrap().unwrap();
        assert_eq!(rating.score, 4);
    }

    #[test]
    fn rate_upserts() {
        let (db, recipe_id) = setup_db();
        let repo = RatingRepository::new(&db);

        repo.rate(recipe_id, Some(1), 3).unwrap();
        repo.rate(recipe_id, Some(1), 5).unwrap();

        let rating = repo.get_for_recipe(recipe_id, Some(1)).unwrap().unwrap();
        assert_eq!(rating.score, 5); // Updated, not duplicated
    }

    #[test]
    fn rate_invalid_score() {
        let (db, recipe_id) = setup_db();
        let repo = RatingRepository::new(&db);

        assert!(repo.rate(recipe_id, Some(1), 0).is_err());
        assert!(repo.rate(recipe_id, Some(1), 6).is_err());
    }

    #[test]
    fn average_rating() {
        let (db, recipe_id) = setup_db();
        let conn = db.conn();
        // Add another user
        conn.execute("INSERT INTO users (id, name) VALUES (2, 'alice')", [])
            .unwrap();

        let repo = RatingRepository::new(&db);
        repo.rate(recipe_id, Some(1), 4).unwrap();
        repo.rate(recipe_id, Some(2), 2).unwrap();

        let avg = repo.average_for_recipe(recipe_id).unwrap().unwrap();
        assert!((avg - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn no_rating_returns_none() {
        let (db, recipe_id) = setup_db();
        let repo = RatingRepository::new(&db);
        assert!(repo.get_for_recipe(recipe_id, Some(1)).unwrap().is_none());
        assert!(repo.average_for_recipe(recipe_id).unwrap().is_none());
    }
}
