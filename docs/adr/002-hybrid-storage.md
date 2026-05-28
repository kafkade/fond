# ADR-002: Recipe Storage — Hybrid Files + SQLite Index

**Status**: Accepted
**Date**: 2025-07-13
**Decision**: Keep `.cook` files as the canonical recipe store and use SQLite/FTS5 as a derived, rebuildable index and overlay.

## Context

fond's first principle is data ownership: the user's recipes must remain portable, inspectable, and editable outside the application. Principle #4 tightens that further by making Cooklang the canonical format, so recipe content must live naturally as `.cook` text rather than inside an opaque database.

At the same time, the product is not just a file viewer. Search, tags, ratings, pantry state, meal plans, cook logs, and grocery lists all need fast queries and relational joins. Section 8 makes that explicit: recipe text belongs in files, while overlays such as notes, ratings, plans, and reference datasets are better represented in SQLite.

The architecture in §2 already describes the canonical loop: write or import `.cook` files, parse them into the domain model, upsert derived search/index rows, and allow `fond reindex` to rebuild the entire database at any time. That gives fond the trust benefits of the Calibre/Obsidian model without sacrificing FTS5-backed performance.

## Decision

fond will adopt a **hybrid persistence model**:

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

`.cook` files own recipe content. SQLite owns the derived search index, relational overlays, migrations, and bundled reference data. `fond reindex` must be able to reconstruct `fond.db` entirely from the file tree plus bundled datasets, proving that the database is disposable and the files are sacred.

## Rationale

- **Ownership first**: users can back up, diff, sync, and hand-edit recipes with ordinary filesystem tools.
- **Cooklang-native round-trip**: keeping recipes as `.cook` files preserves the product's open-format promise.
- **Fast search and filters**: SQLite + FTS5 handles instant list/search/filter use cases at the expected scale.
- **Relational overlays fit naturally**: ratings, pantry state, meal plans, cook logs, and nutrition data do not belong inside a single recipe file.
- **Recovery story is simple**: if the DB is corrupted or deleted, `fond reindex` rebuilds it from durable sources.
- **Strong precedent**: the Calibre/Obsidian pattern is well understood and already cited in the roadmap.

## Alternatives Considered

| Alternative | Rejected Because |
|------------|-----------------|
| Database-only storage | Violates data ownership, hides the real recipes in an opaque store, and makes export feel like an escape hatch instead of a no-op. |
| Files-only storage | Keeps ownership but makes full-text search, filtering, and relational features awkward and slow at scale. |
| Document database | Adds complexity and operational weight without matching SQLite's embedded simplicity or FTS5 support. |
| Photos in SQLite blobs | Conflicts with the content-addressed filesystem plan and makes large binary assets harder to inspect and back up. |

## Consequences

- Strong upside: user trust, portability, and disaster recovery are excellent because the canonical data is plain text.
- Strong upside: search and higher-level household features remain fast and queryable.
- Tradeoff: writes are slightly more complex because file updates and index updates both happen.
- Tradeoff: the parser and reindex path become critical infrastructure and must stay lossless and well tested.
