# Data Model

## Core Entities

### Recipe

The central entity. Stored as a `.cook` file with YAML front-matter metadata.

| Field | Type | Source |
|-------|------|--------|
| `slug` | String | Derived from title (e.g., "chicken-adobo") |
| `title` | String | YAML metadata |
| `description` | Option\<String\> | YAML metadata |
| `source` | Option\<String\> | YAML metadata |
| `source_url` | Option\<String\> | YAML metadata |
| `servings` | Option\<String\> | YAML metadata |
| `prep_time` | Option\<String\> | YAML metadata |
| `cook_time` | Option\<String\> | YAML metadata |
| `total_time` | Option\<String\> | YAML metadata |
| `ingredients` | Vec\<RecipeIngredient\> | Parsed from `.cook` body |
| `steps` | Vec\<Step\> | Parsed from `.cook` body |
| `cookware` | Vec\<Cookware\> | Parsed from `.cook` body |
| `tags` | Vec\<String\> | YAML metadata |
| `created_at` | DateTime | Database |
| `updated_at` | DateTime | Database |

### RecipeIngredient

An ingredient reference within a recipe.

| Field | Type | Description |
|-------|------|-------------|
| `name` | String | Ingredient name (e.g., "soy sauce") |
| `quantity` | Option\<String\> | Amount (e.g., "1/2") |
| `unit` | Option\<String\> | Unit of measure (e.g., "cup") |
| `note` | Option\<String\> | Additional notes |
| `optional` | bool | Whether the ingredient is optional |

### Step

A single instruction step.

| Field | Type | Description |
|-------|------|-------------|
| `section` | Option\<String\> | Section heading (e.g., "For the marinade") |
| `body` | String | Step text |
| `timers` | Vec\<Timer\> | Timer references |
| `order` | u32 | Step order |

### PantryItem

Tracks what's available in the household pantry. Stored in the SQLite overlay (not derived from files).

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID v7 | Primary key |
| `name` | String | Item name |
| `normalized_name` | String | Lowercased, trimmed for matching |
| `active` | bool | Whether the item is currently in the pantry |

## IDs

All entity IDs use **UUID v7** — time-ordered and sortable. This provides natural chronological ordering and avoids coordination issues for future multi-device sync.

## Family-Shared Design

The database schema is designed for household sharing from the start:

- **Shared**: Recipes, ingredients, tags, photos, pantry items, meal plans, grocery lists
- **Per-user** (via `user_id`): Notes, ratings, cook logs, dietary profiles

This avoids the painful retrofit of adding multi-user support later.

## SQLite Schema

The database uses [refinery](https://github.com/rust-db/refinery) for schema migrations. Key tables:

- `recipes` — indexed recipe metadata and raw source
- `tags` — recipe-tag associations
- `pantry_items` — pantry overlay (not rebuilt by reindex)
- `fts_recipes` — FTS5 virtual table for full-text search

## Stability (1.0)

As of **v1.0.0** the data model is **stable** (see [ADR-013](https://github.com/kafkade/fond/blob/main/docs/adr/013-data-model-stability.md)). The `.cook` source-of-truth format and the SQLite overlay schema (migrations V001–V010) are frozen. Two tiers are kept deliberately distinct:

- **Derived index** (`recipes`, `recipe_ingredients`, `steps`, `cookware`, `tags`, FTS5) is disposable — rebuilt from `.cook` files by `fond reindex` and never synced.
- **Authored overlays** (notes, ratings, cook logs, meal plans, pantry, profiles) are device-stable: anchored to the recipe **slug** with **UUIDv7** keys so they survive reindex.

Post-1.0 migrations are additive and backward-compatible; breaking changes would require a 2.0 with a documented migration path.
