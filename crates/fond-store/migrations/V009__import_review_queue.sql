-- Overlay table for queued import drafts that need human review

CREATE TABLE import_review_queue (
    id                 TEXT PRIMARY KEY,
    source_type        TEXT    NOT NULL,
    source_name        TEXT    NOT NULL,
    asset_path         TEXT    NOT NULL DEFAULT '',
    title              TEXT    NOT NULL DEFAULT '',
    draft_cook_text    TEXT    NOT NULL,
    ocr_text           TEXT    NOT NULL DEFAULT '',
    warnings_json      TEXT    NOT NULL DEFAULT '[]',
    status             TEXT    NOT NULL DEFAULT 'pending'
                               CHECK (status IN ('pending', 'accepted', 'rejected')),
    accepted_slug      TEXT    NOT NULL DEFAULT '',
    accepted_file_path TEXT    NOT NULL DEFAULT '',
    created_at         TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at         TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_import_review_queue_status
    ON import_review_queue(status, created_at);
