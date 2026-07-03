# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Voice cook mode (Phase 8 moonshot, ADR-018): `fond cook <slug> --voice` runs a **hands-free, local-first** cook session — steps are read aloud and driven by spoken (or typed) commands so you never touch a device with messy hands. A forgiving natural-language grammar recognizes step navigation ("next", "back", "repeat", "what's next", "go to step three"), ingredient queries ("how much butter?", "list ingredients"), and timers ("set a timer for ten minutes", "start timer", "stop timer", "how much time is left") — announcing firing timers aloud. **On-device speech is the default and nothing requires the cloud** (principles #1/#6): text-to-speech uses the platform's native command (`say` on macOS, `spd-say`/`espeak` on Linux, `System.Speech` on Windows) with **graceful fallback to on-screen text** when unavailable, and speech-to-text is pluggable — pipe any recognizer's output in, or point `--listen-cmd` at a local engine (whisper.cpp, Vosk); any external/cloud tool is your explicit choice and clearly labeled in the mode banner. `--no-speak` runs text-only (still voice-driven), `--tts-cmd` overrides the speaker. Timers keep counting and announcing while the session waits for the next command (background listener thread + ticked timer loop). Completed sessions flow into the same cook-log prompt as TUI cook mode. New pure, exhaustively unit-tested `fond-voice` crate (command grammar + cook-state "brain" that generates every spoken line, decoupled from any UI or speech backend). Single recipe only. See [ADR-018](docs/adr/018-voice-cook-mode.md)
- Community recipe sharing (Phase 8 moonshot, ADR-017): opt-in, ownership-preserving sharing that never runs a server or uploads anything. `fond share export <slug>|--all` packages recipes into a portable, self-contained **`.fondshare` bundle** (a ZIP of your **verbatim `.cook` source** plus a manifest carrying attribution, `--license`, and `--author`); provenance is also **stamped losslessly into each recipe's frontmatter** (`source`, `source url`, `license`, `shared by`) via the `CookDocument` edit layer, so origin and license travel *in the file* and survive later re-export — existing metadata is never clobbered. `fond share inspect <bundle>` shows a bundle's attribution/license before you trust it. `fond share import <bundle>` feeds each recipe through the **existing review queue** (ADR-010) — nothing is written directly — preserving attribution and staying **idempotent**: re-imports skip recipes already in your library or already queued (dedup by source URL, falling back to a content digest), with `--dry-run` support. `fond share publish <bundle> [--to <dir>]` copies a bundle into a **git-friendly static index** (default `<data-dir>/shared/outbox/`) as the decentralized distribution model — **no central server, no accounts, no federation** — and requires **explicit per-action consent** (prints exactly what would leave the device and refuses without an interactive yes or `--yes`); fond performs no upload, you sync/push the folder yourself. Optional `--with-photos` includes a recipe's linked `image:` photo, copied into the review asset store on import. New pure, unit-tested `fond-import::share` module (manifest schema, deterministic digest, provenance stamping, dedup/idempotency planning). Upholds principles #1 (local-first), #2 (data ownership), and #6 (import superpower). See [ADR-017](docs/adr/017-community-sharing.md)
- Multi-recipe meal coordination (Phase 8 moonshot, ADR-016): `fond cook <a> <b> <c> --serve-at 19:00` now coordinates **several dishes onto one shared eat-time**, merging each recipe's backward-scheduled timeline into a single plan that resolves finite **kitchen-resource contention** — one oven (temperature-exclusive), burners, and cook attention. A resource-aware **backward list-scheduling heuristic** places each timed step as late as possible near the serve time and pulls dishes earlier along their slack when a resource is saturated; unavoidable clashes (e.g. two dishes needing the oven at incompatible temperatures) are **reported honestly as conflicts**, never silently fudged. Resource needs are inferred from step text, task type, and parsed oven temperatures (`425°F`, `180C`, `gas mark 6`); untimed steps stay untimed and are never resource-scheduled. Kitchen capacity is overridable with `--ovens` (default 1), `--burners` (default 4), and `--cooks` (default 1). Static output gains a coordinated Recipe/Resource table, a resource-lane summary, and a conflicts section; `--format json` serializes the full `ScheduledMeal`. The TUI cook mode drives the merged plan (steps interleaved in scheduled order, labeled by recipe and resource) and records per-recipe cook logs. A single-slug `fond cook` keeps its exact previous behavior. New `fond-timeline` modules (`resource`, `infer`, `coordinate`); all new serialized fields are `#[serde(default)]`. Also fixes a Cooklang round-trip gap where inline quantities like `350F` were dropped from step bodies. See [ADR-016](docs/adr/016-multi-recipe-coordination.md) sync your **authored overlay** — notes, ratings, cook logs, pantry, meal plans, and dietary profiles — as plain-text, diffable **JSONL sidecar files** that ride the same file-sync channel as your recipes. New `fond overlay export`, `fond overlay import`, and `fond overlay status` commands, plus **automatic overlay import on `fond reindex`** so a synced device converges in one command. Merge is **last-writer-wins** for point data (ratings, pantry, meal plans — keyed by UUIDv7/name + `updated_at`, ties broken deterministically) and **union** for append-only logs (notes, cook logs, profile sets); every last-writer-wins conflict is **reported, never silently applied**, and import is transactional and idempotent. Sidecars live under `<data-dir>/overlay/` (`users/<name>/…` for per-user data, `shared/…` for household pantry and plans); per-user records key on the user **name** so overlays resolve across devices. `cr-sqlite` remains a documented, unused fallback — last-writer-wins has proven sufficient. No new migration and no CI change; builds on the #80 identity fix (migration V010). See [ADR-015](docs/adr/015-overlay-sidecar-codec.md)
- Multi-device sync (Tier 2, ADR-012/ADR-015): sync your **authored overlay** — notes, ratings, cook logs, pantry, meal plans, and dietary profiles — as plain-text, diffable **JSONL sidecar files** that ride the same file-sync channel as your recipes. New `fond overlay export`, `fond overlay import`, and `fond overlay status` commands, plus **automatic overlay import on `fond reindex`** so a synced device converges in one command. Merge is **last-writer-wins** for point data (ratings, pantry, meal plans — keyed by UUIDv7/name + `updated_at`, ties broken deterministically) and **union** for append-only logs (notes, cook logs, profile sets); every last-writer-wins conflict is **reported, never silently applied**, and import is transactional and idempotent. Sidecars live under `<data-dir>/overlay/` (`users/<name>/…` for per-user data, `shared/…` for household pantry and plans); per-user records key on the user **name** so overlays resolve across devices. `cr-sqlite` remains a documented, unused fallback — last-writer-wins has proven sufficient. No new migration and no CI change; builds on the #80 identity fix (migration V010). See [ADR-015](docs/adr/015-overlay-sidecar-codec.md)
- Native recipe editing in the iOS/macOS apps: create, edit, and delete recipes and attach photos directly from SwiftUI, writing back to the canonical `.cook` files with a **lossless Cooklang round-trip** (an unedited recipe re-emits byte-for-byte; unknown frontmatter keys, sections, notes, and comments are preserved). Ingredients are edited inline in step text as Cooklang (`@name{qty%unit}`) with a live parsed ingredient preview; photos are stored content-addressed under `photos/` (ADR-002) and linked via an `image:` frontmatter key. Every write updates the derived SQLite index for that one recipe, and a lightweight optimistic-concurrency guard (base content hash) returns a `Conflict` when the file changed on disk since load. New `fond-domain` `CookDocument` edit layer, `fond-store` single-recipe write/reindex helpers, and `fond-ffi` write surface (`create_recipe`, `save_recipe`, `save_recipe_source`, `attach_photo`, `delete_recipe`, `get_recipe_for_edit`, `preview_ingredients`); see [ADR-011](docs/adr/011-native-apple-bridge.md)
- Multi-device sync (Tier 1, ADR-012): a "Syncing Your Recipes" user guide covering how to replicate your `.cook` files and content-addressed photos across machines with **Syncthing** (recommended), cloud folders (Dropbox/iCloud/Drive/OneDrive), or git — plus the prominent rule that the derived `fond.db` is **never** synced and each device runs `fond reindex` to rebuild its own index. Includes a two-machine end-to-end validation checklist and per-tool ignore patterns
- `fond doctor`: a new advisory command that warns when your data directory (and thus `fond.db`) appears to live inside a file-sync–managed folder (detects Syncthing `.stfolder`, Dropbox, iCloud `Mobile Documents`, Google Drive, OneDrive, and git). Never fails a command; supports `--format table|json`
- Atomic `.cook` and photo writes: every write to the source-of-truth store now goes through a shared write-temp-then-rename helper, so a file-sync daemon watching the folder can never observe a half-written file. Previously only the tag-edit path was atomic; `add`, Paprika/URL import, photo→recipe conversion, and photo blob writes are now atomic too
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
