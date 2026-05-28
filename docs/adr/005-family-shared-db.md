# ADR-005: Database Schema — Family-Shared with Per-User Scoping

**Status**: Proposed
**Date**: 2025-07-13
**Decision**: Use one shared household SQLite database, with `user_id` only on subjective or personal records.

## Context

Principle #3 says fond must be family-shared by design rather than retrofitted later. Even though the first real deployment is assumed to be one co-located household on one machine, the data model has to anticipate multiple people cooking, rating, planning, and maintaining different dietary profiles.

Section 8 already splits the model cleanly. Recipes, ingredients, tags, photos, meal plans, and grocery lists are shared household assets, while notes, ratings, cook logs, and dietary/allergen profiles are personal. Section 2 also assumes one SQLite database with WAL mode and short transactions, which is appropriate for the local-first single-household target.

The key design tension is avoiding both extremes: over-engineering a solo hobby project with full auth/RBAC, or pretending there is only one user and forcing a painful retrofit later. The schema needs just enough multi-user shape from v1 to avoid future migration pain.

## Decision

fond will use **one shared SQLite database** for a household. Shared entities live once; subjective entities carry a **`user_id`** and are interpreted in that user's context.

The MVP may still default to a single active user in the CLI and UI, but the schema will include user-aware columns and tables from the beginning. True remote identity reconciliation, authentication, and multi-device conflict resolution are explicitly deferred to the later sync phase rather than solved inside the Phase 0-3 database design.

## Rationale

- **Matches the actual household model**: family members share recipes and plans but not necessarily ratings, notes, or dietary restrictions.
- **Avoids a retrofit**: user-aware tables are cheap now and expensive later.
- **Keeps storage simple**: a single SQLite file fits the local-first architecture and shared-machine assumption.
- **Supports later features**: family profiles, allergen filters, scoreboards, and meal planning all need user context.
- **Limits scope**: household sharing does not require full account systems or permissions in the MVP.

## Alternatives Considered

| Alternative | Rejected Because |
|------------|-----------------|
| One database per user | Duplicates shared recipe data and makes shared planning and pantry workflows awkward. |
| No user concept until later | Violates the family-shared principle and guarantees a difficult migration once subjective data appears. |
| Full auth/RBAC from day one | Solves a larger problem than the roadmap has, adding major complexity for little household value. |
| Separate shared DB plus personal overlay DBs | Adds synchronization and query complexity without enough benefit at MVP scale. |

## Consequences

- Strong upside: household features can grow naturally from the schema already in place.
- Strong upside: the CLI can stay simple at first while the data model remains future-compatible.
- Tradeoff: even solo-user code paths must carry some user context from the beginning.
- Tradeoff: remote identity, sync conflict handling, and permissions are deliberately postponed and will need their own later ADRs.
