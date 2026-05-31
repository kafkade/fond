use crate::{FondDb, StoreError};

/// A recorded cooking session.
pub struct CookLogRecord {
    pub id: i64,
    pub recipe_id: i64,
    pub user_id: Option<i64>,
    pub started_at: String,
    pub finished_at: String,
    pub steps_completed: i32,
    pub total_steps: i32,
    pub notes: String,
    pub created_at: String,
}

/// Data for creating a new cook log entry.
pub struct NewCookLog {
    pub recipe_id: i64,
    pub user_id: Option<i64>,
    pub started_at: String,
    pub finished_at: String,
    pub steps_completed: i32,
    pub total_steps: i32,
    pub notes: String,
}

/// Repository for cook log persistence.
pub struct CookLogRepository<'a> {
    db: &'a FondDb,
}

impl<'a> CookLogRepository<'a> {
    pub fn new(db: &'a FondDb) -> Self {
        Self { db }
    }

    /// Save a new cook log entry.
    pub fn save(&self, entry: &NewCookLog) -> Result<i64, StoreError> {
        let conn = self.db.conn();
        conn.execute(
            "INSERT INTO cook_logs (recipe_id, user_id, started_at, finished_at, steps_completed, total_steps, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                entry.recipe_id,
                entry.user_id,
                entry.started_at,
                entry.finished_at,
                entry.steps_completed,
                entry.total_steps,
                entry.notes,
            ],
        )
        .map_err(|e| StoreError::Database {
            message: format!("failed to save cook log: {e}"),
        })?;

        Ok(conn.last_insert_rowid())
    }

    /// List cook logs for a given recipe, most recent first.
    pub fn list_for_recipe(&self, recipe_id: i64) -> Result<Vec<CookLogRecord>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, recipe_id, user_id, started_at, finished_at,
                        steps_completed, total_steps, notes, created_at
                 FROM cook_logs
                 WHERE recipe_id = ?1
                 ORDER BY started_at DESC",
            )
            .map_err(|e| StoreError::Database {
                message: format!("failed to prepare cook log query: {e}"),
            })?;

        let rows = stmt
            .query_map([recipe_id], |row| {
                Ok(CookLogRecord {
                    id: row.get(0)?,
                    recipe_id: row.get(1)?,
                    user_id: row.get(2)?,
                    started_at: row.get(3)?,
                    finished_at: row.get(4)?,
                    steps_completed: row.get(5)?,
                    total_steps: row.get(6)?,
                    notes: row.get(7)?,
                    created_at: row.get(8)?,
                })
            })
            .map_err(|e| StoreError::Database {
                message: format!("failed to query cook logs: {e}"),
            })?;

        let mut logs = Vec::new();
        for row in rows {
            logs.push(row.map_err(|e| StoreError::Database {
                message: format!("failed to read cook log row: {e}"),
            })?);
        }
        Ok(logs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_list_cook_log() {
        let db = FondDb::open_memory().unwrap();
        let conn = db.conn();

        // Insert a test recipe
        conn.execute(
            "INSERT INTO recipes (slug, title, file_path) VALUES ('test', 'Test Recipe', 'test.cook')",
            [],
        )
        .unwrap();
        let recipe_id = conn.last_insert_rowid();

        let repo = CookLogRepository::new(&db);
        let entry = NewCookLog {
            recipe_id,
            user_id: None,
            started_at: "2025-07-20T17:15:00".into(),
            finished_at: "2025-07-20T19:00:00".into(),
            steps_completed: 5,
            total_steps: 6,
            notes: "Turned out great!".into(),
        };

        let id = repo.save(&entry).unwrap();
        assert!(id > 0);

        let logs = repo.list_for_recipe(recipe_id).unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].steps_completed, 5);
        assert_eq!(logs[0].total_steps, 6);
        assert_eq!(logs[0].notes, "Turned out great!");
    }

    #[test]
    fn cook_logs_survive_empty_list() {
        let db = FondDb::open_memory().unwrap();
        let repo = CookLogRepository::new(&db);
        let logs = repo.list_for_recipe(999).unwrap();
        assert!(logs.is_empty());
    }
}
