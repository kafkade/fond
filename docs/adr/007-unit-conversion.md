# ADR-007: Unit Conversion Engine — Density-Aware and Honest

**Status**: Proposed
**Date**: 2025-07-13
**Decision**: Model recipe quantities as typed values with unit families and ingredient references, and allow volume-to-weight conversion only when a per-ingredient density is known.

## Context

Cooking software fails when it pretends culinary quantities are generic physics problems. Section 3A.2 makes the key constraint explicit: 1 cup of flour, water, and honey do not weigh the same, vague units such as "a pinch" should not be coerced into numbers, and package units like "1 can" or "1 stick" only make sense in ingredient context.

This conversion model underpins more than scaling. Pantry deduction, grocery consolidation, coverage checks, nutrition estimation, and baker's-percentage workflows all need quantities to be modeled carefully enough that the application is either correct or explicitly refuses the operation.

The roadmap also distinguishes code difficulty from data difficulty. A typed units engine is feasible early, but the density and ontology data needed to make it broadly useful will grow over multiple phases. The architecture must support that staged reality rather than bluff completeness.

## Decision

fond will model quantities as a typed structure such as **`Quantity { value, unit, ingredient_ref }`**. Unit families like volume, weight, count, temperature, vague, and baker's percentage are first-class in the domain model.

Conversions within a family may be performed freely. Cross-family conversion between volume and weight is allowed **only** when the canonical ingredient has a known density in the bundled reference dataset. Unknown density yields a clear refusal, not a guess. Vague units pass through untouched, and package aliases such as "1 can" or "1 stick" are handled through ingredient-specific reference data rather than global assumptions.

## Rationale

- **Culinary correctness**: ingredient density is the real variable, so the model keeps it attached to the ingredient.
- **Honest behavior**: refusing unknown conversions is better than silently inventing wrong numbers.
- **Feature leverage**: the same quantity model supports scaling, pantry, grocery, and nutrition features.
- **Extensible data story**: density tables and aliases can grow over time without changing the core type system.
- **Support for baking**: baker's-percentage and weight-based workflows need explicit units, not loose strings.

## Alternatives Considered

| Alternative | Rejected Because |
|------------|-----------------|
| Generic unit library only | Good for physics units, but not sufficient for ingredient-specific densities, vague culinary units, or package aliases. |
| Assume a global density | Produces confidently wrong answers for many common ingredients. |
| Normalize everything to grams | Loses user intent, mishandles vague units, and breaks natural recipe presentation. |
| Best-effort approximate conversion | Hides uncertainty behind fake precision and erodes trust in scaling and grocery outputs. |

## Consequences

- Strong upside: fond becomes correct-or-honest, which is critical for cooking trust.
- Strong upside: later pantry, grocery, and scaling features can share one consistent quantity model.
- Tradeoff: usefulness depends heavily on the quality and coverage of the bundled density and ontology data.
- Tradeoff: users will sometimes see explicit "cannot convert" messages until the reference dataset grows.
