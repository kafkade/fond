-- fond V004: cook log overlay table
--
-- Overlay table (user data, NOT rebuilt by reindex).
-- Records when a user cooked a recipe and how it went.

CREATE TABLE cook_logs (
    id              INTEGER PRIMARY KEY,
    recipe_id       INTEGER NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
    user_id         INTEGER REFERENCES users(id),
    started_at      TEXT    NOT NULL,
    finished_at     TEXT    NOT NULL,
    steps_completed INTEGER NOT NULL DEFAULT 0,
    total_steps     INTEGER NOT NULL DEFAULT 0,
    notes           TEXT    NOT NULL DEFAULT '',
    created_at      TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_cook_logs_recipe ON cook_logs(recipe_id);
CREATE INDEX idx_cook_logs_user ON cook_logs(user_id);
