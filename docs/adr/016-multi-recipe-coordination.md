# ADR-016: Multi-Recipe Meal Coordination — Resource-Aware Backward Scheduling

**Status**: Proposed
**Date**: 2025-07-13
**Decision**: Merge N single-recipe timeline DAGs into one meal that shares a target serve time, and resolve finite kitchen-resource contention (oven, burners, cook attention) with a backward list-scheduling heuristic that reports unavoidable conflicts honestly instead of fabricating a feasible-looking schedule.

## Context

ADR-008 built single-recipe backward scheduling and explicitly deferred multi-recipe coordination to Phase 8 as "a harder problem." That problem is the one people actually have on a holiday: the turkey, the stuffing, the potatoes, and the greens all need to be hot at 7:00 PM, but there is one oven, four burners, and one pair of hands. Summing four independent timelines is worse than useless — it ignores that the dishes compete for the same physical resources and must all converge on a single eat-time.

Coordinating multiple recipes is fundamentally a **Resource-Constrained Project Scheduling Problem (RCPSP)**: tasks with durations and precedence constraints competing for renewable resources of finite capacity. RCPSP is NP-hard in general, so an exact optimal solver is out of scope for a local-first cooking tool. What the user needs is not provable optimality but a *plausible, honest* plan: when do I start each dish, what overlaps, and where does the kitchen physically not have enough oven/burner/hands so I should adjust (start earlier, hold a dish warm, or borrow a second oven)?

Principle #8 (help people actually cook) and the honesty principle from ADR-008 (untimed stays untimed; never fabricate precision) both carry directly into the multi-recipe case: a coordinator that quietly overlaps two dishes in one oven at incompatible temperatures would be *worse* than no coordinator at all.

## Decision

fond models a coordinated meal as a **merged DAG** over all selected recipes and schedules it with a **resource-aware backward list-scheduling heuristic**, layered additively on the existing single-recipe engine in `fond-timeline`. The single-recipe path is untouched.

### Resource model

Each timeline node carries an inferred `ResourceRequirement { kind, oven_temp?, needs_cook }`:

- **Oven** — capacity 1 by default, and **temperature-exclusive**: two oven tasks may overlap only if their requested temperatures are compatible (within a tolerance band, `COMPAT_TOLERANCE_F = 25°F`). Different temperatures overlapping is a conflict, not a silent merge.
- **Stove burners** — 4 concurrent stove tasks by default.
- **Cook attention** — 1 by default: at most one *active* (hands-on) task at a time. Passive steps (marinate, rest, "bake 40 min") hold their appliance but consume no attention.

Defaults are CLI-overridable (`--ovens`, `--burners`, `--cooks`). Requirements are inferred from `TaskType` plus body/timer keywords and a parsed oven temperature (`425°F`, `180C`, `gas mark 6`). Untimed steps stay untimed and are never resource-scheduled — the honesty principle from ADR-008 is preserved end-to-end.

### Merged scheduling algorithm

1. Build each recipe's `Timeline` independently (existing `build_timeline`).
2. **Merge** into one `MealTimeline`: re-index `NodeId`s into a global space, offset intra-recipe dependencies, and tag each node with its source recipe. No cross-recipe dependencies are invented — each recipe's serve node targets the shared eat-time.
3. Compute a **resource-free backward pass** to get each node's *ideal* latest end (its position if the kitchen were infinite) — this becomes the scheduling priority and defines each node's dependency slack.
4. **Resource-resolution pass** in reverse-topological order (sinks first, ordered by ideal end, then duration, then id): place each timed, resource-using node as late as possible subject to (a) successor start times and (b) resource availability. When a slot is over capacity, pull the node earlier along its slack to the next feasible window, checking oven-temperature clustering, burner count, and cook attention at each candidate.
5. **Conflict detection**: when a node can only be placed by forcing it earlier than its ideal window because a resource is saturated (or an oven-temperature clash is unavoidable), emit a `Conflict` naming both dishes, the contended resource, and the reason (`OvenTemperature`, `OvenCapacity`, `BurnerCapacity`, `CookAttention`). Conflicts are surfaced, never silently resolved.
6. Produce a `ScheduledMeal { serve_at, start_at, per-recipe + merged nodes, reservations, conflicts, totals }`.

### Surfaces

- **CLI**: `fond cook a b c --serve-at 19:00` (1..N slugs; a single slug keeps today's exact behavior) with `--ovens/--burners/--cooks`. Static output is a coordinated table with Recipe and Resource columns, a resource-lane summary, and a clearly separated conflicts section; `--format json` serializes the full `ScheduledMeal`.
- **TUI**: cook mode drives the merged plan by *flattening* it into a single synthetic `Recipe` + matching `ScheduledTimeline` ordered by scheduled start, each step labeled with its recipe and resource. This reuses the entire existing cook-mode UI unchanged; per-recipe cook logs are recorded on completion.

## Rationale

- **Solves the real holiday problem**: convergence on one eat-time with finite oven/stove/hands is the differentiating "moonshot" from ROADMAP §3A.3 / Phase 8.
- **Honesty over false precision**: an unavoidable oven-temperature clash is *reported*, matching ADR-008's untimed-stays-untimed stance; the tool never pretends two dishes fit one oven at two temperatures.
- **Additive, low-risk**: the resource model and coordinator are new `fond-timeline` modules; the single-recipe schedule is byte-for-byte unchanged and all new serialized fields are `#[serde(default)]`.
- **Right complexity for the domain**: a deadline-driven list-scheduling heuristic is fast, deterministic, explainable, and offline — appropriate for a local-first CLI, where a full RCPSP optimizer would be overkill and unexplainable.
- **Reuse**: flattening the merged plan into one synthetic recipe lets the existing TUI and FFI cook surfaces drive a coordinated meal with no duplicated UI.

## Alternatives Considered

| Alternative | Rejected Because |
|------------|-----------------|
| Exact RCPSP / ILP / CP-SAT optimizer | NP-hard; heavyweight solver dependency, poor explainability, and marginal real-world benefit over a good heuristic for household-scale meals. |
| Independent per-recipe timelines shown side by side | Ignores the entire point — no shared resources, no contention resolution, no single start plan. |
| Silently overlap oven tasks / average temperatures | Violates physical reality and the honesty principle; produces confidently wrong plans. |
| Forward scheduling from "now" | Cannot answer "when do I start so everything is ready at 7:00?"; the backward pass is the whole value. |
| Fabricate durations for untimed steps to make the DAG fully schedulable | Poisons the plan with invented precision — same failure ADR-008 already rejected. |

## Consequences

- Strong upside: fond gains its headline Phase 8 capability — genuine multi-dish meal coordination — on top of the existing timeline engine, across CLI, TUI, and FFI.
- Strong upside: conflicts become actionable information ("the pie and the turkey both want the oven at different temps") instead of a hidden scheduling lie.
- Tradeoff: the scheduler is a **heuristic, not an optimizer** — it can produce a feasible, sensible plan that is not provably minimal-makespan, and pathological inputs may report a conflict a cleverer solver could have avoided. This is documented, not hidden.
- Tradeoff: resource inference depends on keyword/temperature extraction from step text, which is imperfect; misclassified steps degrade gracefully (treated as no-resource) rather than corrupting the schedule.
- Tradeoff: end-to-end timing accuracy of a coordinated meal is `[Validation Required]` — it inherits the single-recipe heuristic-duration caveat from ADR-008 and adds resource-contention shifts on top.
