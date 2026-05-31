# Architecture Overview

fond is built as a Cargo workspace with purpose-scoped crates.

## Crate Structure

```
fond/                   ← workspace root
├── crates/
│   ├── fond/           ← Binary crate (CLI entry point, clap v4)
│   ├── fond-core/      ← Shared domain logic, types, traits
│   ├── fond-domain/    ← Domain types: Recipe, Ingredient, Step, etc.
│   ├── fond-store/     ← SQLite persistence, migrations, FTS5 search
│   ├── fond-import/    ← Import pipeline: Paprika, schema.org adapters
│   ├── fond-scrape/    ← Web scraping (future)
│   └── fond-timeline/  ← Cooking timeline: DAG model (future)
```

### Dependency Flow

```
fond (CLI binary)
  ├── fond-core
  ├── fond-domain
  ├── fond-store
  └── fond-import
        └── fond-domain
```

Import crates (`fond-import`) are I/O-free for persistence — they parse external formats and produce domain types. The CLI binary handles file writing and database indexing.

## Storage Model

```
~/fond/
  recipes/            ← .cook files (SOURCE OF TRUTH)
    chicken-adobo.cook
    sourdough.cook
  photos/             ← content-addressed images
    a1/b2c3....jpg
  fond.db             ← SQLite index/overlay (DERIVED, rebuildable)
  config.toml
```

### Source of Truth

`.cook` files are the source of truth. SQLite is a derived, rebuildable index. `fond reindex` reconstructs the database entirely from the files on disk.

### What Lives Where

| Data | Storage | Why |
|------|---------|-----|
| Recipe content | `.cook` files | Portable, user-owned, editable |
| Search index (FTS5) | SQLite | Fast full-text search |
| Tags | SQLite (derived from `.cook` metadata) | Queryable |
| Pantry items | SQLite (overlay) | Not derivable from files |
| Grocery lists | Computed at runtime | Ephemeral |
| Ratings, notes, cook logs | SQLite (overlay, future) | Per-user, subjective |

### Overlay vs. Derived

- **Derived data** (search index, tags, recipe metadata): rebuilt by `fond reindex`. Safe to delete.
- **Overlay data** (pantry, ratings, notes): NOT rebuilt by reindex. Preserved across reindex operations.

## Key Design Decisions

See the [Architecture Decision Records](https://github.com/kafkade/fond/tree/main/docs/adr) for the full rationale behind each decision:

- [ADR-001](https://github.com/kafkade/fond/blob/main/docs/adr/001-core-language-rust.md): Rust as the core language
- [ADR-002](https://github.com/kafkade/fond/blob/main/docs/adr/002-hybrid-storage.md): Hybrid storage (files + SQLite)
- [ADR-003](https://github.com/kafkade/fond/blob/main/docs/adr/003-cooklang-integration.md): Cooklang integration
- [ADR-004](https://github.com/kafkade/fond/blob/main/docs/adr/004-cli-design.md): CLI design
- [ADR-005](https://github.com/kafkade/fond/blob/main/docs/adr/005-family-shared-db.md): Family-shared database
- [ADR-009](https://github.com/kafkade/fond/blob/main/docs/adr/009-pantry-model.md): Pantry model
- [ADR-010](https://github.com/kafkade/fond/blob/main/docs/adr/010-import-architecture.md): Import architecture
