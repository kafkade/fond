# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Native recipe editing in the iOS/macOS apps: create, edit, and delete recipes and attach photos directly from SwiftUI, writing back to the canonical `.cook` files with a **lossless Cooklang round-trip** (an unedited recipe re-emits byte-for-byte; unknown frontmatter keys, sections, notes, and comments are preserved). Ingredients are edited inline in step text as Cooklang (`@name{qty%unit}`) with a live parsed ingredient preview; photos are stored content-addressed under `photos/` (ADR-002) and linked via an `image:` frontmatter key. Every write updates the derived SQLite index for that one recipe, and a lightweight optimistic-concurrency guard (base content hash) returns a `Conflict` when the file changed on disk since load. New `fond-domain` `CookDocument` edit layer, `fond-store` single-recipe write/reindex helpers, and `fond-ffi` write surface (`create_recipe`, `save_recipe`, `save_recipe_source`, `attach_photo`, `delete_recipe`, `get_recipe_for_edit`, `preview_ingredients`); see [ADR-011](docs/adr/011-native-apple-bridge.md)
- Rule-based non-linear recipe scaling: `fond scale <slug> --to <factor> --rules` applies a deterministic, explainable adjustment engine on top of linear scaling (ROADMAP §6.3, §3A.6). Leavening scales sub-linearly (`base × factor^0.75` when scaling up, e.g. doubling a cake's baking powder gives ×1.68, not ×2), salt/spice render as a "to-taste" band, and fat carries a pan-coating note. Every adjusted line keeps its pure-linear value as a reversible reference plus a per-line explanation of the rule and why. Cook time and pan capacity get advisory suggestions (time scales ~volume^⅔) — the recipe's stated times are never rewritten. Linear scaling remains the default; `--rules` is strictly opt-in. Ingredient classification expanded with egg, liquid, flour, and fat categories. No ML/AI — fully deterministic and offline. Exposed via `--format table|json` and the `fond-ffi` bridge
- Ingredient substitution engine: `fond substitute <ingredient>` suggests curated, ranked, sourced alternatives ("out of buttermilk? use milk + lemon juice") from a bundled reference dataset (ROADMAP §6.2) — not a generative model. Each option lists a ratio, cooking context, caveat, and source. Context-aware via `--context baking|sauteing|general`, or `--recipe <slug>` to infer the context from a recipe (e.g., surfacing baking-structure caveats). Advisory only — never auto-applied to a `.cook` file — and supports `--format table|json`. The seed dataset (`data/substitutions/substitutions.json`, ~17 ingredients) is MIT-licensed original work; ratios are pending external validation (see `docs/due-diligence/substitution-dataset.md`)
- watchOS companion app: a native Apple Watch surface for active cook timers and alerts. Starting cook mode on the phone relays the backward-scheduled timeline over WatchConnectivity, so the same live timers appear on the wrist with countdowns, a "Next up" step, and start/pause/+1 min/cancel/advance/end controls that drive the authoritative phone session
- Wrist alerts: a local notification is pre-scheduled per running timer (fires when the app is backgrounded) plus an in-app haptic the instant a countdown reaches zero — a firing timer produces a haptic alert on the Watch
- "Next up" complication / Smart Stack widget (`FondWatchWidget`): a WidgetKit accessory (inline/circular/corner/rectangular) showing the imminent step or running timer with an OS-driven live countdown, fed from a shared App Group snapshot
- `apple/Shared/` relay payload: a dependency-free `Codable` `CookSessionPayload` shared by the iOS app, Watch app, and widget; the phone lowers the FFI `ScheduledTimelineDto` + live timers into it (the Watch never links the Rust core). See [ADR-014](docs/adr/014-watch-companion-relay.md)
- iPad-optimized native layout: the SwiftUI app now uses an adaptive three-column `NavigationSplitView` (sidebar collections/tags → recipe list → detail) that expands on iPad landscape/macOS and gracefully collapses to a stack under Slide Over, Stage Manager, and on iPhone
- Side-by-side cook mode on a wide canvas: steps render beside a live panel with the plan summary and real kitchen timers (start/pause/resume/+1 min/cancel) that count down with a haptic + visual "done" alert; falls back to a single column in compact width
- Keyboard + pointer support in the native app: selection-driven lists (Magic Keyboard arrow keys, trackpad hover) and a ⌘R shortcut to start cook mode
- Inventory-based recipe suggestions: `fond suggest` ranks recipes by pantry coverage % (presence-first, per ADR-009), sorted by coverage then total time. Deterministic and fully offline — no ML. Supports `--tag`, `--cuisine`, `--max-time`, `--source`, `--max-missing` (default 2), `--limit`, and `--format table|json`, and surfaces the missing required ingredients per suggestion

## [1.0.0] - 2026-06-28

**Data model declared stable.** The `.cook` source-of-truth format and the
SQLite overlay schema (migrations V001–V010) are frozen; post-1.0 migrations
are additive and backward-compatible (see ADR-013). This satisfies the Phase 3
definition of done and unblocks the ADR-012 sync precondition.

### Added

- Native Apple bridge: new `fond-ffi` crate exposes read + cook-mode functionality (list, search, tags, recipe view, scaling, cooking timeline, reindex) to Swift via UniFFI-generated bindings over the existing Rust core
- `apple/` workspace: `build-xcframework.sh` builds `Fond.xcframework` + Swift bindings, a `FondKit` Swift package wraps them, and a multiplatform SwiftUI app (`FondApp`) runs natively on iOS and macOS
- SwiftUI proof-of-concept app: recipe browsing, full-text search, recipe detail with live ingredient scaling, and cook mode (backward-scheduled timeline with per-step start times, active/passive work, and timers)
- Self-contained sample data: bundled `.cook` recipes are seeded into app storage on first launch and indexed via `FondClient.reindex()`, reinforcing that the SQLite database is a rebuildable derivative of the source files
- Web UI: `fond serve` launches a local Axum HTTP server with HTMX-powered server-rendered pages for household members who prefer a browser over the CLI
- Recipe browsing with responsive card grid, live search-as-you-type (HTMX, 300ms debounced), and tag filtering
- Recipe detail view showing ingredients, steps, notes, average rating, and source attribution
- Meal plan list and detail views with recipe links
- Grocery list generation from the browser with pantry coverage indicators and category grouping
- Tag cloud page with recipe counts and click-to-filter navigation
- Responsive CSS for tablet and phone use in the kitchen (no JS build step, no external CSS framework)
- `fond-web` crate: Axum 0.8 router, Askama templates, `Mutex<FondDb>` shared state
- `--port` and `--bind` flags on `fond serve` with `FOND_PORT` / `FOND_BIND` env var support
- Family profiles: `fond user add|list|show|rm|set|update` manages household members with allergens and dietary preferences
- Allergen safety: `fond list --exclude-allergens` and `fond search --exclude-allergens` filter out recipes containing the active user's allergens, with substring matching against ~90 ingredient→allergen mappings
- Active user switching: `fond user set <name>` selects the current user for notes, ratings, cook logs, and allergen checks
- Meal planning: `fond plan add|show|rm|list|clear|delete` organizes recipes into named weekly meal plans with `day:meal=recipe-slug` assignment format
- Consolidated grocery lists: `fond grocery from-plan <name>` aggregates ingredients across all recipes in a meal plan, combining duplicates by name+unit, with pantry subtraction and category grouping
- `fond-scrape` crate: isolated HTTP client (`reqwest`-based) with cookie jar support and OS keychain credential storage (`keyring`) for future authenticated import sources
- USDA FoodData Central nutrition subset: 7,108 common cooking ingredients with per-100g macros (kcal, protein, fat, carbs, fiber, sugar, sodium) for future informational nutrition estimates
- Nutrition estimates: `fond nutrition <slug>` shows per-ingredient and total estimated nutrition facts using USDA data, with coverage %, confidence scoring, and aggressive rounding to prevent false precision
- OCR photo import: `fond import photo <path>` extracts local recipe images into editable Cooklang drafts with `--dry-run` preview for printed-first workflows
- Import review queue: `fond review list|show|edit|accept|reject` lets users inspect OCR drafts, fix them in `$EDITOR`, and only write `.cook` files after explicit acceptance

### Changed

- `fond import url` now uses `fond-scrape`'s built-in HTTP client instead of shelling out to `curl`, removing the external dependency

### Fixed

- Notes, ratings, and cook logs now survive `fond reindex` (and reindexing on another device): they are anchored to the recipe slug with stable UUIDv7 IDs instead of the device-specific database rowid, which previously caused them to be silently deleted on every reindex
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
