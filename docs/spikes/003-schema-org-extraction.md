# Spike 003: schema.org/JSON-LD Recipe Extraction

| Field          | Value                                |
| -------------- | ------------------------------------ |
| **Issue**      | [#3](https://github.com/kafkade/fond/issues/3) |
| **Status**     | ✅ Complete                          |
| **Verdict**    | **GO** — schema.org extraction is reliable |
| **Date**       | 2026-05-29                           |
| **Time-box**   | 1 day                                |

## Objective

Validate that schema.org/JSON-LD extraction can reliably import recipes from
food blogs, and assess what HTML fallback is needed when JSON-LD is absent.

## Go / No-Go Criteria (from Issue #3)

| Criterion                                       | Target  | Result    | Status |
| ------------------------------------------------ | ------- | --------- | ------ |
| Blogs yielding title + ingredients + steps + time | 4/5     | **6/6**   | ✅     |
| @graph wrapper (WordPress/Yoast SEO)             | Works   | Works     | ✅     |
| recipeInstructions variants handled              | 3+      | **4**     | ✅     |
| Malformed JSON-LD graceful degradation           | Yes     | Yes       | ✅     |
| HTML fallback when JSON-LD absent                | Partial | Partial   | ✅     |

## JSON-LD Patterns Tested

| #   | Pattern                        | Example Source Type         | Result |
| --- | ------------------------------ | --------------------------- | ------ |
| 1   | Direct Recipe object           | Most recipe sites           | ✅     |
| 2   | `@graph` wrapper               | WordPress + Yoast SEO       | ✅     |
| 3   | Plain string instructions      | Simpler blog plugins        | ✅     |
| 4   | `HowToSection` grouping        | Complex multi-section recipes | ✅   |
| 5   | `@type` as array `["Recipe"]`  | Some CMS platforms          | ✅     |
| 6   | Single concatenated string     | Minimal implementations     | ✅     |
| 7   | HTML-only (no JSON-LD)         | Older blogs                 | ✅ fallback |
| 8   | Malformed JSON-LD              | Broken markup               | ✅ skipped |

## recipeInstructions Variants

The `recipeInstructions` field appears in four forms across the web:

| Variant                 | Structure                          | Handling                    |
| ----------------------- | ---------------------------------- | --------------------------- |
| `HowToStep` array      | `[{"@type": "HowToStep", "text": ...}]` | Extract `text` field  |
| `HowToSection` groups   | `[{"@type": "HowToSection", "itemListElement": [...]}]` | Recurse into `itemListElement` |
| Plain string array      | `["Step 1...", "Step 2..."]`       | Use directly                |
| Single string           | `"Step 1.\nStep 2.\n..."`          | Split on newlines           |

## Field Mapping: schema.org → fond

### Cooklang Metadata (YAML Frontmatter)

| schema.org Field   | fond Mapping   | Notes                               |
| ------------------ | -------------- | ----------------------------------- |
| `name`             | `title`        | Required                            |
| `description`      | `description`  | Recipe summary                      |
| `author`           | `source`       | Person or Organization name         |
| `recipeYield`      | `servings`     | String or number                    |
| `prepTime`         | `prep time`    | ISO 8601 (PT15M)                    |
| `cookTime`         | `cook time`    | ISO 8601 (PT1H30M)                  |
| `totalTime`        | `total time`   | ISO 8601, derivable                 |
| `recipeCuisine`    | `cuisine`      | String or array                     |
| `recipeCategory`   | `category`     | Course type (Dessert, Main, etc.)   |
| `keywords`         | `tags`         | Comma-separated or array            |
| `datePublished`    | `date`         | Original publication date           |

### Cooklang Content

| schema.org Field      | fond Mapping           | Notes                          |
| --------------------- | ---------------------- | ------------------------------ |
| `recipeIngredient`    | `@ingredient{}` lines  | Already structured as array    |
| `recipeInstructions`  | Step text              | 4 variants handled (see above) |

### SQLite Overlay

| schema.org Field   | fond Mapping        | Notes                            |
| ------------------ | ------------------- | -------------------------------- |
| `aggregateRating`  | `recipe_ratings`    | Community rating for reference   |
| `nutrition`        | `recipe_nutrition`  | Structured (calories, fat, etc.) |

### External Assets

| schema.org Field | fond Mapping              | Notes                            |
| ---------------- | ------------------------- | -------------------------------- |
| `image`          | `photos/{hash}.{ext}`     | URL(s) to download               |
| `video`          | Metadata                  | Reference, not downloaded        |

### Import Provenance

| schema.org Field   | fond Mapping         | Notes                           |
| ------------------ | -------------------- | ------------------------------- |
| Source URL          | `source_url`         | Page URL where recipe was found |
| `mainEntityOfPage` | Canonical URL        | Dedup key                       |

## Key Findings

### 1. schema.org Coverage Is Excellent

All 6 synthetic blog patterns extracted complete recipe data: title,
ingredients, steps, and at least one time field. The `scraper` crate handles
HTML parsing efficiently, and `serde_json::Value` provides the flexibility
needed to handle JSON-LD's polymorphic field types.

### 2. Times Are Already Structured

Unlike Paprika's free-text times ("15 min + marinating"), schema.org uses
ISO 8601 duration format (`PT15M`, `PT1H30M`). fond-timeline can parse these
directly without heuristics.

### 3. Ingredients Are Already Structured

schema.org provides `recipeIngredient` as `Vec<String>`, already split into
individual items. This is cleaner than Paprika's newline-delimited string.

### 4. Author Field Is Polymorphic

The `author` field can be a plain string, a `Person` object, an
`Organization` object, or an array. The extractor handles all variants.

### 5. HTML Fallback Is Limited but Useful

When JSON-LD is absent, HTML scraping can extract title, ingredients, and
steps using common CSS class patterns (`.ingredients li`,
`.instructions li`). This covers older blogs that don't use structured data.
However, HTML fallback:

- Cannot extract times, cuisine, or nutrition
- Depends on class names that vary across themes
- Should feed into the review queue (ADR-010) for human verification

### 6. `@graph` Wrapper Is Common

WordPress sites with Yoast SEO plugin wrap all structured data in an
`@graph` array alongside `WebPage`, `Organization`, and `BreadcrumbList`
objects. The extractor correctly filters for `@type: "Recipe"` within
the graph.

## Risks and Mitigations

| Risk                                | Severity | Mitigation                           |
| ----------------------------------- | -------- | ------------------------------------ |
| Real sites vary from synthetic      | Medium   | Test against live sites before prod  |
| HTML fallback fragility             | Medium   | Route fallback results to review queue |
| Nested/complex @graph structures    | Low      | Recursive extraction handles nesting |
| Non-standard @type values           | Low      | Array check covers multi-type        |
| ISO 8601 parsing edge cases         | Low      | Use established duration parser crate |

## Recommendations

1. **Proceed to build `fond-scrape`** with schema.org as the primary
   extraction strategy
2. **Test against live sites** before considering production-ready — validate
   with 5+ real food blogs (Serious Eats, Budget Bytes, Bon Appétit, etc.)
3. **Implement ISO 8601 duration parser** in fond-timeline for `PT15M`-style
   times
4. **Route HTML fallback results to review queue** per ADR-010, since
   extraction quality is lower
5. **Add `reqwest` for HTTP fetching** in fond-scrape (not needed for this
   spike, which operates on HTML strings)

## Test Evidence

All 26 spike tests pass:

```text
test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Test file: `crates/fond-domain/tests/schema_org_spike.rs`

Tests cover: JSON-LD extraction (6 patterns), recipeInstructions variants
(4 types), field extraction (ingredients, times, author, cuisine, category,
yield, nutrition, rating, keywords), HTML fallback (title, ingredients,
steps, non-recipe rejection), malformed JSON-LD handling, field mapping
demonstration, and extraction quality measurement across all blog patterns.
