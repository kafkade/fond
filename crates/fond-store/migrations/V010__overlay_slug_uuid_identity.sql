-- fond V010: anchor authored overlay identity to slug + UUIDv7
--
-- Closes the identity gap (issue #80, ADR-012). Previously notes/ratings/
-- cook_logs referenced recipes by local INTEGER rowid with ON DELETE CASCADE,
-- so `fond reindex` (DELETE FROM recipes) wiped them and rowids differed per
-- device — overlay sync impossible. Rebuild these authored-overlay tables with:
--   * UUIDv7 TEXT primary keys (id), so each row has a device-stable identity
--   * recipe_slug TEXT (NOT recipe_id rowid) as the device-stable recipe anchor
--   * no recipe FK cascade — rows survive reindex (resolved via JOIN on slug)
-- Legacy rows keep their data; recipe_slug is backfilled from recipes.slug.
-- Legacy ids use hex(randomblob) (pre-release data); new rows mint real UUIDv7.

-- ── notes ──────────────────────────────────────────────────────────
CREATE TABLE notes_new (
    id          TEXT    PRIMARY KEY,
    recipe_slug TEXT    NOT NULL,
    user_id     INTEGER REFERENCES users(id),
    body        TEXT    NOT NULL,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO notes_new (id, recipe_slug, user_id, body, created_at)
SELECT lower(hex(randomblob(16))), r.slug, n.user_id, n.body, n.created_at
FROM notes n JOIN recipes r ON r.id = n.recipe_id;

DROP TABLE notes;
ALTER TABLE notes_new RENAME TO notes;
CREATE INDEX idx_notes_recipe_user ON notes(recipe_slug, user_id, created_at);

-- ── ratings ────────────────────────────────────────────────────────
CREATE TABLE ratings_new (
    id          TEXT    PRIMARY KEY,
    recipe_slug TEXT    NOT NULL,
    user_id     INTEGER REFERENCES users(id),
    score       INTEGER NOT NULL CHECK(score >= 1 AND score <= 5),
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE(recipe_slug, user_id)
);

INSERT INTO ratings_new (id, recipe_slug, user_id, score, created_at, updated_at)
SELECT lower(hex(randomblob(16))), r.slug, rt.user_id, rt.score,
       rt.created_at, rt.updated_at
FROM ratings rt JOIN recipes r ON r.id = rt.recipe_id;

DROP TABLE ratings;
ALTER TABLE ratings_new RENAME TO ratings;
CREATE INDEX idx_ratings_score ON ratings(score);

-- ── cook_logs ──────────────────────────────────────────────────────
CREATE TABLE cook_logs_new (
    id              TEXT    PRIMARY KEY,
    recipe_slug     TEXT    NOT NULL,
    user_id         INTEGER REFERENCES users(id),
    started_at      TEXT    NOT NULL,
    finished_at     TEXT    NOT NULL,
    steps_completed INTEGER NOT NULL DEFAULT 0,
    total_steps     INTEGER NOT NULL DEFAULT 0,
    notes           TEXT    NOT NULL DEFAULT '',
    created_at      TEXT    NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO cook_logs_new (id, recipe_slug, user_id, started_at, finished_at,
                           steps_completed, total_steps, notes, created_at)
SELECT lower(hex(randomblob(16))), r.slug, cl.user_id, cl.started_at,
       cl.finished_at, cl.steps_completed, cl.total_steps, cl.notes, cl.created_at
FROM cook_logs cl JOIN recipes r ON r.id = cl.recipe_id;

DROP TABLE cook_logs;
ALTER TABLE cook_logs_new RENAME TO cook_logs;
CREATE INDEX idx_cook_logs_recipe ON cook_logs(recipe_slug);
CREATE INDEX idx_cook_logs_user ON cook_logs(user_id);
