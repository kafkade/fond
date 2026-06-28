use serde::Serialize;

use crate::{FondDb, StoreError};

/// A recipe's cook count for the scoreboard.
#[derive(Debug, Serialize)]
pub struct MostCookedEntry {
    pub slug: String,
    pub title: String,
    pub cook_count: i64,
}

/// A recipe's rating summary for the scoreboard.
#[derive(Debug, Serialize)]
pub struct HighestRatedEntry {
    pub slug: String,
    pub title: String,
    pub avg_score: f64,
    pub rating_count: i64,
}

/// A recent activity entry for the scoreboard.
#[derive(Debug, Serialize)]
pub struct ActivityEntry {
    pub slug: String,
    pub title: String,
    pub activity_type: String,
    pub detail: String,
    pub timestamp: String,
}

/// Full scoreboard result.
#[derive(Debug, Serialize)]
pub struct Scoreboard {
    pub most_cooked: Vec<MostCookedEntry>,
    pub highest_rated: Vec<HighestRatedEntry>,
    pub recent_activity: Vec<ActivityEntry>,
}

/// Repository for scoreboard aggregation queries.
pub struct ScoreboardRepository<'a> {
    db: &'a FondDb,
}

impl<'a> ScoreboardRepository<'a> {
    pub fn new(db: &'a FondDb) -> Self {
        Self { db }
    }

    /// Get the full scoreboard with all three sections.
    pub fn scoreboard(&self, limit: usize, since: Option<&str>) -> Result<Scoreboard, StoreError> {
        Ok(Scoreboard {
            most_cooked: self.most_cooked(limit, since)?,
            highest_rated: self.highest_rated(limit, since)?,
            recent_activity: self.recent_activity(limit, since)?,
        })
    }

    /// Recipes ranked by number of cook log entries.
    pub fn most_cooked(
        &self,
        limit: usize,
        since: Option<&str>,
    ) -> Result<Vec<MostCookedEntry>, StoreError> {
        let conn = self.db.conn();
        let sql = if since.is_some() {
            "SELECT r.slug, r.title, COUNT(*) as cnt
             FROM cook_logs cl
             JOIN recipes r ON r.slug = cl.recipe_slug
             WHERE cl.finished_at >= ?1
             GROUP BY cl.recipe_slug
             ORDER BY cnt DESC
             LIMIT ?2"
        } else {
            "SELECT r.slug, r.title, COUNT(*) as cnt
             FROM cook_logs cl
             JOIN recipes r ON r.slug = cl.recipe_slug
             WHERE ?1 IS NULL
             GROUP BY cl.recipe_slug
             ORDER BY cnt DESC
             LIMIT ?2"
        };

        let mut stmt = conn.prepare(sql).map_err(|e| StoreError::Database {
            message: format!("failed to prepare most-cooked query: {e}"),
        })?;

        let rows = stmt
            .query_map(rusqlite::params![since, limit as i64], |row| {
                Ok(MostCookedEntry {
                    slug: row.get(0)?,
                    title: row.get(1)?,
                    cook_count: row.get(2)?,
                })
            })
            .map_err(|e| StoreError::Database {
                message: format!("failed to query most-cooked: {e}"),
            })?;

        collect_rows(rows)
    }

    /// Recipes ranked by average rating score.
    pub fn highest_rated(
        &self,
        limit: usize,
        since: Option<&str>,
    ) -> Result<Vec<HighestRatedEntry>, StoreError> {
        let conn = self.db.conn();
        let sql = if since.is_some() {
            "SELECT r.slug, r.title,
                    AVG(CAST(rt.score AS REAL)) as avg_score,
                    COUNT(*) as cnt
             FROM ratings rt
             JOIN recipes r ON r.slug = rt.recipe_slug
             WHERE rt.updated_at >= ?1
             GROUP BY rt.recipe_slug
             ORDER BY avg_score DESC, cnt DESC
             LIMIT ?2"
        } else {
            "SELECT r.slug, r.title,
                    AVG(CAST(rt.score AS REAL)) as avg_score,
                    COUNT(*) as cnt
             FROM ratings rt
             JOIN recipes r ON r.slug = rt.recipe_slug
             WHERE ?1 IS NULL
             GROUP BY rt.recipe_slug
             ORDER BY avg_score DESC, cnt DESC
             LIMIT ?2"
        };

        let mut stmt = conn.prepare(sql).map_err(|e| StoreError::Database {
            message: format!("failed to prepare highest-rated query: {e}"),
        })?;

        let rows = stmt
            .query_map(rusqlite::params![since, limit as i64], |row| {
                Ok(HighestRatedEntry {
                    slug: row.get(0)?,
                    title: row.get(1)?,
                    avg_score: row.get(2)?,
                    rating_count: row.get(3)?,
                })
            })
            .map_err(|e| StoreError::Database {
                message: format!("failed to query highest-rated: {e}"),
            })?;

        collect_rows(rows)
    }

