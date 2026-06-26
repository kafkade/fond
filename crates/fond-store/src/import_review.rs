use rusqlite::params;
use serde::Serialize;
use uuid::Uuid;

use crate::db::FondDb;
use crate::error::StoreError;

#[derive(Debug, Clone, Serialize)]
pub struct ImportReviewRecord {
    pub id: String,
    pub source_type: String,
    pub source_name: String,
    pub asset_path: String,
    pub title: String,
    pub draft_cook_text: String,
    pub ocr_text: String,
    pub warnings: Vec<String>,
    pub status: String,
    pub accepted_slug: Option<String>,
    pub accepted_file_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct NewImportReview {
    pub source_type: String,
    pub source_name: String,
    pub asset_path: String,
    pub title: String,
    pub draft_cook_text: String,
    pub ocr_text: String,
    pub warnings: Vec<String>,
}

pub struct ImportReviewRepository<'a> {
    db: &'a FondDb,
}

impl<'a> ImportReviewRepository<'a> {
    pub fn new(db: &'a FondDb) -> Self {
        Self { db }
    }

    pub fn create(&self, draft: &NewImportReview) -> Result<ImportReviewRecord, StoreError> {
        let id = Uuid::now_v7().to_string();
        let warnings_json =
            serde_json::to_string(&draft.warnings).map_err(|e| StoreError::Database {
                message: format!("failed to serialize review warnings: {e}"),
            })?;

        self.db.conn().execute(
            "INSERT INTO import_review_queue (
                id, source_type, source_name, asset_path, title,
                draft_cook_text, ocr_text, warnings_json, status
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending')",
            params![
                id,
                draft.source_type,
                draft.source_name,
                draft.asset_path,
                draft.title,
                draft.draft_cook_text,
                draft.ocr_text,
                warnings_json,
            ],
        )?;

        self.get(&id)?.ok_or_else(|| StoreError::Database {
            message: "created review record could not be reloaded".to_string(),
        })
    }

    pub fn list_pending(&self) -> Result<Vec<ImportReviewRecord>, StoreError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, source_type, source_name, asset_path, title, draft_cook_text,
                    ocr_text, warnings_json, status, accepted_slug, accepted_file_path,
                    created_at, updated_at
             FROM import_review_queue
             WHERE status = 'pending'
             ORDER BY created_at ASC, id ASC",
        )?;

        let rows = stmt.query_map([], row_to_import_review)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    pub fn get(&self, id: &str) -> Result<Option<ImportReviewRecord>, StoreError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, source_type, source_name, asset_path, title, draft_cook_text,
                    ocr_text, warnings_json, status, accepted_slug, accepted_file_path,
                    created_at, updated_at
             FROM import_review_queue
             WHERE id = ?1",
        )?;

        Ok(stmt.query_row(params![id], row_to_import_review).ok())
    }

    pub fn update_draft(
        &self,
        id: &str,
        title: &str,
        draft_cook_text: &str,
    ) -> Result<bool, StoreError> {
        let updated = self.db.conn().execute(
            "UPDATE import_review_queue
             SET title = ?2,
                 draft_cook_text = ?3,
                 updated_at = datetime('now')
             WHERE id = ?1 AND status = 'pending'",
            params![id, title, draft_cook_text],
        )?;

        Ok(updated > 0)
    }

    pub fn mark_accepted(
        &self,
        id: &str,
        accepted_slug: &str,
        accepted_file_path: &str,
    ) -> Result<bool, StoreError> {
        let updated = self.db.conn().execute(
            "UPDATE import_review_queue
             SET status = 'accepted',
                 accepted_slug = ?2,
                 accepted_file_path = ?3,
                 updated_at = datetime('now')
             WHERE id = ?1 AND status = 'pending'",
            params![id, accepted_slug, accepted_file_path],
        )?;

        Ok(updated > 0)
    }

    pub fn mark_rejected(&self, id: &str) -> Result<bool, StoreError> {
        let updated = self.db.conn().execute(
            "UPDATE import_review_queue
             SET status = 'rejected',
                 updated_at = datetime('now')
             WHERE id = ?1 AND status = 'pending'",
            params![id],
        )?;

        Ok(updated > 0)
    }
}

fn row_to_import_review(row: &rusqlite::Row<'_>) -> rusqlite::Result<ImportReviewRecord> {
    let warnings_json: String = row.get(7)?;
    let warnings = serde_json::from_str(&warnings_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let accepted_slug: String = row.get(9)?;
    let accepted_file_path: String = row.get(10)?;

    Ok(ImportReviewRecord {
        id: row.get(0)?,
        source_type: row.get(1)?,
        source_name: row.get(2)?,
        asset_path: row.get(3)?,
        title: row.get(4)?,
        draft_cook_text: row.get(5)?,
        ocr_text: row.get(6)?,
        warnings,
        status: row.get(8)?,
        accepted_slug: if accepted_slug.is_empty() {
            None
        } else {
            Some(accepted_slug)
        },
        accepted_file_path: if accepted_file_path.is_empty() {
            None
        } else {
            Some(accepted_file_path)
        },
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_load_pending_review() {
        let db = FondDb::open_memory().unwrap();
        let repo = ImportReviewRepository::new(&db);

        let created = repo
            .create(&NewImportReview {
                source_type: "ocr-photo".to_string(),
                source_name: "card.jpg".to_string(),
                asset_path: "photos/review/test.jpg".to_string(),
                title: "Grandma Soup".to_string(),
                draft_cook_text: "---\ntitle: Grandma Soup\n---\n".to_string(),
                ocr_text: "GRANDMA SOUP".to_string(),
                warnings: vec!["Needs review".to_string()],
            })
            .unwrap();

        assert_eq!(created.status, "pending");
        assert_eq!(created.title, "Grandma Soup");
        assert_eq!(created.warnings, vec!["Needs review"]);

        let pending = repo.list_pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, created.id);
    }

    #[test]
    fn update_and_transition_review() {
        let db = FondDb::open_memory().unwrap();
        let repo = ImportReviewRepository::new(&db);

        let created = repo
            .create(&NewImportReview {
                source_type: "ocr-photo".to_string(),
                source_name: "card.jpg".to_string(),
                asset_path: "photos/review/test.jpg".to_string(),
                title: "Old Title".to_string(),
                draft_cook_text: "---\ntitle: Old Title\n---\n".to_string(),
                ocr_text: "OLD TITLE".to_string(),
                warnings: Vec::new(),
            })
            .unwrap();

        assert!(
            repo.update_draft(&created.id, "New Title", "---\ntitle: New Title\n---\n")
                .unwrap()
        );

        let updated = repo.get(&created.id).unwrap().unwrap();
        assert_eq!(updated.title, "New Title");

        assert!(
            repo.mark_accepted(&created.id, "new-title", "new-title.cook")
                .unwrap()
        );

        let accepted = repo.get(&created.id).unwrap().unwrap();
        assert_eq!(accepted.status, "accepted");
        assert_eq!(accepted.accepted_slug.as_deref(), Some("new-title"));
        assert_eq!(
            accepted.accepted_file_path.as_deref(),
            Some("new-title.cook")
        );
        assert!(repo.list_pending().unwrap().is_empty());
    }
}
