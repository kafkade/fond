# Due Diligence: Download & Subset USDA FoodData Central

**Date**: 2026-05-31
**Status**: Complete
**Related Issue**: [#18](https://github.com/kafkade/fond/issues/18)
**Related Documents**: [ingredient-dataset-review.md](ingredient-dataset-review.md),
ADR-007 (unit conversion), Roadmap §4.5, §8.1 (NutritionFact entity)
**Roadmap References**: Assumptions A10, A11, Decision D14

## Summary

Downloaded and subsetted USDA FoodData Central datasets (Foundation Foods +
SR Legacy) into a **7,108-item nutrition reference** suitable for embedding in
the fond binary. The subset covers common cooking ingredients with per-100g
macronutrient data (kcal, protein, fat, carbohydrates, fiber, sugar, sodium).

**Key finding**: At 169 KB compressed, the subset is well within acceptable
limits for binary embedding.

## Data Sources

### Foundation Foods (Oct 2024)

| Attribute | Value |
|-----------|-------|
| **URL** | `https://fdc.nal.usda.gov/fdc-datasets/FoodData_Central_foundation_food_csv_2024-10-31.zip` |
| **Downloaded** | 2026-05-31 |
| **ZIP size** | 3.1 MB |
| **SHA-256** | `8587f941ec252bd9aaf7b85a18a42980fb540f509b34300ed8dd4d10ca9edf44` |
| **Total foods** | 387 `foundation_food` entries (68,875 including samples) |
| **After filtering** | 335 items |

Foundation Foods is the USDA's most analytically rigorous dataset, with
multiple samples per food averaged into a single reference value. This is the
preferred source for any food that appears in both datasets.

### SR Legacy (Apr 2018, final release)

| Attribute | Value |
|-----------|-------|
| **URL** | `https://fdc.nal.usda.gov/fdc-datasets/FoodData_Central_sr_legacy_food_csv_2018-04.zip` |
| **Downloaded** | 2026-05-31 |
| **ZIP size** | 5.8 MB |
| **SHA-256** | `b80817294b8850530aaedf2e515c02593b1824f763a0ff356e5c2081643e6fd0` |
| **Total foods** | 7,793 `sr_legacy_food` entries |
| **After filtering** | 6,773 items |

SR Legacy is the final release of the USDA Standard Reference database. It
will not be updated, but remains the most comprehensive public-domain food
composition dataset for generic (non-branded) cooking ingredients.

## License

**Public domain** — U.S. Government work per 17 U.S.C. § 105.

> Works produced by officers and employees of the U.S. Government as part of
> their official duties are not subject to copyright protection in the United
> States.

No restrictions on use, modification, redistribution, or embedding. Attribution
is recommended (not legally required) as a matter of academic courtesy. See
`THIRD_PARTY.md` for the attribution entry.

## Filtering Methodology

### Category filtering

Excluded USDA food categories (not relevant to cooking ingredients):

| Category | Reason |
|----------|--------|
| Baby Foods | Not cooking ingredients |
| Fast Foods | Prepared chain restaurant items |
| Restaurant Foods | Prepared restaurant items |
| Meals, Entrees, and Side Dishes | Pre-made meals, not ingredients |
| American Indian/Alaska Native Foods | Very specific traditional items |
| Branded Food Products Database | Product-oriented, too large |
| Quality Control Materials | Lab standards, not food |

All other 21 categories are included (Dairy and Egg Products, Spices and Herbs,
Fats and Oils, Poultry Products, Vegetables, Fruits, Meats, Seafood, Grains,
Legumes, Nuts/Seeds, Baked Products, Beverages, Sweets, Snacks, Sausages,
Soups/Sauces, Breakfast Cereals, Alcoholic Beverages).

### Description keyword filtering

Items with these terms in the description are excluded:

- `babyfood`, `baby food`, `infant formula`
- `school lunch`, `hospital`
- `supplement`, `nutrition bar`, `protein bar`, `energy bar`, `meal replacement`
- `military`, `mre `, `usda commodity`
- `not further specified`

### Foundation Foods data_type filtering

Foundation Foods ZIPs contain multiple data types per food (raw samples,
market acquisitions, sub-samples). Only `foundation_food` entries (the
aggregated reference values) are kept. This reduces 68,875 raw entries to 387
meaningful foods.

## Output Format

**File**: `data/usda/usda_nutrition_subset.csv`

| Column | Type | Description |
|--------|------|-------------|
| `fdc_id` | integer | USDA FoodData Central ID (stable identifier) |
| `description` | string | Food description |
| `category` | string | USDA food category |
| `data_type` | string | `foundation` or `sr_legacy` |
| `kcal` | float | Energy per 100g (kcal) |
| `protein_g` | float | Protein per 100g (g) |
| `fat_g` | float | Total fat per 100g (g) |
| `carb_g` | float | Carbohydrate per 100g (g) |
| `fiber_g` | float | Dietary fiber per 100g (g), may be empty |
| `sugar_g` | float | Total sugars per 100g (g), may be empty |
| `sodium_mg` | float | Sodium per 100g (mg), may be empty |

### Nutrient ID mapping

| Column | Primary ID | Fallback IDs | USDA Name |
|--------|-----------|-------------|-----------|
| `kcal` | 1008 | 2047, 2048 | Energy / Energy (Atwater General/Specific Factors) |
| `protein_g` | 1003 | — | Protein |
| `fat_g` | 1004 | — | Total lipid (fat) |
| `carb_g` | 1005 | — | Carbohydrate, by difference |
| `fiber_g` | 1079 | — | Fiber, total dietary |
| `sugar_g` | 2000 | 1063 | Total Sugars (NLEA) / Sugars, Total |
| `sodium_mg` | 1093 | — | Sodium, Na |

Energy fallback is necessary because Foundation Foods often use Atwater factor
calculations (nutrient IDs 2047/2048) instead of the classic energy ID (1008).

## Size Metrics

| Metric | Value |
|--------|-------|
| Total items | 7,108 |
| Foundation Foods items | 335 |
| SR Legacy items | 6,773 |
| Raw CSV size | 905.6 KB |
| Gzipped size (level 9) | 168.9 KB |
| Compression ratio | 18.6% |

### Size assessment for binary embedding

At **169 KB compressed**, this subset is well within acceptable limits for
embedding in a Rust binary via `include_bytes!` + runtime decompression.
For reference:

- The Rust standard library's Unicode tables are ~100 KB
- A typical Rust binary is 1–10 MB
- Adding 169 KB (<1% of a typical binary) is negligible

Alternatives for delivery:
- **`include_bytes!` + gzip decompression**: Simplest, ~169 KB binary increase
- **SQLite load at `fond init`**: Parse CSV into `nutrition_facts` table at
  initialization time
- **Build-time code generation**: Convert CSV to a static Rust array (larger
  binary but zero runtime parsing)

The recommended approach is loading into SQLite at `fond init` time, consistent
with fond's storage model (ADR-002: SQLite as a derived, rebuildable index).

### Category distribution

| Category | Count |
|----------|-------|
| Beef Products | 972 |
| Vegetables and Vegetable Products | 892 |
| Baked Products | 520 |
| Lamb, Veal, and Game Products | 464 |
| Fruits and Fruit Juices | 417 |
| Poultry Products | 391 |
| Beverages | 367 |
| Sweets | 360 |
| Pork Products | 341 |
| Dairy and Egg Products | 335 |
| Legumes and Legume Products | 312 |
| Finfish and Shellfish Products | 274 |
| Soups, Sauces, and Gravies | 257 |
| Cereal Grains and Pasta | 222 |
| Fats and Oils | 217 |
| Breakfast Cereals | 195 |
| Sausages and Luncheon Meats | 177 |
| Snacks | 174 |
| Nut and Seed Products | 156 |
| Spices and Herbs | 65 |

### Nutrient coverage

| Nutrient | Coverage |
|----------|----------|
| kcal | 100.0% (7,108 / 7,108) |
| protein_g | 100.0% (7,107 / 7,108) |
| fat_g | 100.0% (7,107 / 7,108) |
| carb_g | 100.0% (7,107 / 7,108) |
| fiber_g | 91.7% (6,521 / 7,108) |
| sugar_g | 74.5% (5,295 / 7,108) |
| sodium_mg | 99.5% (7,072 / 7,108) |

The Big 4 (kcal, protein, fat, carbs) have effectively 100% coverage. Fiber
and sodium are above 90%. Sugar coverage is lower (74.5%) because many SR
Legacy entries predate the NLEA sugar reporting requirement.

## Reproduction

To regenerate the subset from scratch:

```bash
python data/scripts/subset_usda.py
```

The script downloads both USDA ZIP files into `data/usda/raw/` (gitignored),
processes them, and writes the subset to `data/usda/usda_nutrition_subset.csv`.

Raw downloads are cached — re-running the script skips already-downloaded files.

## Conclusion

This validates Roadmap Assumption A10 ("USDA FoodData Central can be embedded
offline") with concrete data:

1. ✅ **Size is reasonable**: 169 KB compressed for 7,108 foods
2. ✅ **License is clear**: Public domain, no restrictions
3. ✅ **Coverage is comprehensive**: All major cooking ingredient categories
   represented with near-complete macronutrient data
4. ✅ **Reproducible**: Script downloads and subsets deterministically

The subset is ready for integration when the `NutritionFact` entity is
implemented (Roadmap Phase 3, §4.5).
