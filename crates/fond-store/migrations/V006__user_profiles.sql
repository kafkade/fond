-- fond V006: user profiles — dietary preferences and allergens
--
-- Overlay tables (user data, NOT rebuilt by reindex).
-- Dietary preferences and allergens are per-user personal data.

-- ───────────────────────────────────────────────────────────
-- Per-user allergens
-- ───────────────────────────────────────────────────────────

CREATE TABLE user_allergens (
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    allergen    TEXT    NOT NULL,
    PRIMARY KEY (user_id, allergen)
);

-- ───────────────────────────────────────────────────────────
-- Per-user dietary preferences
-- ───────────────────────────────────────────────────────────

CREATE TABLE user_dietary_prefs (
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    pref        TEXT    NOT NULL,
    PRIMARY KEY (user_id, pref)
);

-- ───────────────────────────────────────────────────────────
-- Ingredient → allergen mapping (reference data)
--
-- Uses substring-style ingredient patterns for broader matching.
-- Rebuilt by `fond reindex` from embedded seed data.
-- ───────────────────────────────────────────────────────────

CREATE TABLE ingredient_allergens (
    pattern     TEXT    NOT NULL,
    allergen    TEXT    NOT NULL,
    PRIMARY KEY (pattern, allergen)
);

-- ───────────────────────────────────────────────────────────
-- Application settings (key-value, for current-user selection etc.)
-- ───────────────────────────────────────────────────────────

CREATE TABLE app_settings (
    key         TEXT    PRIMARY KEY,
    value       TEXT    NOT NULL
);

-- Default: current user is the 'default' user from V005.
INSERT INTO app_settings (key, value) VALUES ('current_user_id', '1');
