# Due Diligence: Ingredient Substitution Reference Dataset

**Date**: 2026-07-01
**Status**: Seed dataset — pending external validation
**Related Issue**: [#78](https://github.com/kafkade/fond/issues/78)
**Roadmap References**: §6.2 (ingredient substitution engine),
§3A.1 (ingredient ontology)

## Summary

fond ships a curated, **advisory** ingredient substitution reference so that
`fond substitute <ingredient>` can answer "out of buttermilk? use milk + lemon
juice" with ranked, sourced, context-aware options. This is a **reference
dataset, not a generative model** (ROADMAP §6.2). Substitutions are never
auto-applied to a `.cook` file — the command is read-only and prints a
disclaimer.

## Design constraints (from ROADMAP §6.2)

| Constraint | How it is met |
|------------|---------------|
| Curated, not generative | Static, hand-authored JSON dataset (`data/substitutions/substitutions.json`) |
| Context-aware (baking vs. sauteing) | Each option is tagged with one or more `contexts` (`baking` / `sauteing` / `general`) |
| Ranked | Each option carries a `rank`; the CLI orders by rank, then by context relevance |
| Sourced | Each option carries a `source` citation |
| Advisory / reversible | Read-only command; disclaimer footer; never mutates a recipe |
| Local-first | Embedded at compile time via `include_str!`; no network access |

## Licensing analysis

- **Substitution ratios are facts.** A conversion such as "1 cup buttermilk =
  1 cup milk + 1 tbsp lemon juice" is an unprotectable fact/idea, not
  copyrightable expression (cf. *Feist v. Rural*). Facts and simple
  measurement conversions are not owned by any publisher.
- **All prose is original.** The `ratio` and `caveat` text is authored for
  fond; no copyrighted descriptions, tables, or article text were copied.
- **`source` fields are attributions, not excerpts.** They name the general
  authority commonly associated with a technique (King Arthur Baking,
  America's Test Kitchen, Serious Eats, or "common practice"), not a quoted
  passage.
- **Compatibility.** The dataset is released as original work under fond's MIT
  license (see `LICENSE`, `THIRD_PARTY.md`).

## Validation status ⚠️

The issue carries the `validation` label ("Needs external validation before
proceeding"). The current dataset is a **seed** of ~17 common ingredients and
~40 substitutions. Before these ratios are treated as authoritative:

- [ ] Cross-check each ratio against at least two independent published sources.
- [ ] Have a cook/baker review the baking caveats for correctness.
- [ ] Confirm context tags (especially which swaps are safe for baking).

Until then, the CLI presents every result as advisory and disclaims it.

## Data model

`data/substitutions/substitutions.json`:

```jsonc
{
  "schema_version": 1,
  "description": "...",
  "entries": [
    {
      "canonical": "buttermilk",
      "aliases": ["cultured buttermilk"],
      "substitutions": [
        {
          "substitute": "milk + lemon juice",
          "ratio": "1 cup buttermilk = 1 cup milk + 1 tbsp lemon juice; ...",
          "contexts": ["baking", "general"],
          "caveat": "In baking, the acidity reacts with baking soda for lift ...",
          "rank": 1,
          "source": "King Arthur Baking Company"
        }
      ]
    }
  ]
}
```

The dataset is parsed by `fond-core::substitution`, which resolves aliases and
singular/plural forms, filters/prioritizes by cooking context, and returns
ranked results. It is **user-extendable in a later phase**; the seed ships now
and grows per phase (consistent with the ingredient-ontology approach in
`ingredient-dataset-review.md`).
