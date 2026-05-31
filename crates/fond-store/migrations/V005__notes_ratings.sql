-- fond V005: notes and ratings overlay tables
--
-- Overlay tables (user data, NOT rebuilt by reindex).
-- Notes and ratings are per-user subjective data.

CREATE TABLE notes (
    id          INTEGER PRIMARY KEY,
    recipe_id   INTEGER NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
    user_id     INTEGER REFERENCES users(id),
    body        TEXT    NOT NULL,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_notes_recipe_user ON notes(recipe_id, user_id, created_at);

CREATE TABLE ratings (
    recipe_id   INTEGER NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
    user_id     INTEGER REFERENCES users(id),
    score       INTEGER NOT NULL CHECK(score >= 1 AND score <= 5),
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (recipe_id, user_id)
);

CREATE INDEX idx_ratings_score ON ratings(score);

-- Insert a default user for single-user mode.
-- Multi-user support (fond user set <name>) can be added later.
INSERT OR IGNORE INTO users (id, name) VALUES (1, 'default');
