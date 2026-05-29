# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
