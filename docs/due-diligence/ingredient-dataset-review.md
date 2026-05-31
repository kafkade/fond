# Due Diligence: Source & License-Check Starter Ingredient Dataset

**Date**: 2025-05-30
**Status**: Reviewed — Recommended approach identified
**Related Issue**: [#17](https://github.com/kafkade/fond/issues/17)
**Related ADRs**: ADR-007 (unit conversion), ADR-009 (pantry model)
**Roadmap References**: §3A.1 (ingredient ontology), §3A.2 (unit conversion),
Assumptions A10, A11, Failure Modes F9, F10

## Summary

fond needs an embeddable ingredient reference dataset for canonical name
matching, category/aisle mapping, density (g/mL) for volume↔weight conversion,
and allergen classification. After evaluating four candidate data sources, the
recommended approach is:

1. **USDA FoodData Central** (public domain) as the primary source for canonical
   names, categories, nutrient data, and density values
2. **FoodOn ontology** (CC BY 4.0) as a supplementary source for hierarchical
   category mapping
3. **Hand-curated seed dataset** for aisle mapping and cooking-specific aliases
   not present in scientific databases

All candidates are embeddable (no runtime API dependency) and compatible with
fond's MIT license.

## Requirements

From the issue and roadmap (§3A.1, §3A.2, §8.3):

| Requirement | Priority | Roadmap Ref |
|-------------|----------|-------------|
| Canonical ingredient names | Must-have | §3A.1 |
| Aliases/synonyms (scallion = green onion) | Must-have | §3A.1 |
| Category/aisle mapping (produce, dairy, etc.) | Must-have | §3A.1, §3A.5 |
| Density (g/mL) for volume↔weight conversion | Should-have | §3A.2, ADR-007 |
| Allergen classification | Should-have | §3A.1, F9 |
| Embeddable (no runtime API) | Must-have | Issue #17 |
| Permissive license (MIT-compatible) | Must-have | Issue #17, §9 |

---

## Candidate Evaluation

### 1. USDA FoodData Central

| Attribute | Details |
|-----------|---------|
| **License** | Public domain (US Government work, 17 U.S.C. § 105) |
| **URL** | <https://fdc.nal.usda.gov/> |
| **Format** | CSV and JSON bulk downloads, REST API |
| **Size** | Foundation Foods: ~2,600 items; SR Legacy: ~7,800 items; Branded Foods: ~400,000+ items |
| **Updated** | Continuously (Branded monthly; Foundation periodically) |

**What it provides:**

- ✅ **Canonical names**: `description` field (e.g., "Chicken, broilers or
  fryers, breast, meat only, cooked, roasted")
- ⚠️ **Aliases**: Not directly — names are scientific/formal, not
  cooking-friendly. "scallion" won't appear; "Onions, spring or scallions"
  might.
- ✅ **Categories**: `food_category` field (e.g., "Poultry Products", "Dairy and
  Egg Products", "Vegetables and Vegetable Products")
- ✅ **Density**: Derivable from `food_portion.csv` — gram weights per volume
  measure (e.g., "1 cup = 125g" for flour). Not a direct g/mL column but
  calculable.
- ⚠️ **Allergens**: Only in Branded Foods dataset (column flags like
  `allergen_contains_milk`). Foundation Foods lacks allergen fields.
- ✅ **Embeddable**: Bulk CSV download, no API dependency
- ✅ **MIT-compatible**: Public domain — no restrictions whatsoever

**Assessment**: Best source for density data and food categories. Names need
mapping to cooking-friendly aliases. Foundation Foods + SR Legacy are the right
subsets (Branded Foods is too large and product-oriented).

### 2. Open Food Facts

| Attribute | Details |
|-----------|---------|
| **License** | Open Database License (ODbL 1.0) |
| **URL** | <https://world.openfoodfacts.org/data> |
| **Format** | CSV, JSON, SQLite bulk downloads |
| **Size** | ~3 million products |
| **Updated** | Daily |

**What it provides:**

- ✅ **Canonical names**: Via `ingredients_tags`
- ✅ **Aliases**: Community-contributed synonyms across languages
- ✅ **Categories**: Hierarchical `categories_tags`
- ⚠️ **Density**: Not systematically present; some products have CIQUAL codes
  for cross-referencing
- ✅ **Allergens**: `allergens_tags` field with major allergen classification
- ✅ **Embeddable**: Bulk download available

**License concern — ODbL ShareAlike**: ODbL requires that if you publicly use
a "produced work" derived from the database, you must make the adapted database
available under ODbL. For fond's use case (embedding a curated subset):

- Using ODbL data as a **lookup reference** embedded in MIT-licensed software is
  generally acceptable — the ShareAlike applies to the *database*, not the
  software.
- However, if fond distributes a derived/adapted ingredient dataset built
  primarily from Open Food Facts, that derived dataset must also be ODbL.
- This creates a **license asymmetry** within the project: the code is MIT but
  the bundled data would be ODbL.

**Assessment**: Rich allergen and alias data, but ODbL ShareAlike creates
licensing complexity. Better as a supplementary source for allergen
cross-referencing than as the primary dataset.

### 3. FoodOn Ontology

| Attribute | Details |
|-----------|---------|
| **License** | Creative Commons Attribution 4.0 (CC BY 4.0) |
| **URL** | <https://github.com/FoodOntology/foodon> |
| **Format** | OWL (Web Ontology Language) |
| **Size** | ~30,000 terms |
| **Updated** | Periodically (academic project) |

**What it provides:**

- ✅ **Canonical names**: Formal food terms with hierarchical relationships
- ✅ **Categories**: Deep hierarchical taxonomy (food → plant food → fruit →
  citrus → lemon)
- ⚠️ **Aliases**: Some synonyms via ontology annotations
- ❌ **Density**: Not included
- ⚠️ **Allergens**: Partial — some allergen-related terms but not a systematic
  classification
- ⚠️ **Embeddable**: OWL format requires parsing; would need extraction into
  a simpler format

**Assessment**: Excellent for hierarchical category mapping (building an
aisle taxonomy). CC BY 4.0 is fully MIT-compatible (attribution only). OWL
format adds processing overhead but the extracted taxonomy is valuable.

### 4. CulinaryDB

| Attribute | Details |
|-----------|---------|
| **License** | Research use (academic publication, no explicit open license stated) |
| **URL** | <https://cosylab.iiitd.edu.in/culinarydb/> |
| **Format** | CSV (4 files in ZIP) |
| **Size** | ~1,500 ingredients across 45,000 recipes |
| **Updated** | Static (research dataset, ~2017) |

**What it provides:**

- ✅ **Canonical names**: Aliased ingredient names
- ✅ **Aliases/synonyms**: Explicit synonym column
- ✅ **Categories**: Ingredient category column
- ❌ **Density**: Not included
- ❌ **Allergens**: Not included
- ✅ **Embeddable**: Small CSV files

**License concern**: No explicit permissive license. Published as a research
dataset with downloadable CSVs, but terms of use are not clearly stated. Using
it in an MIT-licensed product would require clarification from the authors.

**Assessment**: Good alias/synonym data but missing density and allergens.
Unclear licensing makes it unsuitable without explicit permission.

---

## Comparison Matrix

| | USDA FoodData | Open Food Facts | FoodOn | CulinaryDB |
|--|---------------|----------------|--------|------------|
| **License** | ✅ Public domain | ⚠️ ODbL (ShareAlike) | ✅ CC BY 4.0 | ❌ Unclear |
| **MIT-compatible** | ✅ Yes | ⚠️ Data only | ✅ Yes | ❌ Unknown |
| **Canonical names** | ✅ Formal | ✅ Community | ✅ Formal | ✅ Aliased |
| **Cooking aliases** | ⚠️ Limited | ✅ Rich | ⚠️ Some | ✅ Good |
| **Categories** | ✅ Good | ✅ Hierarchical | ✅ Deep | ✅ Basic |
| **Aisle mapping** | ⚠️ Scientific | ⚠️ Product-based | ⚠️ Taxonomy | ❌ No |
| **Density (g/mL)** | ✅ Derivable | ⚠️ Sparse | ❌ No | ❌ No |
| **Allergens** | ⚠️ Branded only | ✅ Good | ⚠️ Partial | ❌ No |
| **Size for embedding** | ✅ Subset-able | ⚠️ Very large | ✅ Moderate | ✅ Small |
| **Maintenance** | ✅ Active (USDA) | ✅ Active | ⚠️ Academic | ❌ Static |

---

## Recommended Approach

### Primary: USDA FoodData Central (public domain)

Use **Foundation Foods** and **SR Legacy** subsets as the primary data source:

1. **Extract canonical names** from `description` field
2. **Map categories** from `food_category` to fond's aisle taxonomy
3. **Derive density values** from `food_portion.csv` (gram weight per volume
   measure)
4. **Extract allergen flags** from the Branded Foods subset where available,
   cross-referenced to Foundation Foods items

**Why**: Public domain license is maximally permissive, density data is
available, and USDA is the authoritative source for food composition data in
the US. No licensing concerns whatsoever.

### Supplementary: Hand-Curated Seed Dataset

Build a **cooking-specific mapping layer** on top of USDA data:

1. **Cooking-friendly aliases**: Map USDA's formal names to how recipes actually
   reference ingredients ("Onions, spring or scallions" → aliases: `scallion`,
   `green onion`, `spring onion`)
2. **Aisle mapping**: Map USDA food categories to grocery store sections
   (`Vegetables → Produce`, `Dairy and Egg Products → Dairy & Eggs`)
3. **Common allergen flags**: Hand-curate the FDA "Big 9" allergen
   classification for the seed set (~300-500 common cooking ingredients)
4. **Density overrides**: Curate kitchen-tested density values for the most
   common volume↔weight conversions (flour, sugar, butter, rice, etc.)

**Why**: No existing dataset maps ingredients to grocery aisles the way a
home cook thinks about them. This layer is small, maintainable, and can grow
over phases per Roadmap F10 mitigation.

### Optional Supplementary: FoodOn (CC BY 4.0)

Use FoodOn's hierarchical taxonomy to inform the category/aisle mapping:

- Extract the food hierarchy relevant to cooking ingredients
- Use as a reference for building fond's own category tree
- Attribute per CC BY 4.0 requirements

### Not Recommended: Open Food Facts, CulinaryDB

- **Open Food Facts**: ODbL ShareAlike creates licensing complexity for an
  MIT-licensed project. Use as a *reference* for building the hand-curated
  layer, but don't embed ODbL-licensed data directly.
- **CulinaryDB**: No clear license. Don't use without explicit permission.

---

## Attribution Requirements

All embedded data must be attributed in `THIRD_PARTY.md`:

| Source | License | Attribution Required |
|--------|---------|---------------------|
| USDA FoodData Central | Public domain | Recommended (not legally required) |
| FoodOn | CC BY 4.0 | Yes — must credit FoodOn and CC BY 4.0 |
| Hand-curated data | MIT (fond project) | N/A — original work |

### Example `THIRD_PARTY.md` Entry

```markdown
## Ingredient Reference Data

### USDA FoodData Central
- **Source**: U.S. Department of Agriculture, Agricultural Research Service.
  FoodData Central, 2019. fdc.nal.usda.gov.
- **License**: Public domain (U.S. Government work)
- **Usage**: Canonical ingredient names, food categories, density values,
  and nutrient data derived from Foundation Foods and SR Legacy datasets.

### FoodOn Food Ontology (if used)
- **Source**: Dooley DM, et al. FoodOn: a harmonized food ontology to increase
  global food traceability, quality control and data integration. npj Science
  of Food 2, 23 (2018).
- **License**: Creative Commons Attribution 4.0 International (CC BY 4.0)
- **Usage**: Hierarchical food category taxonomy used to inform ingredient
  classification.
```

---

## Implementation Plan

### Phase 1 (MVP — current)

fond already has a hardcoded categorizer in `fond-store/src/grocery.rs` with
~10 aisle categories and keyword-based matching. This is sufficient for MVP.

**No dataset embedding needed yet.** The current approach works for basic
grocery list categorization.

### Phase 2 (Beta)

1. Download USDA Foundation Foods + SR Legacy CSV dumps
2. Extract and subset to ~2,000-3,000 common cooking ingredients
3. Build the cooking-alias mapping layer (~500 entries to start)
4. Compute density values from portion data
5. Store as an embedded SQLite table or bundled CSV, loaded at `fond init` time
6. Create `THIRD_PARTY.md` with proper attribution

### Phase 3 (1.0)

1. Add allergen flags (FDA Big 9) for the seed set
2. Expand alias coverage based on import data (learn from parsed recipes)
3. Add substitution groups
4. Allow user-contributed additions to the dataset

---

## Conclusion

The ingredient dataset problem is **solvable with public-domain data** (USDA)
plus a hand-curated cooking-specific layer. No license conflicts with fond's
MIT license.

Key decisions:

1. ✅ **USDA FoodData Central** is the right primary source (public domain,
   density data, authoritative)
2. ✅ **FoodOn** is a useful supplementary taxonomy (CC BY 4.0, MIT-compatible)
3. ⚠️ **Open Food Facts** is useful as a reference but ODbL ShareAlike means
   don't embed directly
4. ❌ **CulinaryDB** has unclear licensing — avoid
5. ✅ **Hand-curated seed dataset** needed for cooking aliases and aisle mapping
6. ✅ Current hardcoded categorizer is sufficient for MVP; structured dataset
   is a Phase 2 deliverable

This validates Roadmap Assumption A10 ("USDA FoodData Central can be embedded
offline" — Validated) and provides a path for A11 ("Ingredient density tables
can be assembled" — now Validated in principle, pending Phase 2 implementation).
