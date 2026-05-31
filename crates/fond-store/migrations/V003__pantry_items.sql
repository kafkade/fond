-- fond V003: pantry_items overlay table
--
-- Presence-first pantry model (ADR-009): users mark items as
-- available (present=1) or unavailable (present=0). Quantity,
-- unit, expiry, and par_level are optional enhancements.
--
-- This is an OVERLAY table — NOT deleted by `fond reindex`.
-- The pantry is household-shared (no user_id column).

CREATE TABLE pantry_items (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL UNIQUE COLLATE NOCASE,
    present     INTEGER NOT NULL DEFAULT 1,
    quantity    TEXT,
    unit        TEXT,
    expiry      TEXT,
    par_level   TEXT,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_pantry_items_present ON pantry_items(present);