    /// Recent activity across cook logs, notes, and ratings.
    pub fn recent_activity(
        &self,
        limit: usize,
        since: Option<&str>,
    ) -> Result<Vec<ActivityEntry>, StoreError> {
        let conn = self.db.conn();
        let since_clause = if since.is_some() {
            "WHERE timestamp >= ?1"
        } else {
            "WHERE ?1 IS NULL OR 1=1"
        };

        let sql = format!(
            "SELECT slug, title, activity_type, detail, timestamp FROM (
                SELECT r.slug, r.title, 'cooked' as activity_type,
                       cl.steps_completed || '/' || cl.total_steps || ' steps' as detail,
                       cl.finished_at as timestamp
                FROM cook_logs cl
                JOIN recipes r ON r.slug = cl.recipe_slug
              UNION ALL
                SELECT r.slug, r.title, 'noted' as activity_type,
                       SUBSTR(n.body, 1, 60) as detail,
                       n.created_at as timestamp
                FROM notes n
                JOIN recipes r ON r.slug = n.recipe_slug
              UNION ALL
                SELECT r.slug, r.title, 'rated' as activity_type,
                       rt.score || '/5' as detail,
                       rt.updated_at as timestamp
                FROM ratings rt
                JOIN recipes r ON r.slug = rt.recipe_slug
             )
             {since_clause}
             ORDER BY timestamp DESC
             LIMIT ?2"
        );

        let mut stmt = conn.prepare(&sql).map_err(|e| StoreError::Database {
            message: format!("failed to prepare activity query: {e}"),
        })?;

        let rows = stmt
            .query_map(rusqlite::params![since, limit as i64], |row| {
                Ok(ActivityEntry {
                    slug: row.get(0)?,
                    title: row.get(1)?,
                    activity_type: row.get(2)?,
                    detail: row.get(3)?,
                    timestamp: row.get(4)?,
                })
            })
            .map_err(|e| StoreError::Database {
                message: format!("failed to query recent activity: {e}"),
            })?;

        collect_rows(rows)
    }
}

fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
) -> Result<Vec<T>, StoreError> {
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| StoreError::Database {
            message: format!("failed to read row: {e}"),
        })?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CookLogRepository, NewCookLog, NoteRepository, RatingRepository};

    fn setup_db() -> (FondDb, String) {
        let db = FondDb::open_memory().unwrap();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO recipes (slug, title, file_path) VALUES ('adobo', 'Chicken Adobo', 'adobo.cook')",
            [],
        )
        .unwrap();
        (db, "adobo".to_string())
    }

    #[test]
    fn scoreboard_empty() {
        let (db, _) = setup_db();
        let repo = ScoreboardRepository::new(&db);
        let sb = repo.scoreboard(10, None).unwrap();
        assert!(sb.most_cooked.is_empty());
        assert!(sb.highest_rated.is_empty());
        assert!(sb.recent_activity.is_empty());
    }

    #[test]
    fn scoreboard_most_cooked() {
        let (db, slug) = setup_db();

        let log_repo = CookLogRepository::new(&db);
        for i in 0..3 {
            log_repo
                .save(&NewCookLog {
                    recipe_slug: slug.clone(),
                    user_id: Some(1),
                    started_at: format!("2025-07-{:02}T17:00:00", 20 + i),
                    finished_at: format!("2025-07-{:02}T19:00:00", 20 + i),
                    steps_completed: 5,
                    total_steps: 5,
                    notes: String::new(),
                })
                .unwrap();
        }

        let repo = ScoreboardRepository::new(&db);
        let sb = repo.scoreboard(10, None).unwrap();
        assert_eq!(sb.most_cooked.len(), 1);
        assert_eq!(sb.most_cooked[0].cook_count, 3);
        assert_eq!(sb.most_cooked[0].slug, "adobo");
    }

    #[test]
    fn scoreboard_highest_rated() {
        let (db, slug) = setup_db();

        let rating_repo = RatingRepository::new(&db);
        rating_repo.rate(&slug, Some(1), 5).unwrap();

        let repo = ScoreboardRepository::new(&db);
        let sb = repo.scoreboard(10, None).unwrap();
        assert_eq!(sb.highest_rated.len(), 1);
        assert!((sb.highest_rated[0].avg_score - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn scoreboard_recent_activity() {
        let (db, slug) = setup_db();

        let log_repo = CookLogRepository::new(&db);
        log_repo
            .save(&NewCookLog {
                recipe_slug: slug.clone(),
                user_id: Some(1),
                started_at: "2025-07-20T17:00:00".into(),
                finished_at: "2025-07-20T19:00:00".into(),
                steps_completed: 5,
                total_steps: 5,
                notes: String::new(),
            })
            .unwrap();

        let note_repo = NoteRepository::new(&db);
        note_repo.add(&slug, Some(1), "Perfect adobo!").unwrap();

        let rating_repo = RatingRepository::new(&db);
        rating_repo.rate(&slug, Some(1), 4).unwrap();

        let repo = ScoreboardRepository::new(&db);
        let sb = repo.scoreboard(10, None).unwrap();
        assert_eq!(sb.recent_activity.len(), 3);
    }

    #[test]
    fn scoreboard_since_filter() {
        let (db, slug) = setup_db();

        let log_repo = CookLogRepository::new(&db);
        log_repo
            .save(&NewCookLog {
                recipe_slug: slug.clone(),
                user_id: Some(1),
                started_at: "2025-01-01T17:00:00".into(),
                finished_at: "2025-01-01T19:00:00".into(),
                steps_completed: 5,
                total_steps: 5,
                notes: String::new(),
            })
            .unwrap();

        let repo = ScoreboardRepository::new(&db);
        // Filter to after the cook log — should be empty
        let sb = repo.scoreboard(10, Some("2025-06-01")).unwrap();
        assert!(sb.most_cooked.is_empty());
    }
}
