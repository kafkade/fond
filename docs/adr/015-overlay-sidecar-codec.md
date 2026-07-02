# ADR-015: Authored-overlay sidecar codec (Sync Tier 2)

**Status**: Accepted
**Date**: 2026-07-01
**Decision**: Implement ADR-012 Tier 2 by exporting the *authored* overlay slice
to **per-user, line-oriented JSONL sidecar files** carried over the existing
Tier 1 file-sync channel, merged on import with **last-writer-wins** for point
data and **union** for append-only logs, with every conflict reported. Keep
[`cr-sqlite`](https://github.com/vlcn-io/cr-sqlite) as a documented fallback,
not built — last-writer-wins is sufficient at household scale.

## Context

[ADR-012](012-sync-multi-device.md) split fond's sync surfaces into **Tier 1**
(recipe `.cook` files + content-addressed photos — already syncable) and
**Tier 2** (the authored overlay: notes, ratings, cook logs, pantry, meal plans,
dietary profiles). It chose *sidecar export over the same file-sync channel* as
the preferred Tier 2 mechanism but deferred building it, gating on (a) data-model
stability (reached at 1.0.0, ADR-013) and (b) closing the identity gap.

The identity gap is now closed: migration **V010** (issue #80) re-anchored
notes/ratings/cook_logs to UUIDv7 primary keys plus a device-stable
`recipe_slug`, so authored rows survive a `fond reindex` on any device. Pantry
and meal plans already carry `updated_at`; meal-plan entries reference recipes by
slug. Both preconditions are met, so Tier 2 can ship.

## Decision

### Format & layout

Sidecars live under an overlay directory (default `<data_dir>/overlay/`) that
rides the same file-sync channel as recipes. One **JSON object per line**;
records are sorted deterministically on export so diffs stay minimal and
human-reviewable.

```text
overlay/
  users/<user-slug>/
    notes.jsonl        # union merge (append-only), key = id (UUIDv7)
    ratings.jsonl      # last-writer-wins, key = recipe_slug
    cook-logs.jsonl    # union merge (append-only), key = id (UUIDv7)
    profile.jsonl      # union merge of allergen + dietary-pref membership
  shared/
    pantry.jsonl       # last-writer-wins, key = name (NOCASE)
    meal-plans.jsonl   # last-writer-wins per-plan snapshot, key = name
```

Notes, ratings, cook logs, and dietary profiles are **per-user**; pantry and
meal plans are **household-shared** (no `user_id`). Each per-user record carries
a `user` **name** as its device-stable identity — the directory slug is
cosmetic. On import the local user is resolved (or created) by name. ADR-005
still defers full identity reconciliation; name is the pragmatic bridge at
household scale.

### Merge semantics

| Overlay | Identity | Merge | Conflict handling |
|---------|----------|-------|-------------------|
| notes | `id` (UUIDv7) | union — insert if `id` absent | none (additive) |
| cook_logs | `id` (UUIDv7) | union — insert if `id` absent | none (additive) |
| ratings | (recipe_slug, user) | LWW by `(updated_at, id)` | overwrite/skip of a differing score reported |
| pantry_items | `name` (NOCASE) | LWW by `updated_at` | overwrite/skip of a differing presence reported |
| meal_plans | `name` | LWW whole-plan snapshot by `updated_at` (winner's entry set replaces the loser's) | overwrite/skip of a differing entry set reported |
| dietary profile | (user, allergen/pref) | union of set membership | none (additive) |

- **LWW preserves the winner's id/timestamps on apply**, so all devices converge
  to the same row regardless of import order. Ties break by id/name for
  determinism.
- **Additive merges never delete.** Deletions do not propagate in this codec — a
  deliberate, data-safe limitation consistent with append-only logs. (Meal plans
  are the exception: a whole-plan LWW snapshot *does* propagate entry deletions,
  because the newer side's entry set replaces the older side's.)
- **Conflicts are always reported**, never silently applied. A conflict is any
  LWW case where local and incoming values differ; the report names the winner.
- Import runs in a single transaction (all-or-nothing) and is idempotent —
  re-importing the same sidecars changes nothing.

### Surface

- `fond overlay export [--dir <path>] [--user <name>]` — write sidecars.
- `fond overlay import [--dir <path>]` — merge sidecars, printing a report.
- `fond overlay status [--dir <path>]` — show the sidecar directory and local
  overlay counts.
- `fond reindex` **auto-imports** the overlay after rebuilding the index (when
  `overlay/` exists), so a synced device converges in one command. The
  `fond-store::reindex()` signature is unchanged; the CLI orchestrates
  reindex-then-import to keep the crate decoupled from path layout.

Export stays explicit in this version (no write-time auto-export); a future hook
can auto-export on relevant writes.

### `cr-sqlite`: documented, not built

Per the ADR-012 deliverable, `cr-sqlite` was to be evaluated *only if* real
concurrent-edit pain showed last-writer-wins to be insufficient. At current
household scale that pain has **not** appeared: writes are naturally partitioned
(per-user notes/ratings/logs; disjoint pantry/plan edits are rare and, when they
collide, LWW + a reported conflict is acceptable). Adding a native CRDT extension
plus per-column causal-metadata tables to the deliberately *disposable* DB is not
justified. **Decision: keep `cr-sqlite` as the documented fallback; do not build
it now.** Revisit only if always-on multi-writer concurrent editing becomes a
real workflow.

## Rationale

- **One mechanism for everything**: authored data rides the same file channel as
  recipes — no second moving part, no server, no mandatory cloud.
- **Ownership extended to personal data**: your notes/ratings are plain text you
  own, diffable and portable, not trapped in a binary DB.
- **Tractable merge**: UUID keys + append-only logs make most cases conflict-free
  or human-resolvable; only point data needs LWW, which is simple and converges.
- **No premature complexity**: avoids a CRDT runtime until the household actually
  needs automatic multi-writer merge.

## Alternatives Considered

| Alternative | Rejected / Deferred Because |
|-------------|-----------------------------|
| **`cr-sqlite` CRDT as the default** | Native extension + causal-metadata tables on a disposable DB; heavier than needed. **Kept as fallback.** |
| **Sync the whole `fond.db` blob** | Derived, device-specific, rebuildable; blob-sync corrupts across devices. **Rejected (ADR-012).** |
| **Per-row `updated_at` on profile sets for LWW deletes** | Requires a migration for a rarely-edited set; union merge is simpler and loses no data. **Deferred.** |
| **A dedicated `fond-export`-style crate** | Overlay sync is tightly coupled to the store schema; a `fond-store` module keeps identity/merge logic next to the tables. **Rejected for now.** |

## Consequences

- New `fond-store::overlay` module (codec + merge engine + `MergeReport`), new
  `fond overlay` CLI subcommand, and reindex auto-import. No new migration
  (V010 supplies UUIDs; pantry/meal_plans already carry `updated_at`).
- **No CI / `kafkade/github-infra` change**: no new required checks and no job
  renames — the single `CI` gate is unaffected.
- Deletions of notes/ratings/cook-logs and of profile-set members do not
  propagate; a future ADR can add tombstones if this becomes painful.
- Cross-user identity remains name-keyed; true identity reconciliation is still
  deferred to a later ADR (per ADR-005).
- Decision Log **D18**'s overlay half moves to `[Validated]`.
