# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Family profiles: `fond user add|list|show|rm|set|update` manages household members with allergens and dietary preferences
- Allergen safety: `fond list --exclude-allergens` and `fond search --exclude-allergens` filter out recipes containing the active user's allergens, with substring matching against ~90 ingredient→allergen mappings
- Active user switching: `fond user set <name>` selects the current user for notes, ratings, cook logs, and allergen checks
- Meal planning: `fond plan add|show|rm|list|clear|delete` organizes recipes into named weekly meal plans with `day:meal=recipe-slug` assignment format
- Consolidated grocery lists: `fond grocery from-plan <name>` aggregates ingredients across all recipes in a meal plan, combining duplicates by name+unit, with pantry subtraction and category grouping
- `fond-scrape` crate: isolated HTTP client (`reqwest`-based) with cookie jar support and OS keychain credential storage (`keyring`) for future authenticated import sources
- USDA FoodData Central nutrition subset: 7,108 common cooking ingredients with per-100g macros (kcal, protein, fat, carbs, fiber, sugar, sodium) for future informational nutrition estimates

### Changed

- `fond import url` now uses `fond-scrape`'s built-in HTTP client instead of shelling out to `curl`, removing the external dependency

### Fixed

- Documented NYT Cooking and Cook's Illustrated/ATK scraping limitation: both services prohibit automated access in their ToS; Paprika bridge is the recommended import path
- Due diligence: USDA FoodData Central download, subsetting methodology, license verification (public domain), and binary embedding size assessment (169 KB compressed)
- Paprika import: `fond import paprika <path>` ingests `.paprikarecipes` / `.paprikarecipe` archives into `.cook` files with ingredient parsing, section headers, and provenance metadata
- URL import: `fond import url <url>` extracts recipes from any schema.org/JSON-LD page with HTML fallback, `--dry-run` preview, and URL dedup
- Pantry management: `fond pantry add|rm|list|check` tracks ingredient presence with fuzzy matching and per-recipe coverage %
- Grocery list generation: `fond grocery from-recipe <slug>` with pantry subtraction, category grouping, and `--include-pantry` flag
- JSON export: `fond export [--recipe <slug>] [--output <path>]` with schema-versioned envelope
- Paprika export: `fond export --export-format paprika --output <path>` with round-trip compatible archive format
- mdBook documentation: user guide, CLI reference, importing/exporting, pantry/grocery, architecture, and data model
- `fond-import` crate with trait-based import pipeline, Paprika adapter, and schema.org adapter
- Pantry overlay table (V003 migration) that survives `fond reindex`
- Cooking timeline engine: `fond cook <slug> --serve-at <HH:MM> --plan` computes a backward-scheduled cooking plan from a target serve time with active/passive time breakdown
- TUI cook mode: `fond cook <slug>` enters a full-screen interactive cook mode with step-by-step guidance, live countdown timers, and optional backward-scheduled timeline rail
- Live countdown timers with pause/resume, terminal bell alerts on completion, and multiple concurrent timer support
- Cook log persistence: cooking sessions are recorded with start/end time, steps completed, and total steps (V004 migration)
- `fond-timeline` crate: DAG model for recipe steps with task type classification (active/passive × prep/cook/rest), duration extraction from timer annotations and heuristic text parsing, and backward scheduling via reverse topological sort
- Due diligence: ingredient dataset sourcing review (USDA, Open Food Facts, FoodOn, CulinaryDB)
- Recipe scaling: `fond scale <slug> --to 2x` or `--servings 8` with linear quantity math, human-friendly fraction formatting, and non-linear warnings for leavening, salt, spices, and thickeners
- Recipe notes: `fond note <slug> <text>` adds per-user notes to recipes, `fond note <slug>` lists them, `--delete <id>` removes
- Recipe ratings: `fond rate <slug> <1-5>` sets a star rating (upsert per user), `fond rate <slug>` shows current rating with average
- Cooking scoreboard: `fond scoreboard` shows most cooked, highest rated, and recent activity across cook logs, notes, and ratings with `--since` date filter and `--limit` control

