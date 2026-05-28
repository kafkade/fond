# ADR-010: Import Architecture — Trait-Based Adapter Pipeline

**Status**: Proposed
**Date**: 2025-07-13
**Decision**: Build imports as source-specific adapters that emit `RecipeDraft`s into a shared normalize→Cooklang→validate→write/review pipeline with first-class dry-run support.

## Context

Import is the product's superpower and the primary activation metric. The roadmap promises that a Paprika power user can migrate hundreds of recipes in under 10 minutes, yet the same system must also handle schema.org pages, authenticated sites, raw `.cook` files, and later heuristic text or OCR sources.

Those sources differ wildly in quality. Some provide clean structured data, some offer messy HTML, and some can only be partially normalized. Section 3.6 therefore establishes a review queue so clean recipes do not block on ambiguous ones, while Section 16 rejects lossy import and all-or-nothing batch behavior.

Because `.cook` files are the canonical store, every importer eventually has to converge on the same durable output and validation rules. The architecture should make that common path the center of gravity, with source-specific code kept at the edges.

## Decision

fond will implement imports in `fond-import` as a **trait-based adapter pipeline**. Each source-specific adapter implements an interface like `Importer -> Vec<RecipeDraft>` and produces raw drafts with provenance intact.

All drafts then flow through one shared pipeline:

```text
Importer
  -> Vec<RecipeDraft>
  -> normalize
  -> to-Cooklang
  -> validate
  -> clean recipes     -> write .cook + index
  -> ambiguous recipes -> review queue (`fond review`)
```

A **dry-run** mode is mandatory for batch imports. In dry-run, fond performs extraction, normalization, validation, duplicate checks, and reporting without writing files, so users can see how many recipes are clean versus queued for review before any mutation happens.

## Rationale

- **One quality bar**: every source shares the same normalization, validation, and review behavior.
- **Extensibility**: adding a new importer is mostly one new adapter rather than a fresh end-to-end pipeline.
- **No-data-loss fit**: ambiguous input can be preserved and reviewed instead of being silently dropped or guessed.
- **User trust**: dry-run makes imports inspectable before they touch the canonical file store.
- **Throughput**: clean recipes can write immediately, preserving the <10-minute migration promise.

## Alternatives Considered

| Alternative | Rejected Because |
|------------|-----------------|
| One bespoke import path per source | Duplicates normalization logic, creates inconsistent quality, and makes new importers expensive to add. |
| Strict all-or-nothing batch import | One bad recipe can stall an otherwise successful migration. |
| Lossy "good enough" import | Violates the product's ownership and trust promises by dropping or inventing data. |
| Manual review before any write | Makes large imports too slow and breaks the core migration promise. |

## Consequences

- Strong upside: imports stay modular at the source layer and consistent at the normalization layer.
- Strong upside: dry-run and review queues make risky batch imports trustworthy instead of opaque.
- Tradeoff: the draft and validation pipeline becomes a central abstraction that must be carefully designed and tested.
- Tradeoff: review tooling and duplicate detection become first-class product work rather than incidental follow-up tasks.
