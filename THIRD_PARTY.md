# Third-Party Data & Content Attribution

This file attributes third-party data and content bundled with or derived for
use in fond. fond's source code is licensed under MIT (see `LICENSE`). The
attributions below cover **data assets**, not code dependencies (which are
listed in `Cargo.lock`).

## USDA FoodData Central

- **Source**: U.S. Department of Agriculture, Agricultural Research Service.
  FoodData Central, 2019. <https://fdc.nal.usda.gov/>
- **License**: Public domain (U.S. Government work, 17 U.S.C. § 105)
- **Datasets used**:
  - Foundation Foods (October 2024 release)
  - SR Legacy (April 2018, final release)
- **Usage**: Per-100g macronutrient data (energy, protein, fat, carbohydrates,
  fiber, sugar, sodium) for ~7,100 common cooking ingredients. Used for
  informational nutrition estimates in recipe display.
- **File**: `data/usda/usda_nutrition_subset.csv`
- **Details**: See `docs/due-diligence/usda-fooddata-central.md`

## Ingredient Substitution Reference

- **Source**: Hand-curated by the fond project from widely published,
  non-proprietary culinary conversions (e.g., King Arthur Baking, America's
  Test Kitchen, Serious Eats) and common cooking practice. Ratios are facts,
  not copyrightable expression; all descriptive text is authored originally
  for fond.
- **License**: Original work, MIT (see `LICENSE`). Per-entry `source` fields
  cite the general authority for the conversion, not a copied text.
- **Usage**: Advisory, ranked, context-tagged (baking / sauteing / general)
  ingredient substitution suggestions surfaced by `fond substitute`. Never
  auto-applied to a `.cook` file.
- **File**: `data/substitutions/substitutions.json`
- **Details**: See `docs/due-diligence/substitution-dataset.md`
- **Status**: Seed dataset — ratios need external validation before being
  treated as authoritative (tracked via the `validation` label on
  [#78](https://github.com/kafkade/fond/issues/78)).
