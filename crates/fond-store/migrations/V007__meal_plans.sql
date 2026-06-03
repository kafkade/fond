-- fond V007: meal planning
--
-- Overlay tables (user data, NOT rebuilt by reindex).
-- Meal plans are household-shared (no user_id).

-- ───────────────────────────────────────────────────────────
-- Meal plans
-- ───────────────────────────────────────────────────────────

CREATE TABLE meal_plans (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL UNIQUE,
    start_date  TEXT,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

-- ───────────────────────────────────────────────────────────
-- Meal plan entries
--
-- Uses recipe_slug (not recipe_id) as the stable reference
-- so entries survive `fond reindex` which rebuilds recipe IDs.
-- ───────────────────────────────────────────────────────────

CREATE TABLE meal_plan_entries (
    id              INTEGER PRIMARY KEY,
    meal_plan_id    INTEGER NOT NULL REFERENCES meal_plans(id) ON DELETE CASCADE,
    plan_date       TEXT    NOT NULL,
    meal            TEXT    NOT NULL DEFAULT 'dinner',
    recipe_slug     TEXT    NOT NULL,
    created_at      TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE(meal_plan_id, plan_date, meal, recipe_slug)
);

CREATE INDEX idx_meal_plan_entries_plan ON meal_plan_entries(meal_plan_id);
