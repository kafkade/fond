# ADR-013: Data Model Stability — 1.0 Freeze

**Status**: Accepted
**Date**: 2026-06-28
**Decision**: Declare the fond data model **stable** at v1.0.0. The on-disk
format (`.cook` files + content-addressed photos) and the SQLite overlay schema
(migrations V001–V010) are frozen; post-1.0 migrations must be **additive and
backward-compatible**. This satisfies the Phase 3 definition of done ("data model
declared stable → 1.0") and unblocks the ADR-012 Tier 2 sync precondition.

## Context

Phase 3 DoD (ROADMAP §13) gates 1.0 on the data model being declared stable.
ADR-012 made overlay sync conditional on record identity being device-stable:
the last blocker was that authored overlays (notes, ratings, cook logs)
referenced recipes by local `INTEGER` rowid with `ON DELETE CASCADE`, so
`fond reindex` wiped them and rowids differed per device. Issue #80 / migration
**V010** resolved this by re-anchoring those overlays to `recipe_slug` with
UUIDv7 `TEXT` primary keys. With that landed, no breaking schema change is
anticipated.

## Schema review (V001–V010)

Two tiers, intentionally separated:

- **Derived index (disposable, rebuilt by `fond reindex`, never synced):**
  `recipes`, `recipe_ingredients`, `steps`, `cookware`, `tags`, `recipe_fts`.
  These keep local `INTEGER` rowids — correct, because they are reconstructed
  from `.cook` files on each device and carry no authored data.
- **Authored overlays (device-stable, survive reindex, syncable):** `notes`,
  `ratings`, `cook_logs` (UUIDv7 + `recipe_slug`, V010), `meal_plans` /
  entries (`recipe_slug`, V007), `pantry_items` (V003, ingredient name),
  `user_profiles` (V006), `import_review_queue` (V009). Reference data:
  `nutrition_facts` (V008).

`.cook` files remain the source of truth and round-trip losslessly via
`raw_source` passthrough (ADR-002, ADR-003). Identity is anchored in the file
(slug) and overlay (UUIDv7), independent of any device's rebuilt index.

## Freeze policy (post-1.0)

- Migrations are **additive only**: new tables/columns/indexes; no destructive
  rewrites of existing authored data, no identity-anchor changes.
- New authored overlays MUST use UUIDv7 PKs and slug-based recipe anchors.
- `.cook` extensions go in sidecar metadata or the overlay, never by breaking
  the Cooklang spec (principle #4).
- Breaking changes would require a 2.0 and a documented migration path.

## Consequences

- 1.0 ships with confidence that user data survives reindex and is sync-ready.
- ADR-012 flips Proposed → Accepted; Tier 2 sync precondition is met.
- The DB stays disposable/derivable; `.cook` files stay sacred and portable.
