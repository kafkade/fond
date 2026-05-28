# ADR-003: Cooklang Integration — `cooklang-rs`

**Status**: Proposed
**Date**: 2025-07-13
**Decision**: Integrate the community `cooklang-rs` crate behind a swappable parser interface, gated by a Phase 0 spike on real recipes.

## Context

Principle #4 makes fond explicitly Cooklang-native. The product cannot treat Cooklang as an export format bolted on later; it must parse and emit `@ingredient{}`, `#cookware{}`, `~timer{}`, and `---` metadata faithfully because `.cook` files are the source of truth.

That requirement touches nearly every early deliverable. Phase 0 depends on a parser spike, Phase 1 importers must normalize foreign recipes into Cooklang, and the storage model in ADR-002 depends on lossless round-trips between file content and the domain model. A weak parser would jeopardize the entire roadmap's critical path.

The roadmap already assumes a community Rust crate exists but marks usability as `[Validation Required]`. Section 14.1 makes the required validation concrete: parse 10 real recipes, round-trip them, and assess gaps before committing the architecture to this dependency.

## Decision

fond will adopt **`cooklang-rs`** as the primary parser for Cooklang files, but only after a Phase 0 spike validates the real-world behavior the project needs.

The integration will sit behind a trait boundary in the domain layer so the rest of the application depends on a stable parsing/emission interface rather than on the crate directly. If the spike reveals missing emission or round-trip features, fond will first prefer contributing upstream or adding a thin emitter layer before considering a parser replacement.

## Rationale

- **Spec alignment**: using a community Cooklang implementation keeps fond aligned with the wider ecosystem instead of inventing a private dialect.
- **Schedule protection**: adopting an existing parser is much cheaper than spending weeks on a bespoke implementation during Phase 0.
- **Single-binary fit**: a native Rust crate preserves the one-language, one-binary architecture.
- **Upstream leverage**: any gaps fixed upstream benefit both fond and the broader Cooklang ecosystem.
- **Risk isolation**: a trait boundary limits the blast radius if the parser must later be swapped.

## Alternatives Considered

| Alternative | Rejected Because |
|------------|-----------------|
| Write a custom Cooklang parser | Reinvents a published spec, costs multiple weeks, and creates a long-term maintenance burden before the product proves itself. |
| Shell out to an external reference parser | Breaks the single-binary goal, adds runtime/process dependencies, and complicates cross-platform distribution. |
| Parse only loosely and store raw text | Violates the Cooklang-native promise and makes timeline, scaling, cookware, and metadata features much weaker. |

## Consequences

- Strong upside: fond gets to stand on existing Cooklang work and stay ecosystem-compatible.
- Strong upside: the parser spike creates an explicit go/no-go checkpoint before too much code depends on the crate.
- Tradeoff: fond becomes dependent on an external library whose round-trip behavior may not yet be complete.
- Tradeoff: if gaps appear, the project may need upstream contributions or a thin local emitter layer before Phase 0 is truly done.
