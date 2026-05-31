use crate::{FondDb, StoreError};

/// A note record from the database.
pub struct NoteRecord {
    pub id: i64,
    pub recipe_id: i64,
    pub user_id: Option<i64>,
    pub body: String,
    pub created_at: String,
}

/// Repository for recipe notes.
pub struct NoteRepository<'a> {
    db: &'a FondDb,
}

impl<'a> NoteRepository<'a> {
    pub fn new(db: &'a FondDb) -> Self {
        Self { db }
    }

    /// Add a note to a recipe.
    pub fn add(&self, recipe_id: i64, user_id: Option<i64>, body: &str) -> Result<i64, StoreError> {
        let conn = self.db.conn();
        conn.execute(
            "INSERT INTO notes (recipe_id, user_id, body) VALUES (?1, ?2, ?3)",
            rusqlite::params![recipe_id, user_id, body],
        )
        .map_err(|e| StoreError::Database {
            message: format!("failed to save note: {e}"),
        })?;
        Ok(conn.last_insert_rowid())
    }

    /// List notes for a recipe, most recent first.
    pub fn list_for_recipe(
        &self,
        recipe_id: i64,
        user_id: Option<i64>,
    ) -> Result<Vec<NoteRecord>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, recipe_id, user_id, body, created_at
                 FROM notes
                 WHERE recipe_id = ?1 AND (?2 IS NULL OR user_id = ?2)
                 ORDER BY created_at DESC",
            )
            .map_err(|e| StoreError::Database {
                message: format!("failed to prepare notes query: {e}"),
            })?;

        let rows = stmt
            .query_map(rusqlite::params![recipe_id, user_id], |row| {
                Ok(NoteRecord {
                    id: row.get(0)?,
                    recipe_id: row.get(1)?,
                    user_id: row.get(2)?,
                    body: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| StoreError::Database {
                message: format!("failed to query notes: {e}"),
            })?;

        let mut notes = Vec::new();
        for row in rows {
            notes.push(row.map_err(|e| StoreError::Database {
                message: format!("failed to read note row: {e}"),
            })?);
        }
        Ok(notes)
    }

    /// Delete a note by ID (only if owned by the given user).
    pub fn delete(&self, note_id: i64, user_id: Option<i64>) -> Result<bool, StoreError> {
        let conn = self.db.conn();
        let affected = conn
            .execute(
                "DELETE FROM notes WHERE id = ?1 AND (?2 IS NULL OR user_id = ?2)",
                rusqlite::params![note_id, user_id],
            )
            .map_err(|e| StoreError::Database {
                message: format!("failed to delete note: {e}"),
            })?;
        Ok(affected > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_list_notes() {
        let db = FondDb::open_memory().unwrap();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO recipes (slug, title, file_path) VALUES ('test', 'Test', 'test.cook')",
            [],
        )
        .unwrap();
        let recipe_id = conn.last_insert_rowid();

        let repo = NoteRepository::new(&db);
        let id = repo.add(recipe_id, Some(1), "Great recipe!").unwrap();
        assert!(id > 0);

        repo.add(recipe_id, Some(1), "Even better the second time")
            .unwrap();

        let notes = repo.list_for_recipe(recipe_id, Some(1)).unwrap();
        assert_eq!(notes.len(), 2);
        let bodies: Vec<&str> = notes.iter().map(|n| n.body.as_str()).collect();
        assert!(bodies.contains(&"Great recipe!"));
        assert!(bodies.contains(&"Even better the second time"));
    }

    #[test]
    fn delete_note() {
        let db = FondDb::open_memory().unwrap();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO recipes (slug, title, file_path) VALUES ('test', 'Test', 'test.cook')",
            [],
        )
        .unwrap();
        let recipe_id = conn.last_insert_rowid();

        let repo = NoteRepository::new(&db);
        let id = repo.add(recipe_id, Some(1), "Delete me").unwrap();
        assert!(repo.delete(id, Some(1)).unwrap());

        let notes = repo.list_for_recipe(recipe_id, Some(1)).unwrap();
        assert!(notes.is_empty());
    }

    #[test]
    fn delete_note_wrong_user_fails() {
        let db = FondDb::open_memory().unwrap();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO recipes (slug, title, file_path) VALUES ('test', 'Test', 'test.cook')",
            [],
        )
        .unwrap();
        let recipe_id = conn.last_insert_rowid();

        // Add another user
        conn.execute("INSERT INTO users (id, name) VALUES (2, 'other')", [])
            .unwrap();

        let repo = NoteRepository::new(&db);
        let id = repo.add(recipe_id, Some(1), "My note").unwrap();
        // User 2 can't delete user 1's note
        assert!(!repo.delete(id, Some(2)).unwrap());
    }
}
