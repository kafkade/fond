# Copilot Instructions for fond

## Project Overview

fond (French: *fond de cuisine* — the browned bits in a pan that form the foundation of a sauce; English: *fondness* — warmth, affection) is a local-first, CLI-first, Cooklang-native personal cooking and recipe management application written in Rust, designed for a household (family-shared). It treats recipes as portable plain-text `.cook` files the user owns forever, imports existing collections from Paprika / NYT Cooking / Cook's Illustrated in minutes, and helps the user actually *cook* — with realistic timelines that work backward from when they want to eat.

## Non-Negotiable Constraints

Every code contribution, architecture decision, and feature design must uphold these:

1. **Local-first** — The app must work fully offline. All core features (recipe management, search, pantry, grocery lists, cooking timelines) function without an internet connection. Network access is only for optional URL import and web scraping.
2. **Data ownership** — The user owns 100% of their data. `.cook` files are the source of truth; SQLite is a derived, rebuildable index. `fond reindex` reconstructs the DB from files. No vendor lock-in.
3. **Family-shared from day one** — The database schema includes `user_id` scoping for subjective data (notes, ratings, cook logs) from the first migration. No single-user shortcuts that require painful retrofits.
4. **Cooklang-native** — Recipes are stored as `.cook` plain-text files. The format is round-trippable — parse and re-emit without data loss. Extensions go in sidecar metadata or the SQLite overlay, never by breaking the Cooklang spec.
5. **CLI-first** — The CLI is the primary interface and a first-class product. Web, iOS, macOS, and Watch are future platforms built on the same core library.
6. **Import as a superpower** — Importing from Paprika, schema.org sites, and subscription services must be frictionless, idempotent, and lossless. Import quality is a first-impression feature.

## Architecture

Cargo workspace with 7 crates:

- `fond/` — Binary crate (CLI entry point, clap v4)
- `fond-core/` — Shared domain logic, types, traits. Pure Rust, no I/O.
- `fond-domain/` — Domain types: Recipe, Ingredient, Step, Cookware, Tag, etc.
- `fond-store/` — SQLite persistence (rusqlite), schema migrations (refinery), FTS5 search.
- `fond-import/` — Import pipeline: trait-based adapters (Paprika, schema.org, NYT, ATK).
- `fond-scrape/` — Web scraping: HTTP extraction, JSON-LD, site-specific parsers.
- `fond-timeline/` — Cooking timeline: DAG model, backward scheduling, active/passive time.

Later additions (not yet scaffolded):

- `fond-export/` — Export implementations (JSON, Paprika, plain copy)

### Storage Model (ADR-002)

```text
~/fond/
  recipes/            ← .cook files (SOURCE OF TRUTH)
    chicken-adobo.cook
    sourdough.cook
  photos/             ← content-addressed images
    a1/b2c3....jpg
  fond.db             ← SQLite index/overlay (DERIVED, rebuildable)
  config.toml
```

- `.cook` files own recipe content (principles #2, #4).
- SQLite owns derived search index (FTS5), relational overlays (ratings, pantry, plans, cook logs), and reference datasets (ingredients, nutrition).
- `fond reindex` rebuilds `fond.db` entirely from files + bundled reference data. The DB is disposable; the files are sacred.

### Key Data Model Decisions

- **Entities**: Recipe, Ingredient (canonical), RecipeIngredient, Step, Cookware, Tag, Photo, User, Note, Rating, CookLog, PantryItem, MealPlan, MealPlanEntry, GroceryList, GroceryItem, NutritionFact.
- **IDs**: UUID v7 (time-ordered, sortable).
- **Family-shared vs per-user**: Recipes, ingredients, tags, photos, meal plans, grocery lists are shared. Notes, ratings, cook logs, dietary profiles are scoped by `user_id`.
- **Pantry**: Presence-first (bool), opt-in quantity. Coverage % works from presence alone. Consumption deduction requires explicit user confirmation — never silent.
- **Timeline**: Steps modeled as a DAG with `{duration, task_type, depends_on}`. Backward scheduling from target eat-time. Untimed steps stay untimed (never fabricated).

## Conventions

- **License**: MIT — all contributions must be compatible
- **PR title format**: `feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:`
- **Commit trailer**: Include `Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>` when Copilot contributes
- **Error handling**: `thiserror` for library crates, `anyhow` for CLI binary. No `unwrap()` in user-facing paths.
- **CLI output**: Respect `NO_COLOR` env var. Support `--format table|json` for all list commands. Human tables by default via `comfy-table`/`tabled`.
- **Database migrations**: Use `refinery` with embedded SQL migrations. Migrations are idempotent.
- **Import idempotency**: Every importer stores source IDs (e.g., source URL, Paprika ID) for dedup on re-import. User edits are never overwritten.
- **Cooklang round-trip**: Any `.cook` file parsed by fond must emit back to identical content. If extensions are needed, they go in metadata or the DB overlay.

## Git Policy

**Never execute Git commands that modify history or submit code.** This includes `git commit`, `git push`, `git rebase`, `git merge`, `git reset`, `git cherry-pick`, `git revert`, and `git tag`. Read-only commands like `git status`, `git diff`, `git log`, and `git branch` are fine. The maintainer must always review and commit changes themselves.

## CI / Infrastructure Dependency

**Branch protection for this repo is managed via OpenTofu in `kafkade/github-infra` (`repos/fond/main.tf`).** The `required_status_checks` list must match the job names in `.github/workflows/validate.yml`. The current required check is `CI`. If you rename, add, or remove CI jobs that are used as merge gates, the corresponding IaC config must be updated or PRs will be permanently blocked. Always flag this when proposing workflow changes.

## Reference Documents

- Full product roadmap: `ROADMAP.md`
- Architecture Decision Records: `docs/adr/`
- CLI command reference: see ADR-004 (`docs/adr/004-cli-design.md`)
