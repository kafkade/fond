# Spike #4: SQLite/FTS5 Derive-from-Files + Reindex

| Field       | Value                                                   |
| ----------- | ------------------------------------------------------- |
| Issue       | [#4](https://github.com/kafkade/fond/issues/4)          |
| ADR         | [002 — Hybrid Storage](../adr/002-hybrid-storage.md)    |
| Crate       | `fond-store`                                            |
| Status      | ✅ GO                                                   |

## Objective

Prove the hybrid storage model from ADR-002: `.cook` files as the source of
truth with SQLite as a derived, rebuildable index. Validate FTS5 full-text
search, atomic reindex, and recovery from database deletion.

## Schema (v1)

```sql
-- Core recipe index (derived from .cook files)
recipes        (id, file_path UNIQUE, title, description, servings, source,
                prep_time, cook_time, content_hash, indexed_at)
ingredients    (id, recipe_id FK, name, quantity, sort_order)
steps          (id, recipe_id FK, section_name, body, sort_order)
tags           (name, recipe_id FK) — composite PK

-- Full-text search (FTS5, rowid = recipes.id)
recipe_fts     (title, ingredients_text, steps_text, tags_text)

-- Migration tracking
schema_version (version PK, applied_at)
```

**Design decisions:**

- No `AUTOINCREMENT` — plain `INTEGER PRIMARY KEY` for id reuse after DELETE
- FTS5 `rowid` explicitly set to match `recipes.id` for efficient joins
- `content_hash` enables future incremental reindex (change detection)
- `ON DELETE CASCADE` on child tables for clean teardown
- WAL journal mode for concurrent read performance

## Reindex Model

```text
.cook files → parse all → transaction { DELETE ALL → INSERT ALL } → commit
```

- **Atomic**: entire rebuild inside a single transaction — no partial state
- **Idempotent**: running reindex twice yields identical results
- **Deterministic**: file paths sorted before processing
- **Fault-tolerant**: invalid files skipped with error reporting; valid files
  still indexed
- **Recoverable**: delete the database, reindex, zero data loss

## Test Results

25 tests covering:

| Category                | Tests | Notes                                              |
| ----------------------- | ----- | -------------------------------------------------- |
| Schema & FTS5           | 2     | Table creation, version tracking, FTS5 available   |
| Indexing                | 5     | Single recipe, ingredients, steps, tags, hash       |
| Title derivation        | 1     | Filename → title when metadata absent              |
| FTS5 search             | 7     | Title, ingredient, step, tag, cross-field, ranking, phrase/prefix |
| Reindex                 | 5     | Idempotent, updates, invalid files, empty/missing dirs |
| Recovery                | 1     | Delete DB → recreate → reindex → full recovery     |
| Performance             | 2     | 1k reindex, 1k search                             |
| Summary report          | 1     | Printed diagnostic table                           |
| **Total**               | **25**|                                                    |

All 25 tests pass on Windows, macOS, and Linux (CI matrix).

## Performance

Measured with 1,000 synthetic recipes (5–9 ingredients, 3–6 steps each):

| Operation       | Time      | Threshold |
| --------------- | --------- | --------- |
| Reindex 1k      | ~3s       | < 15s     |
| FTS5 search avg | ~0.1ms    | < 100ms   |

## Findings

### Confirmed

1. **rusqlite bundled-SQLite includes FTS5** — no separate feature flag needed;
   `bundled` gives us a recent SQLite with FTS5 enabled
2. **Atomic reindex via `unchecked_transaction()`** — takes `&self` not
   `&mut self`, compatible with shared `Connection` references
3. **FTS5 phrase and prefix queries** work out of the box (`"chicken thighs"`,
   `chick*`)
4. **WAL mode** — `PRAGMA journal_mode = WAL` returns a row, must use
   `query_row` not `execute_batch`
5. **Cross-field search** — FTS5 searches all columns by default; column
   prefixes (`title:`, `ingredients_text:`) scope to specific fields
6. **Content hash** — `DefaultHasher` (non-cryptographic) sufficient for
   change detection; fast, deterministic per build

### Risks Identified

1. **Migration framework deferred** — manual `schema_version` table for spike;
   production should adopt `refinery` or equivalent for versioned migrations
2. **Reindex is full rebuild** — acceptable for Phase 0/1; incremental reindex
   (using content_hash) should be implemented for 1k+ recipe collections
3. **FTS5 tokenizer is default (unicode61)** — adequate for English; may need
   custom tokenizer for CJK recipe names (Phase 5+)
4. **Single-directory scan** — current implementation reads flat directory;
   production needs recursive walk or configurable paths

### Surprises

- **Performance exceeded expectations** — 1k reindex in ~3s, search in
  sub-millisecond; SQLite is remarkably fast for this workload
- **Cooklang `Item` enum** has an undocumented catch-all variant — the `_ =>`
  wildcard arm in match is required for forward compatibility

## Recommendations

1. **Adopt `refinery` for migrations** — `schema_version` table is fine for
   prototyping but production needs versioned SQL migration files
2. **Add incremental reindex** — compare content_hash before re-parsing; only
   rebuild changed recipes
3. **Add user overlay tables** — notes, ratings, cook log (not part of this
   spike per ADR-002 scope)
4. **Add database integrity checks** — `PRAGMA integrity_check` on startup,
   automatic reindex on corruption
5. **Consider `tokio-rusqlite`** — for async CLI/web integration in Phase 1+

## Verdict

### ✅ GO

The hybrid storage model is validated. SQLite/FTS5 provides fast, reliable
full-text search over `.cook` files with atomic rebuild and zero-data-loss
recovery. The schema supports the Phase 1 MVP requirements. The `fond reindex`
command can be implemented directly from this spike's code.
