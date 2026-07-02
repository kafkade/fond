# fond-store

SQLite persistence, migrations, and FTS5 search for [fond](https://github.com/kafkade/fond) — a local-first, CLI-first personal cooking & recipe manager.

The database is a **derived index** — `.cook` recipe files on disk are the source of truth, and `fond reindex` rebuilds the DB from those files. The database is disposable; the files are sacred.

## Features

- **FTS5 full-text search** across recipe titles, ingredients, steps, and tags with relevance ranking.
- **Filtered queries** — combine tag, cuisine, max cook time, and source filters with text search.
- **Schema migrations** via [refinery](https://crates.io/crates/refinery) — idempotent, embedded SQL migrations.
- **Tag management** — list tags with counts, query tags per recipe, add/remove tags (writes back to `.cook` files).
- **Reindex** — rebuild the entire database from `.cook` files on disk. Content-hash based skip for unchanged files.
- **Single-recipe writes** — `write_recipe_file`, `read_recipe_file`, `delete_recipe`, and `remove_old_file_after_rename` persist an individual recipe: the `.cook` file is written first (source of truth), then just that recipe's rows are upserted in the derived index. Includes `content_hash`/`bytes_hash` helpers for optimistic-concurrency guards and content-addressed photos. Powers native app editing via `fond-ffi`.
- **Authored-overlay sync** (`overlay` module) — export the authored overlay (notes, ratings, cook logs, pantry, meal plans, dietary profiles) to per-user JSONL sidecar files and merge them back with last-writer-wins (point data) / union (append-only logs), reporting every conflict. Carries ADR-012 Tier 2 over the file-sync channel; see [ADR-015](../../docs/adr/015-overlay-sidecar-codec.md).

## Storage Model

```text
~/fond/
  recipes/            ← .cook files (SOURCE OF TRUTH)
    chicken-adobo.cook
  fond.db             ← SQLite index (DERIVED, rebuildable)
```

## Usage

```rust
use fond_store::{FondPaths, FondDb, Repo};

let paths = FondPaths::resolve(None)?;
let db = FondDb::open(&paths.db_path)?;
let repo = Repo::new(&db);

let results = repo.search("chicken")?;
for r in &results {
    println!("{} ({})", r.title, r.slug);
}
```

## License

[MIT](https://github.com/kafkade/fond/blob/main/LICENSE)

Part of the [fond](https://github.com/kafkade/fond) workspace.