## [0.3.0] - 2026-05-29

### Added

- Filtered recipe listing with `fond list --tag <tag> --max-time <minutes> --cuisine <cuisine> --source <url>`
- Filtered full-text search with `fond search <query> --tag --max-time --cuisine --source`
- Tag management command: `fond tag --list`, `fond tag <slug>`, `fond tag <slug> --add <tags>`, `fond tag <slug> --remove <tags>`
- JSON output support for all new tag and filter commands
- Crate-level README files for `fond-domain`, `fond-core`, and `fond-store` (visible on crates.io)

## [0.2.2] - 2026-05-29

### Fixed

- Workspace dependency.
- Release matrix target.

## [0.2.0] - 2026-05-29

### Added

- Project scaffolding: README, CONTRIBUTING, LICENSE (MIT), CHANGELOG
- Cargo workspace with four initial crates: `fond`, `fond-domain`, `fond-core`, `fond-store`
- CLI binary with `fond init` command to bootstrap the data directory
- Platform-aware data directory resolution (XDG/Library/AppData)
- GitHub Actions CI: build, test, clippy, and fmt on Linux/macOS/Windows
- Cross-platform release workflow (5 targets with SHA-256 checksums)
- Issue templates (bug report, feature request, spike), PR template, CODEOWNERS
- Dependabot configuration for Cargo and GitHub Actions
- Cooklang recipe parser integration via `cooklang` crate v0.18 (spike #1 — GO)
- Spike report documenting parser evaluation and go/no-go decision (`docs/spikes/001-cooklang-parser.md`)
- Test corpus of 11 `.cook` recipe fixtures covering diverse cuisines and Cooklang features
- Paprika export format parser proof-of-concept with `flate2`/`zip` (spike #2 — GO)
- Spike report documenting Paprika format analysis and field mapping (`docs/spikes/002-paprika-format.md`)
- schema.org/JSON-LD recipe extraction proof-of-concept with `scraper` (spike #3 — GO)
- Spike report documenting JSON-LD patterns and field mapping (`docs/spikes/003-schema-org-extraction.md`)
- SQLite/FTS5 derive-from-files and atomic reindex proof-of-concept (spike #4 — GO)
- Full-text search across recipe titles, ingredients, steps, and tags with phrase and prefix queries
- Spike report documenting hybrid storage validation and schema design (`docs/spikes/004-sqlite-fts5.md`)
- Domain model types: `Recipe`, `RecipeIngredient`, `Step`, `Timer`, `Cookware` with serde serialization
- Cooklang-to-domain parser (`parse_cook`) with YAML array metadata, numeric value, and tag support
- `.cook` emitter (`emit_cook`) with raw-source passthrough for lossless file preservation
- Slug generation module for URL-safe recipe identifiers
- SQLite schema with refinery migrations: recipes, ingredients, steps, cookware, tags, users, FTS5 index
- Recipe repository with upsert, lookup (by id/slug/path), list, and FTS5 search
- Atomic two-phase reindex: parse all `.cook` files then rebuild derived tables in a single transaction
- CLI commands: `fond reindex`, `fond view <slug>`, `fond list`, `fond search <query>`
- CLI commands: `fond add` (ingest `.cook` file or create from title), `fond edit <slug>`, `fond rm <slug>`
- Shell completion generation via `fond completions <shell>` (bash, zsh, fish, PowerShell)
- Global `--format table|json` and `--json` flags for machine-readable output on all commands
- Tabular output with `comfy-table` for `fond list` and `fond search`
- `$VISUAL` / `$EDITOR` integration for `fond add --title` and `fond edit`
- File-first deletion in `fond rm` with confirmation prompt and `--yes` bypass
- Single-recipe deletion from the index via `delete_recipe_by_slug`
