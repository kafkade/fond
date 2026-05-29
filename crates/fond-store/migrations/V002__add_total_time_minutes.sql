-- fond V002: add computed total_time_minutes for efficient filtering
--
-- Derived column — populated during upsert/reindex from the
-- total_time text field (or prep_time + cook_time sum).
-- NULL means the time is unknown or unparseable.

ALTER TABLE recipes ADD COLUMN total_time_minutes INTEGER;
