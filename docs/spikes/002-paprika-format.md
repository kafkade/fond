# Spike 002: Paprika Export Format Reverse-Engineering

| Field          | Value                                |
| -------------- | ------------------------------------ |
| **Issue**      | [#2](https://github.com/kafkade/fond/issues/2) |
| **Status**     | ✅ Complete                          |
| **Verdict**    | **GO** — format is well-understood and parseable |
| **Date**       | 2026-05-28                           |
| **Time-box**   | 1 day                                |

## Objective

Reverse-engineer the Paprika `.paprikarecipes` export format, map fields to
fond's domain model, and write a parsing proof-of-concept that extracts 5+
recipes.

## Go / No-Go Criteria (from Issue #2)

| Criterion                                    | Result          | Status |
| -------------------------------------------- | --------------- | ------ |
| Extract title, ingredients, steps            | ✅ Reliable     | ✅     |
| Extract source URL                           | ✅ Reliable     | ✅     |
| Extract photos                               | ✅ Base64 field | ✅     |
| Format encrypted or legally restricted       | ❌ Neither      | ✅     |
| Parse 5+ recipes in proof-of-concept         | 6 synthetic + 500 batch | ✅ |

## Format Analysis

### File Types

| Extension           | Structure                         | Compression |
| ------------------- | --------------------------------- | ----------- |
| `.paprikarecipe`    | Single gzip-compressed JSON       | gzip        |
| `.paprikarecipes`   | ZIP archive of gzipped JSON files | ZIP + gzip  |

### Archive Layout

A `.paprikarecipes` file is a standard ZIP archive. Each entry is named by
UUID (e.g., `A1B2C3D4.paprikarecipe`) and contains a single recipe as
gzip-compressed JSON. No encryption, no DRM.

### Recipe JSON Schema

All fields are optional except `name`. The schema is permissive — unknown
fields are preserved for forward compatibility.

```json
{
  "name": "Recipe Name",
  "uid": "UUID-string",
  "description": "Short description",
  "ingredients": "newline-delimited string",
  "directions": "newline-delimited string",
  "notes": "free text",
  "servings": "string",
  "prep_time": "string (e.g. '15 min')",
  "cook_time": "string (e.g. '30 min')",
  "total_time": "string (e.g. '45 min')",
  "source": "source name",
  "source_url": "URL string",
  "categories": ["array", "of", "strings"],
  "nutrition": "free text",
  "rating": 5,
  "difficulty": "string",
  "yield": "string",
  "on_favorites": true,
  "created": "timestamp string",
  "photo": "base64-encoded image data",
  "photo_hash": "hash string",
  "photo_url": "URL string",
  "image_url": "URL string",
  "hash": "internal hash",
  "scale": null
}
```

## Field Mapping: Paprika → fond

### Cooklang Metadata (YAML Frontmatter)

| Paprika Field  | fond Mapping              | Notes                              |
| -------------- | ------------------------- | ---------------------------------- |
| `name`         | `title`                   | Required                           |
| `source`       | `source`                  | Human-readable source name         |
| `source_url`   | `source_url`              | Original URL                       |
| `servings`     | `servings`                | String, may need normalization     |
| `prep_time`    | `prep time`               | Free text, not structured duration |
| `cook_time`    | `cook time`               | Free text                          |
| `total_time`   | `total time`              | Free text, derivable               |
| `categories`   | `tags`                    | Join as comma-separated            |
| `difficulty`   | `difficulty`              | Custom metadata key                |

### Cooklang Content

| Paprika Field  | fond Mapping              | Notes                              |
| -------------- | ------------------------- | ---------------------------------- |
| `ingredients`  | `@ingredient{}` lines     | Split on `\n`, parse quantities    |
| `directions`   | Step text                 | Split on `\n`, each = one step     |
| `notes`        | `> note` lines or sidecar | Preserve as Cooklang notes         |

### SQLite Overlay (fond's DB, not in `.cook` files)

| Paprika Field  | fond Mapping              | Notes                              |
| -------------- | ------------------------- | ---------------------------------- |
| `rating`       | `user_ratings` table      | 1–5 integer                        |
| `on_favorites` | `user_favorites`          | Boolean                            |
| `nutrition`    | `recipe_nutrition`        | Free text, parse later             |

### Import Provenance

| Paprika Field  | fond Mapping              | Notes                              |
| -------------- | ------------------------- | ---------------------------------- |
| `uid`          | `import_source_id`        | For idempotent re-import           |
| `created`      | `import_timestamp`        | Original creation date             |
| `hash`         | `import_hash`             | Detect drift between imports       |

### External Assets

| Paprika Field  | fond Mapping              | Notes                              |
| -------------- | ------------------------- | ---------------------------------- |
| `photo`        | `photos/{hash}.{ext}`     | Content-addressed file storage     |
| `photo_hash`   | Dedup key                 | Skip re-extracting identical photos|
| `photo_url`    | Metadata                  | Fallback for re-fetch              |
| `image_url`    | Metadata                  | Alternative image source           |

### Unmapped / Preserved Raw

| Paprika Field     | Strategy                           |
| ----------------- | ---------------------------------- |
| `scale`           | Preserve in raw import blob        |
| `yield`           | Map to `servings` if no `servings` |
| `description`     | Append to notes or metadata        |
| Unknown fields    | Captured via `serde(flatten)` into `extra` map, stored in import provenance for lossless migration |

## Key Findings

### 1. Format Is Straightforward

Standard ZIP + gzip, no encryption. Any language with zip/gzip support can
read it. Rust crates `zip` + `flate2` handle it cleanly.

### 2. Ingredients and Directions Are Plain Text

Paprika stores ingredients and directions as newline-delimited strings, not
structured data. The fond importer will need to:

- Split on newlines
- Parse quantities from ingredient lines (e.g., "2 lbs chicken thighs")
- Detect section headers (e.g., "For the Sauce:") — these appear as lines
  in the ingredients/directions text, separated by blank lines

### 3. Photos Can Be Large

The `photo` field contains base64-encoded image data inline in the JSON.
For a collection of 500+ recipes with photos, this could mean gigabytes of
data. The production importer should:

- Stream photo extraction (don't hold all photos in memory)
- Store photos as content-addressed files under `photos/`
- Use `photo_hash` for deduplication

### 4. Time Fields Are Free Text

`prep_time`, `cook_time`, and `total_time` are unstructured strings
(e.g., "15 min + marinating", "1 hr 5 min"). The importer will need a
lightweight parser to extract structured durations for fond's timeline
engine.

### 5. One Bad Recipe Should Not Block the Batch

The proof-of-concept demonstrates graceful degradation: if one entry in the
archive has corrupt JSON, the parser skips it and continues with the rest.
This aligns with ADR-010's pipeline design (clean recipes write immediately,
ambiguous ones go to review queue).

### 6. Unknown Fields Are Preserved

Using `#[serde(flatten)]` captures any fields not in our struct into a
generic map. This ensures we never silently drop data from future Paprika
versions.

## Performance

| Metric          | Result              |
| --------------- | ------------------- |
| 500 recipes     | ~50 ms parse time   |
| Throughput      | ~10,000 recipes/sec |
| Archive size    | ~164 KB (no photos) |

The <10-minute target for 500 recipes is trivially met for parsing. Photo
extraction will be the bottleneck in production, not JSON parsing.

## Risks and Mitigations

| Risk                              | Severity | Mitigation                              |
| --------------------------------- | -------- | --------------------------------------- |
| Paprika format changes over time  | Medium   | Unknown fields preserved; version check |
| Photo memory pressure             | Medium   | Stream extraction, don't eager-decode   |
| Ingredient quantity parsing       | Low      | Heuristic parser, review queue fallback |
| Time string parsing               | Low      | Regex-based duration extractor          |
| Synthetic-only test fixtures      | Medium   | Validate with real export before production (user has exported their collection) |

## Recommendations

1. **Proceed to build `fond-import`** with Paprika as the first adapter
2. **Validate with real export** — test against the user's actual Paprika
   collection before considering the importer production-ready
3. **Implement ingredient line parser** — extract quantities, units, and
   names from Paprika's plain-text ingredient lines
4. **Defer photo extraction** — implement as a second pass, stream-based
5. **Add time string parser** — lightweight duration extractor for timeline
   engine integration

## Test Evidence

All 24 spike tests pass:

```text
test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Test file: `crates/fond-domain/tests/paprika_spike.rs`

Tests cover: single recipe parsing, archive parsing, field extraction (9
field types), edge cases (Unicode, null, empty, unknown fields, section
headers), archive edge cases (directories, non-recipe files, corrupt
entries), performance (500 recipes), and field mapping demonstration.
