# Research: Sync & Multi-Device Strategy

**Date**: 2026-06-26
**Status**: Complete (research) — feeds **ADR-012**
**Related Issue**: [#31](https://github.com/kafkade/fond/issues/31)
**Related Documents**: [ADR-002 Hybrid Storage](../adr/002-hybrid-storage.md),
[ADR-005 Family-Shared DB](../adr/005-family-shared-db.md),
[ADR-012 Sync & Multi-Device](../adr/012-sync-multi-device.md)
**Roadmap References**: Phase 7 (§13), §2 (concurrency), §12 (stack),
§15 (dependency map), §16 (feasibility), §19 Decision Log **D18**

> **Scope.** This is a *research-only* deliverable. It evaluates approaches and
> records a direction; it does **not** ship sync. Per Issue #31 and the roadmap,
> conflict resolution is a 🔴 problem deliberately **deferred until the data
> model is declared stable** (end of Phase 3). No code, crate, or prototype is
> introduced here.

---

## 1. Summary

fond does not have one sync problem — it has **two**, with very different
difficulty:

1. **Recipe content** (`.cook` files + content-addressed photos) — the
   **source of truth** (ADR-002). This is *already syncable today* with ordinary
   file-sync tools the user controls. 🟢
2. **The SQLite overlay** (`fond.db`) — a mix of a *disposable derived index*
   and *genuinely authored personal data* (notes, ratings, cook logs, pantry,
   meal plans, dietary profiles). Only the authored slice actually needs sync,
   and merging concurrent writes to it is the 🔴 part. 🔴

**Recommendation (detailed in ADR-012):**

- **Tier 1 — adopt file-sync first.** Recommend and document **Syncthing** as
  the default (peer-to-peer, no mandatory cloud, honors local-first), with
  cloud folders (Dropbox/iCloud/Drive) and **git** as supported alternatives.
  This covers the highest-value, lowest-risk surface — the recipes the user
  owns — with essentially zero new code.
- **Tier 2 — defer overlay sync.** When personal-overlay multi-device sync is
  warranted, prefer **sidecar export carried by the same file-sync channel**
  over a database-level CRDT. Evaluate **`cr-sqlite`** only if automatic
  multi-writer overlay merge proves necessary. Both require data-model
  preconditions that are **not yet met** (see §6).
- **Defer the hard parts:** conflict resolution and cross-device identity
  reconciliation remain out of scope until the data model is stable.

This keeps D18 directionally intact while sharpening it: the *file-sync half* is
now **`[Validated]`**; the *overlay/CRDT half* stays **`[Validation Required]`**.

---

## 2. What actually needs to sync

The crucial move is to stop treating `fond.db` as one thing. Per ADR-002 the
database is *derived and rebuildable* — but only **partly**. Some rows can be
regenerated from `.cook` files by `fond reindex`; others exist **only** in the
database and would be **lost forever** if it were deleted. Only the latter are
true sync payload.

| Layer | Examples (current schema) | Origin | Syncs how? |
|-------|---------------------------|--------|------------|
| **Recipe content** | `.cook` files, photos | Files (truth) | **File-sync (Tier 1)** |
| **Derived index** | `recipes`, `recipe_ingredients`, `steps`, `cookware`, `tags`, FTS | Rebuilt by `reindex` from files | **Never synced** — regenerate on each device |
| **Bundled reference** | `nutrition_facts`, `ingredient_allergens` | Shipped with binary | **Never synced** — same on every device |
| **Authored overlay** | `notes`, `ratings`, `cook_logs`, `pantry_items`, `meal_plans`/`meal_plan_entries`, `users`, `user_dietary_prefs`, `user_allergens`, `app_settings`, `import_review_queue` | Typed by the user, DB-only | **The real Tier 2 problem** |

**Implication:** the disposable index never travels — each device rebuilds it
locally with `reindex`. So multi-device sync reduces to (a) replicating the
`.cook`/photo files, and (b) replicating a *small, append-mostly* set of
authored overlay rows. That is a far smaller and more tractable problem than
"sync the whole database."

---

## 3. Tier 1 — File-sync for recipes & photos

Because recipes are plain `.cook` text and photos are content-addressed
immutable blobs, they suit general-purpose file sync extremely well. fond does
not need to *build* this — only to **recommend, document, and not fight it**
(atomic write-temp-then-rename is already the file-write strategy per §2).

| Option | Model | Pros | Cons | Verdict |
|--------|-------|------|------|---------|
| **Syncthing** | P2P, self-hosted, no cloud | No third party sees your data; free; cross-platform; honors local-first/ownership | Both devices online to converge; user sets it up once | **Recommended default** |
| **Cloud folder** (Dropbox/iCloud/Drive) | Hosted folder | Zero setup for most users; always-on relay; mobile-friendly | Third-party custody; subscription; the lock-in fond exists to escape (use for *transport* only, not format) | **Supported** |
| **Git** | Versioned DVCS | Full history, diffs, explicit conflict surfacing, free remotes | Manual push/pull (or a wrapper); binary photos bloat history (use Git LFS or keep photos out) | **Supported (power users)** |

**Conflict behavior at this tier.** Concurrent edits to the *same* `.cook` file
on two devices are the only real conflict. Syncthing/Dropbox keep both copies
(`file.sync-conflict-….cook`); git surfaces a merge conflict. Because `.cook` is
line-oriented text, these are human-resolvable — and far better than a silent
binary overwrite. Edits to *different* files never conflict. This matches the
ADR-002/ownership promise: users diff and resolve with ordinary tools.

**Recommended documentation deliverable (future, not now):** a short
"Sync your recipes" guide in the mdBook showing the three options, with
Syncthing as the walk-through, plus the reminder that `fond reindex` rebuilds
the local index on each device after files arrive.

---

## 4. Tier 2 — The authored overlay (the hard half)

The authored overlay is small but **DB-only** and **multi-writer** (two family
members, or one user on two devices, both rating/noting). Options, worst-fit to
best-fit for fond's principles:

### 4.1 Option A — Single-host DB + `fond serve` (do nothing extra)

Keep one authoritative database on a home server/primary machine; other devices
reach it over the (planned Phase 4) web/HTTP surface. **No sync at all.**

- ✅ Zero merge complexity; consistent with §2's "household shares one DB" (A5).
- ✅ Already the implicit Phase 0–4 model.
- ❌ Requires connectivity to the host; weak true-offline multi-device story.
- **Verdict:** the honest **status quo** and a legitimate long-term answer for a
  co-located household. Should be stated as the baseline before anything fancier.

### 4.2 Option B — Sidecar export carried by file-sync (**recommended Tier 2**)

Periodically export authored overlay rows to **plain-text sidecar files** that
live next to the recipes and ride the *same Tier 1 channel*; reimport on
`reindex`. e.g. per-user `overlay/<user>/ratings.jsonl`, `notes.jsonl`,
`cook-logs.jsonl` — append-mostly, line-oriented, diffable.

- ✅ One sync mechanism (files) for *everything*; no second moving part.
- ✅ Extends the ownership principle to personal data (your notes are text you
  own, not trapped in a binary DB).
- ✅ Append-mostly logs (cook logs, notes) merge cleanly; line-conflicts are
  human-resolvable like recipes.
- ✅ Last-writer-wins is acceptable for point data (a rating) with UUID +
  timestamp keys.
- ❌ Needs a stable, device-independent **record identity** (see §6) and an
  export/import codec — real work, but *ordinary* work, not distributed-systems
  research.
- **Verdict:** **best fit.** Keeps fond file-centric and avoids a CRDT runtime.

### 4.3 Option C — `cr-sqlite` CRDT extension

Load the [`cr-sqlite`](https://github.com/vlcn-io/cr-sqlite) extension and mark
authored tables as CRDTs; it tracks per-column causal metadata and merges
multi-writer changes automatically (the approach `toku` weighed, per §12/§19).

- ✅ Automatic, principled multi-writer merge; strong for true concurrent
  editing across always-on devices.
- ❌ Adds a native extension dependency + per-row/column bookkeeping tables to
  the "disposable" DB, muddying the rebuild-from-files story.
- ❌ Still needs a transport (it merges *changesets*, it doesn't move bytes).
- ❌ Heavier than the household actually needs at current scale; conflict
  semantics still must be designed.
- **Verdict:** **evaluate only if** Option B's last-writer-wins proves
  insufficient for real concurrent-edit pain. Keep as the documented fallback,
  not the default.

### 4.4 Option D — Custom sync server / protocol

Build a fond-specific sync service with accounts, deltas, and conflict policy.

- ✅ Total control; could power a hosted product someday.
- ❌ Massive scope; mandatory infrastructure; directly contradicts local-first /
  no-mandatory-cloud. **Rejected** for the foreseeable roadmap.

---

## 5. Conflict & identity scenarios

| Scenario | Tier | Handling |
|----------|------|----------|
| Edit *different* recipes on two devices | 1 | Trivial — disjoint files merge automatically |
| Edit the *same* `.cook` on two devices | 1 | File-sync keeps both / git conflict; line-level, human-resolvable |
| Photo added on two devices | 1 | Content-addressed ⇒ identical content = identical path = no conflict |
| Rate the *same* recipe on two devices | 2 | Point datum; UUID+timestamp ⇒ last-writer-wins (Option B) or CRDT (Option C) |
| Append cook logs / notes on two devices | 2 | Append-only ⇒ union merge, no conflict |
| Same user identity across devices | — | **Deferred** — see §6.2 (ADR-005 postpones identity reconciliation) |
| Two people, same household, two machines | 2 | `user_id` scoping disjoints most writes; cross-user shared tables (meal plans) need Option B/C |

The takeaway: **almost everything is conflict-free or human-resolvable** once
content-addressing (photos), UUID keys (overlay), and append-only logs are in
place. The genuinely hard residue is *concurrent edits to the same shared,
mutable record* — small, and explicitly deferred.

---

## 6. Data-model preconditions (why Tier 2 must wait)

Two concrete preconditions are **not yet satisfied** in the current schema. This
is the real reason to defer, beyond "conflict resolution is hard."

### 6.1 Device-stable record identity (currently missing)

The roadmap declares **UUIDv7** IDs (§8, §12, D-stack), but the implemented
`fond-store` migrations use **local `INTEGER PRIMARY KEY` rowids**, and overlay
rows reference recipes via that integer (`recipe_id INTEGER REFERENCES
recipes(id)`). Local integer rowids are **device-specific** — `recipes.id = 42`
on laptop A is a *different* recipe on laptop B after each rebuilds its index
from files. Therefore:

- Any synced overlay row must key the recipe by something **device-stable** —
  the recipe's **file path / slug** or a **UUID embedded in the `.cook`
  front-matter** — not the local rowid.
- Authored rows themselves (a given note, rating) need their own **stable global
  ID** (UUIDv7) so two devices can recognize "the same row" and merge rather
  than duplicate.

This gap must be closed (align the schema with the stated UUIDv7 decision, and
anchor recipe identity in the file, not the index) **before** any overlay sync —
Option B or C — is viable.

### 6.2 Identity reconciliation (deferred by ADR-005)

ADR-005 deliberately postpones "true remote identity reconciliation,
authentication, and multi-device conflict resolution" to this phase. Until a
device-spanning notion of *who a user is* exists, `user_id`-scoped rows cannot be
safely merged across machines. This research **confirms** that deferral rather
than resolving it.

### 6.3 "Data model stable" gate

Issue #31 and §15 both gate sync on **data model stability (end of Phase 3)**.
Building overlay sync against a still-moving schema would mean re-deriving merge
semantics on every migration. **Hold Tier 2 until that gate is reached.**

---

## 7. Phased recommendation

```text
Phase 7a (low-risk, near-term, mostly docs)
  └─ Document & bless Tier 1 file-sync (Syncthing default; cloud/git supported)
     • .cook + photos sync today; reindex rebuilds local index per device
     • No new code beyond a docs guide; ownership principle upheld

Phase 7b (precondition work — only after data model is STABLE, end Phase 3)
  └─ Close identity gaps (§6): adopt UUIDv7 per stated decision; anchor recipe
     identity in the file (slug/front-matter UUID), not the local rowid

Phase 7c (overlay sync — only if/when needed)
  └─ Option B: sidecar export/import of authored overlay over the Tier 1 channel
     • last-writer-wins for point data; union for append-only logs
  └─ Option C (cr-sqlite) ONLY if concurrent-edit pain proves Option B too weak

Always deferred until justified by real pain:
  • Conflict-resolution UX, custom sync server (Option D), accounts/auth
```

---

## 8. Comparison matrix

| Approach | Surface | New code | Honors local-first | Conflict story | Recommended |
|----------|---------|----------|--------------------|----------------|-------------|
| Syncthing / cloud / git | `.cook` + photos | ~none (docs) | ✅ (Syncthing/git) | Text/CA, human-resolvable | ✅ **Tier 1 now** |
| Single-host + `fond serve` | overlay | none | ✅ | N/A (one writer) | ✅ baseline |
| Sidecar export over file-sync | overlay | moderate | ✅ | LWW + append-union | ✅ **Tier 2 pref.** |
| `cr-sqlite` CRDT | overlay | higher | ⚠️ (extension) | automatic merge | ⏸ fallback only |
| Custom sync server | all | very high | ❌ | full control | ❌ rejected |

---

## 9. References

- **ADR-002** Hybrid Storage — `.cook` truth, SQLite derived/rebuildable.
- **ADR-005** Family-Shared DB — `user_id` scoping; identity reconciliation
  explicitly deferred to the sync phase.
- **ADR-012** Sync & Multi-Device — the decision this research feeds.
- ROADMAP **Phase 7** (§13.1), **§2** (WAL/atomic writes, A5 single-DB), **§12**
  (stack; `cr-sqlite`/`toku` precedent), **§15** (`.cook → file-sync →?
  cr-sqlite`), **§16** (sync row), **§19 D18**.
- [Syncthing](https://syncthing.net/) — P2P file sync.
- [`cr-sqlite`](https://github.com/vlcn-io/cr-sqlite) — CRDT extension for SQLite
  (vlcn.io), the merge engine evaluated as the Tier 2 fallback.
- Precedent: `toku` weighed `cr-sqlite` for the same overlay-merge question.
