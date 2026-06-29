# ADR-012: Sync & Multi-Device Strategy

**Status**: Accepted
**Date**: 2026-06-26 (accepted 2026-06-28: data model declared stable, see ADR-013)
**Decision**: Sync recipe content via user-controlled **file-sync first**;
**defer** authored-overlay sync, and when it is built prefer **sidecar export
over the same file-sync channel**, evaluating **`cr-sqlite`** only if automatic
multi-writer merge proves necessary. Conflict resolution and cross-device
identity reconciliation remain deferred until the data model is declared stable
(end of Phase 3).

## Context

Principle #1 (local-first) and #2 (data ownership) require that fond work fully
offline and that the user own their data forever. ADR-002 makes `.cook` files
the source of truth and SQLite a *derived, rebuildable* index/overlay. ADR-005
adopts a single shared household database with `user_id` scoping and explicitly
**defers** "remote identity reconciliation, authentication, and multi-device
conflict resolution" to the sync phase. Roadmap Phase 7, §15, and Decision Log
**D18** all point the same way: *file-sync first, CRDTs only if overlays need
them*, with conflict resolution flagged 🔴 and deferred.

The supporting research
([sync-multi-device-strategy.md](../research/sync-multi-device-strategy.md))
establishes that fond has **two** distinct sync surfaces, not one:

1. **Recipe content** — `.cook` files + content-addressed photos. The source of
   truth; already syncable today with ordinary file-sync tools. 🟢
2. **The SQLite overlay** — partly a *disposable derived index* (rebuilt by
   `fond reindex`, never synced) and partly *authored, DB-only data* (notes,
   ratings, cook logs, pantry, meal plans, dietary profiles). Only the authored
   slice is genuine sync payload, and merging concurrent writes to it is the 🔴
   part. 🔴

The research also surfaced a concrete blocker: although UUIDv7 IDs are the
stated decision (§8/§12), the current `fond-store` schema uses **local
`INTEGER PRIMARY KEY` rowids**, and overlay rows reference recipes by that local
integer. Local rowids are device-specific, so overlay sync is impossible until
record identity is anchored to something device-stable (the file/slug, or a
UUID), independent of each device's rebuilt index.

## Decision

fond adopts a **two-tier, file-centric sync strategy**:

**Tier 1 — Recipe content via user-controlled file-sync (now / near-term).**
fond will **recommend and document** file-sync for `.cook` files and photos
rather than build a sync engine. **Syncthing** is the recommended default
(peer-to-peer, no mandatory cloud, honors local-first); **cloud folders**
(Dropbox/iCloud/Drive) and **git** are supported alternatives. The disposable
index is **never synced** — each device runs `fond reindex` to rebuild it
locally after files arrive. Atomic write-temp-then-rename (already used) keeps
file writes safe under sync.

**Tier 2 — Authored-overlay sync (deferred; design recorded, not built).**
When personal-overlay multi-device sync is warranted, the **baseline** remains a
single-host database reached over the Phase 4 web/HTTP surface (no sync). Beyond
that, the **preferred** mechanism is **sidecar export/import carried by the same
Tier 1 file-sync channel** (append-mostly, line-oriented, diffable text owned by
the user): last-writer-wins for point data (e.g. a rating, keyed by UUID +
timestamp) and union merge for append-only logs (notes, cook logs).
**`cr-sqlite`** is the documented **fallback**, to be evaluated only if
real concurrent-edit pain shows last-writer-wins is insufficient.

**Deferred deliberately** (no implementation in this ADR): conflict-resolution
UX, cross-device identity reconciliation (per ADR-005), accounts/auth, and any
custom sync server. Tier 2 is additionally **gated on data-model stability
(end of Phase 3)** and on first closing the identity gap (adopt UUIDv7 per the
stated decision; anchor recipe identity in the file, not the local rowid).

This ADR is **Proposed** — it records direction and defers the 🔴 work, matching
ADR-005's forward-looking lifecycle.

## Rationale

- **Leverages owned files**: recipes are plain text and photos are immutable
  content-addressed blobs — they already sync cleanly, so Tier 1 is near-zero
  code and fully upholds ownership and local-first.
- **Right-sizes the hard problem**: separating the disposable index from the
  authored overlay shrinks "sync the database" down to "replicate a small,
  append-mostly set of authored rows," which is tractable.
- **One mechanism for everything**: sidecar-over-file-sync avoids a second
  moving part and extends the ownership promise to personal data.
- **Avoids premature complexity**: a CRDT runtime (`cr-sqlite`) adds native
  dependencies and per-row metadata to a database that ADR-002 wants to keep
  disposable; it is reserved for when it is actually needed.
- **Honest deferral**: conflict resolution and identity are genuinely hard and
  depend on a stable schema; building them now would mean re-deriving merge
  semantics on every migration.

## Alternatives Considered

| Alternative | Rejected / Deferred Because |
|-------------|-----------------------------|
| **Custom fond sync server** (accounts, deltas, hosted) | Massive scope; mandatory infrastructure; contradicts local-first / no-mandatory-cloud. **Rejected.** |
| **`cr-sqlite` as the default overlay sync** | Adds a native extension + causal-metadata tables to the "disposable" DB before the household needs automatic multi-writer merge. **Kept as fallback, not default.** |
| **Sync the whole `fond.db` as a blob** | The DB is derived and device-specific (local rowids, rebuildable index); blob-syncing it overwrites and corrupts across devices. **Rejected.** |
| **Mandatory cloud folder for all data** | Re-introduces third-party custody and lock-in fond exists to escape; cloud is supported only as an optional *transport*. **Rejected as default.** |
| **Build overlay sync now** | Data model is not yet stable (gated to end of Phase 3) and record identity is not device-stable; premature. **Deferred.** |

## Consequences

- **Near-term**: a documentation deliverable ("Sync your recipes" guide,
  Syncthing walk-through) and the standing reminder that `fond reindex` rebuilds
  the local index per device. No engine, no new crate, no CI/`kafkade/github-infra`
  change.
- **Precondition surfaced**: before any Tier 2 work, the schema must align with
  the UUIDv7 decision and anchor recipe identity in the file (slug/front-matter
  UUID) rather than the local `INTEGER` rowid. This is now a tracked dependency.
- **Decision Log**: D18 is refined — the *file-sync half* becomes `[Validated]`
  while the *overlay/CRDT half* stays `[Validation Required]`.
- **Future ADRs**: the eventual overlay-sync mechanism (sidecar codec vs
  `cr-sqlite`), conflict-resolution UX, and identity reconciliation each warrant
  their own ADR once the data model is stable.
- **No lock-in**: every tier keeps `.cook` files (and, for Tier 2, plain-text
  sidecars) as user-owned, portable artifacts.
