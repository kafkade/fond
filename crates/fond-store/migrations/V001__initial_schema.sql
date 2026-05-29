-- fond V001: initial schema
--
-- Derived index tables (rebuilt by `fond reindex`):
--   recipes, recipe_ingredients, steps, cookware, tags, recipe_fts
--
-- Overlay tables (user data, NOT rebuilt by reindex):
--   users

-- ───────────────────────────────────────────────────────────
-- Derived: recipe index (from .cook files)
-- ───────────────────────────────────────────────────────────

CREATE TABLE recipes (
    id          INTEGER PRIMARY KEY,
    file_path   TEXT    NOT NULL UNIQUE,
    slug        TEXT    NOT NULL UNIQUE,
    title       TEXT    NOT NULL,
    source      TEXT    NOT NULL DEFAULT '',
    source_url  TEXT    NOT NULL DEFAULT '',
    description TEXT    NOT NULL DEFAULT '',
    recipe_yield TEXT   NOT NULL DEFAULT '',
    prep_time   TEXT    NOT NULL DEFAULT '',
    cook_time   TEXT    NOT NULL DEFAULT '',
    total_time  TEXT    NOT NULL DEFAULT '',
    servings    TEXT    NOT NULL DEFAULT '',
    content_hash TEXT   NOT NULL DEFAULT '',
    raw_source  TEXT    NOT NULL DEFAULT '',
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE recipe_ingredients (
    id          INTEGER PRIMARY KEY,
    recipe_id   INTEGER NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
    name        TEXT    NOT NULL,
    quantity    TEXT    NOT NULL DEFAULT '',
    unit        TEXT    NOT NULL DEFAULT '',
    note        TEXT    NOT NULL DEFAULT '',
    optional    INTEGER NOT NULL DEFAULT 0,
    sort_order  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE steps (
    id          INTEGER PRIMARY KEY,
    recipe_id   INTEGER NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
    section     TEXT    NOT NULL DEFAULT '',
    body        TEXT    NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE cookware (
    id          INTEGER PRIMARY KEY,
    recipe_id   INTEGER NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
    name        TEXT    NOT NULL,
    quantity    TEXT    NOT NULL DEFAULT ''
);

CREATE TABLE tags (
    name        TEXT    NOT NULL,
    recipe_id   INTEGER NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
    PRIMARY KEY (name, recipe_id)
);

-- ───────────────────────────────────────────────────────────
-- FTS5 full-text search index
-- ───────────────────────────────────────────────────────────

CREATE VIRTUAL TABLE recipe_fts USING fts5(
    title,
    ingredients_text,
    steps_text,
    tags_text
);

-- ───────────────────────────────────────────────────────────
-- Overlay: users (family-shared from day one)
-- ───────────────────────────────────────────────────────────

CREATE TABLE users (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL UNIQUE,
    is_active   INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

-- ───────────────────────────────────────────────────────────
-- Indexes
-- ───────────────────────────────────────────────────────────

CREATE INDEX idx_recipe_ingredients_recipe ON recipe_ingredients(recipe_id);
CREATE INDEX idx_steps_recipe ON steps(recipe_id);
CREATE INDEX idx_cookware_recipe ON cookware(recipe_id);
CREATE INDEX idx_tags_recipe ON tags(recipe_id);
CREATE INDEX idx_recipes_slug ON recipes(slug);
