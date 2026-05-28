# ADR-008: Cooking Timeline Engine — DAG Backward Scheduling

**Status**: Proposed
**Date**: 2025-07-13
**Decision**: Represent recipe steps as a directed acyclic graph and compute backward schedules from a target serve time, leaving untimed steps explicit rather than fabricated.

## Context

Principle #8 is one of fond's clearest differentiators: the application should help people actually cook, not just store recipe text. Section 3A.3 explains why naive timeline math fails: active and passive time are different, many steps can overlap, and dependencies matter more than the raw sum of durations.

The roadmap also makes the user need concrete. `fond cook X --serve-at 19:00` must answer "when do I start?" rather than merely "how long does this recipe take?" That requires backward scheduling from the desired finish time, not just a forward timer list.

Timing data is imperfect. Some recipes include `~timer{}` annotations, some imply timing in prose, and some leave timing intentionally vague ("cook until done"). The engine therefore needs a model that can reason precisely where data exists without inventing precision where it does not.

## Decision

fond will model a recipe timeline as a **directed acyclic graph (DAG)** of steps, where each node carries at least `{duration, task_type, depends_on}` and task types distinguish active-prep, passive-prep, active-cook, passive-cook, and rest.

```text
marinate (8h passive) ──► sear chicken (10m active) ──► rest (10m passive) ──► serve
cook rice (30m passive-cook) ─────────────────────────┘
```

Given a target eat-time, the scheduler will run a **reverse topological pass**: start from the serve node, compute each successor's latest allowable start, then propagate backward so each prerequisite gets its latest-start time. Durations come first from `~timer{}` annotations and second from conservative heuristics over step text. If no trustworthy duration is available, the step remains untimed and is shown as such rather than being assigned a fake number.

## Rationale

- **Models real cooking**: dependencies and parallelism matter more than the total minutes listed on a recipe card.
- **Answers the right question**: backward scheduling supports "when do I begin?" and "what overlaps?" directly.
- **Honesty over guesswork**: untimed steps stay untimed instead of poisoning the whole schedule with invented data.
- **Incremental delivery**: single-recipe DAG scheduling can ship in Phase 2 before multi-recipe coordination exists.
- **Extensible foundation**: oven/stove contention and multi-recipe merging can later build on the same graph model.

## Alternatives Considered

| Alternative | Rejected Because |
|------------|-----------------|
| Flat sum of durations | Ignores overlap and passive time, so it routinely overestimates and misleads. |
| Forward-only scheduling | Can sequence steps, but does not directly answer target serve-time planning. |
| ML timing prediction | No training data, poor explainability, and excessive complexity for an early-stage cooking tool. |
| Force every step to have a duration | Encourages fabricated timing for subjective steps like "cook until done." |

## Consequences

- Strong upside: fond gains a genuinely differentiated cook-planning capability instead of a timer list masquerading as a schedule.
- Strong upside: the model can power both CLI output and later TUI/web cook modes.
- Tradeoff: heuristic duration extraction remains imperfect and must be clearly labeled as such.
- Tradeoff: multi-recipe coordination and resource contention are explicitly deferred because they are a harder Phase 8 problem.
