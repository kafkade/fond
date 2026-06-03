-- Reference table: USDA FoodData Central nutrition data (per 100g).
-- This is bundled reference data, not user data. Populated from the
-- embedded CSV at database initialization time.
CREATE TABLE IF NOT EXISTS nutrition_facts (
    fdc_id INTEGER PRIMARY KEY,
    description TEXT NOT NULL,
    normalized_description TEXT NOT NULL,
    category TEXT NOT NULL,
    kcal REAL NOT NULL,
    protein_g REAL,
    fat_g REAL,
    carb_g REAL,
    fiber_g REAL,
    sugar_g REAL,
    sodium_mg REAL
);

CREATE INDEX IF NOT EXISTS idx_nutrition_facts_normalized
ON nutrition_facts(normalized_description);

CREATE INDEX IF NOT EXISTS idx_nutrition_facts_category
ON nutrition_facts(category);
