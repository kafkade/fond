# ADR-009: Pantry & Grocery Model — Presence-First, Opt-In Quantity

**Status**: Proposed
**Date**: 2025-07-13
**Decision**: Make pantry tracking presence-first with optional quantities, and require explicit confirmation before any consumption deduction or grocery subtraction becomes authoritative.

## Context

The roadmap is explicit that pantry value is high but pantry tedium is dangerous. Section 18 names it directly as failure mode F3: **tedium is the #1 pantry failure mode**, and a precise-but-burdensome inventory system is more likely to be abandoned than used.

Section 3A.4 and Section 4 frame the practical opportunity. Users still get immediate value from simple "have/don't have" presence data through pantry coverage percentages, "what can I cook tonight?" ranking, and basic grocery deltas, even if they never enter weights, expiry dates, or par levels.

That suggests a deliberate asymmetry: reads should be useful with almost no data entry, while precision should be optional for power users. The pantry model has to reward low-effort usage instead of demanding spreadsheet-grade maintenance up front.

## Decision

fond will use a **presence-first, opt-in quantity** pantry model. A `PantryItem` records `present` by default, with `quantity`, `unit`, `expiry`, and `par_level` as optional enhancements.

Coverage percentage and basic pantry-aware suggestions work from presence alone. When a user cooks a recipe, fond may propose quantity deductions or pantry updates, but any deduction requires **explicit confirmation** rather than silent mutation. Grocery generation subtracts only what the pantry model can support honestly and leaves uncertainty visible instead of pretending precision.

## Rationale

- **Minimizes friction**: users can get value from the pantry without turning it into a second job.
- **Protects trust**: manual confirmation prevents the model from drifting invisibly away from kitchen reality.
- **Supports gradual adoption**: power users can add quantity, expiry, and par levels only where it matters.
- **Unlocks valuable reads early**: coverage %, grocery deltas, and pantry-aware ranking do not require perfect data.
- **Fits the roadmap**: this is the explicit mitigation for failure mode F3.

## Alternatives Considered

| Alternative | Rejected Because |
|------------|-----------------|
| Mandatory quantity tracking | Maximizes precision on paper, but strongly predicts abandonment in real households. |
| Fully automatic pantry deduction | Feels magical at first, then drifts from reality and undermines user trust. |
| Quantity-first pantry with presence implied | Front-loads data entry instead of delivering early low-effort wins. |
| No pantry model | Gives up meal-planning, coverage, and grocery-list value that the product promises. |

## Consequences

- Strong upside: pantry adoption is more likely because the first step is lightweight.
- Strong upside: grocery and meal-planning features can grow from a trustworthy baseline instead of brittle false precision.
- Tradeoff: some grocery results will remain approximate until the user opts into quantity data and the units engine matures.
- Tradeoff: pantry workflows depend on the ontology and unit-conversion layers to become truly powerful at scale.
