//! Multi-recipe meal coordination: merge several single-recipe timelines into
//! one meal schedule and resolve oven/stove/cook contention.
//!
//! Cooking several dishes for one meal means merging their timeline DAGs and
//! sharing finite kitchen resources. This module:
//!
//! 1. [`merge_timelines`] combines N [`Timeline`]s into one [`MealTimeline`],
//!    re-indexing node ids and tagging each node with its source recipe.
//! 2. [`schedule_meal`] backward-schedules the merged plan from a target serve
//!    time using a **resource-aware list-scheduling heuristic**: each timed
//!    step is placed as late as possible near the deadline, subject to its
//!    successors and to resource availability; when a resource is contended the
//!    more-flexible step is pulled earlier, and the contention is reported as a
//!    [`Conflict`] rather than silently ignored.
//!
//! The scheduler is a heuristic, not an optimal solver — resource-constrained
//! project scheduling is NP-hard. It favors honesty (report what it moved and
//! why) over the illusion of a perfect plan. See ADR-016.

use chrono::{Duration, NaiveDateTime};
use serde::{Deserialize, Serialize};

use crate::model::{NodeId, ScheduledNode, Timeline, TimelineNode};
use crate::resource::{KitchenResources, OvenTemp, ResourceKind};

/// Provenance for one recipe contributing to a meal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeSource {
    pub title: String,
    pub slug: String,
}

/// A merged, unscheduled multi-recipe timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MealTimeline {
    /// One entry per contributing recipe, indexed by `recipe_index`.
    pub sources: Vec<RecipeSource>,
    /// All nodes across all recipes, with globally re-indexed ids.
    pub nodes: Vec<MealNode>,
}

/// A merged timeline node tagged with its source recipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MealNode {
    /// The underlying timeline node (with a globally-unique id).
    pub node: TimelineNode,
    /// Index into [`MealTimeline::sources`].
    pub recipe_index: usize,
}

/// A scheduled node in a coordinated meal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledMealNode {
    /// The underlying timeline node.
    #[serde(flatten)]
    pub node: TimelineNode,
    /// Index into [`ScheduledMeal::sources`].
    pub recipe_index: usize,
    /// Title of the recipe this step belongs to (convenience for display).
    pub recipe_title: String,
    /// Computed start time.
    pub scheduled_start: NaiveDateTime,
    /// Computed end time.
    pub scheduled_end: NaiveDateTime,
}

/// The kind of resource contention that was detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConflictKind {
    /// Two dishes needed the oven at incompatible temperatures at the same time.
    OvenTemperature,
    /// More dishes needed the oven simultaneously than there are ovens.
    OvenCapacity,
    /// More dishes needed the stove simultaneously than there are burners.
    BurnerCapacity,
    /// More hands-on steps overlapped than there are cooks.
    CookAttention,
}

impl ConflictKind {
    pub fn label(&self) -> &'static str {
        match self {
            ConflictKind::OvenTemperature => "oven temperature",
            ConflictKind::OvenCapacity => "oven capacity",
            ConflictKind::BurnerCapacity => "burner capacity",
            ConflictKind::CookAttention => "cook attention",
        }
    }
}

/// A reported resource contention between two dishes.
///
/// The scheduler resolves contention by moving the flagged step (`recipe_a` /
/// `step_a`) earlier than its ideal slot so `recipe_b` / `step_b` can keep the
/// contended resource. It is reported so the cook knows the plan is a
/// compromise, not a fabricated "everything fits perfectly" schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    pub kind: ConflictKind,
    pub resource: ResourceKind,
    /// The dish that was moved earlier to resolve the contention.
    pub recipe_a: String,
    pub step_a: String,
    /// The dish that kept the contended resource at the ideal time.
    pub recipe_b: String,
    pub step_b: String,
    /// Human-readable explanation.
    pub detail: String,
}

/// A fully scheduled, coordinated meal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledMeal {
    /// Contributing recipes, indexed by `recipe_index`.
    pub sources: Vec<RecipeSource>,
    /// Target serve time — all dishes aim to finish here.
    pub serve_at: NaiveDateTime,
    /// Earliest start across all steps.
    pub start_at: NaiveDateTime,
    /// Resource configuration used for scheduling.
    pub resources: KitchenResources,
    /// Scheduled steps, ordered by start time.
    pub nodes: Vec<ScheduledMealNode>,
    /// Resource contentions the scheduler detected and resolved.
    pub conflicts: Vec<Conflict>,
    /// Sum of active (hands-on) durations, seconds.
    pub total_active_seconds: u64,
    /// Sum of passive (hands-off) durations, seconds.
    pub total_passive_seconds: u64,
    /// Whether any step has unknown duration.
    pub has_untimed_steps: bool,
}

/// Merge several single-recipe timelines into one meal timeline.
///
/// Node ids are re-indexed into a single global space and dependency edges are
/// offset accordingly. No cross-recipe dependencies are introduced — each
/// recipe simply shares the meal's target serve time.
pub fn merge_timelines(timelines: &[Timeline]) -> MealTimeline {
    let mut sources = Vec::with_capacity(timelines.len());
    let mut nodes = Vec::new();
    let mut offset = 0usize;

    for (recipe_index, tl) in timelines.iter().enumerate() {
        sources.push(RecipeSource {
            title: tl.recipe_title.clone(),
            slug: tl.recipe_slug.clone(),
        });

        for node in &tl.nodes {
            let mut n = node.clone();
            n.id = NodeId(offset + node.id.0);
            n.depends_on = node
                .depends_on
                .iter()
                .map(|d| NodeId(offset + d.0))
                .collect();
            nodes.push(MealNode {
                node: n,
                recipe_index,
            });
        }

        offset += tl.nodes.len();
    }

    MealTimeline { sources, nodes }
}

/// A resource reservation on the timeline.
#[derive(Debug, Clone)]
struct Reservation {
    node: usize,
    resource: ResourceKind,
    start: NaiveDateTime,
    end: NaiveDateTime,
    oven_temp: Option<OvenTemp>,
}

/// Backward-schedule a merged meal timeline, resolving resource contention.
pub fn schedule_meal(
    meal: &MealTimeline,
    serve_at: NaiveDateTime,
    resources: KitchenResources,
) -> ScheduledMeal {
    let n = meal.nodes.len();

    if n == 0 {
        return ScheduledMeal {
            sources: meal.sources.clone(),
            serve_at,
            start_at: serve_at,
            resources,
            nodes: vec![],
            conflicts: vec![],
            total_active_seconds: 0,
            total_passive_seconds: 0,
            has_untimed_steps: false,
        };
    }

    // Map global NodeId -> index. Ids are contiguous 0..n after merge, but be
    // defensive and build an explicit lookup.
    let mut id_to_idx = std::collections::HashMap::with_capacity(n);
    for (i, mn) in meal.nodes.iter().enumerate() {
        id_to_idx.insert(mn.node.id.0, i);
    }

    // Predecessors (dependencies) and successors, by index.
    let mut deps: Vec<Vec<usize>> = vec![vec![]; n];
    let mut successors: Vec<Vec<usize>> = vec![vec![]; n];
    for (i, mn) in meal.nodes.iter().enumerate() {
        for d in &mn.node.depends_on {
            if let Some(&di) = id_to_idx.get(&d.0) {
                deps[i].push(di);
                successors[di].push(i);
            }
        }
    }

    // Resource-free backward pass → ideal latest end, used to prioritize which
    // independent node claims the latest slot first.
    let ideal_end = ideal_backward_ends(meal, serve_at, &successors, &deps);

    // Placement state.
    let mut placed_start: Vec<NaiveDateTime> = vec![serve_at; n];
    let mut placed_end: Vec<NaiveDateTime> = vec![serve_at; n];
    let mut placed = vec![false; n];
    let mut reservations: Vec<Reservation> = Vec::new();
    let mut conflicts: Vec<Conflict> = Vec::new();

    // Process sinks first (reverse topological). A node is ready once all its
    // successors are placed.
    let mut unplaced_succ: Vec<usize> = successors.iter().map(|s| s.len()).collect();

    for _ in 0..n {
        // Ready = unplaced nodes with all successors placed.
        let next = (0..n)
            .filter(|&i| !placed[i] && unplaced_succ[i] == 0)
            .max_by(|&a, &b| {
                // Latest ideal finish first; then longer duration; then lower id.
                ideal_end[a]
                    .cmp(&ideal_end[b])
                    .then(node_seconds(&meal.nodes[a].node).cmp(&node_seconds(&meal.nodes[b].node)))
                    .then(b.cmp(&a))
            });

        let i = match next {
            Some(i) => i,
            None => break, // cycle guard (shouldn't happen for valid DAGs)
        };

        // Upper bound for this node's end: no later than serve, and no later
        // than the earliest already-placed successor's start.
        let mut end_cap = serve_at;
        for &s in &successors[i] {
            if placed_start[s] < end_cap {
                end_cap = placed_start[s];
            }
        }

        let node = &meal.nodes[i].node;
        match node.duration.as_ref().map(|d| d.seconds) {
            None => {
                // Untimed: anchor at the cap, occupy no resource.
                placed_start[i] = end_cap;
                placed_end[i] = end_cap;
            }
            Some(secs) => {
                let dur = Duration::seconds(secs as i64);
                let (start, end, mut found) =
                    place_timed(i, node, end_cap, dur, &reservations, &resources, meal);
                placed_start[i] = start;
                placed_end[i] = end;
                conflicts.append(&mut found);

                // Reserve the resources this node occupies.
                if node.resource.is_oven() {
                    reservations.push(Reservation {
                        node: i,
                        resource: ResourceKind::Oven,
                        start,
                        end,
                        oven_temp: node.resource.oven_temp.clone(),
                    });
                } else if node.resource.is_stove() {
                    reservations.push(Reservation {
                        node: i,
                        resource: ResourceKind::Stove,
                        start,
                        end,
                        oven_temp: None,
                    });
                }
                if node.resource.needs_cook {
                    reservations.push(Reservation {
                        node: i,
                        resource: ResourceKind::Cook,
                        start,
                        end,
                        oven_temp: None,
                    });
                }
            }
        }

        placed[i] = true;
        for &d in &deps[i] {
            unplaced_succ[d] = unplaced_succ[d].saturating_sub(1);
        }
    }

    // Build output nodes, ordered by start time.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        placed_start[a]
            .cmp(&placed_start[b])
            .then(meal.nodes[a].recipe_index.cmp(&meal.nodes[b].recipe_index))
            .then(
                meal.nodes[a]
                    .node
                    .step_index
                    .cmp(&meal.nodes[b].node.step_index),
            )
    });

    let mut total_active = 0u64;
    let mut total_passive = 0u64;
    let mut has_untimed = false;

    let out_nodes: Vec<ScheduledMealNode> = order
        .iter()
        .map(|&i| {
            let mn = &meal.nodes[i];
            match mn.node.duration.as_ref() {
                Some(d) => {
                    if mn.node.task_type.is_active() {
                        total_active += d.seconds;
                    } else {
                        total_passive += d.seconds;
                    }
                }
                None => has_untimed = true,
            }
            ScheduledMealNode {
                node: mn.node.clone(),
                recipe_index: mn.recipe_index,
                recipe_title: meal.sources[mn.recipe_index].title.clone(),
                scheduled_start: placed_start[i],
                scheduled_end: placed_end[i],
            }
        })
        .collect();

    let start_at = placed_start.iter().copied().min().unwrap_or(serve_at);

    // Deduplicate conflicts (same pair + kind can be recorded once).
    dedup_conflicts(&mut conflicts);

    ScheduledMeal {
        sources: meal.sources.clone(),
        serve_at,
        start_at,
        resources,
        nodes: out_nodes,
        conflicts,
        total_active_seconds: total_active,
        total_passive_seconds: total_passive,
        has_untimed_steps: has_untimed,
    }
}

fn node_seconds(node: &TimelineNode) -> u64 {
    node.duration.as_ref().map(|d| d.seconds).unwrap_or(0)
}

/// Resource-free backward pass: each node's latest end = min successor latest
/// start (capped at serve), latest start = end - duration.
fn ideal_backward_ends(
    meal: &MealTimeline,
    serve_at: NaiveDateTime,
    successors: &[Vec<usize>],
    deps: &[Vec<usize>],
) -> Vec<NaiveDateTime> {
    let n = meal.nodes.len();
    let mut latest_end = vec![serve_at; n];
    let mut latest_start = vec![serve_at; n];

    // Reverse topological order via Kahn on successors.
    let mut out_degree: Vec<usize> = successors.iter().map(|s| s.len()).collect();
    let mut queue: std::collections::VecDeque<usize> =
        (0..n).filter(|&i| out_degree[i] == 0).collect();

    while let Some(i) = queue.pop_front() {
        if !successors[i].is_empty() {
            latest_end[i] = successors[i]
                .iter()
                .map(|&s| latest_start[s])
                .min()
                .unwrap_or(serve_at);
        }
        latest_start[i] = match meal.nodes[i].node.duration.as_ref() {
            Some(d) => latest_end[i] - Duration::seconds(d.seconds as i64),
            None => latest_end[i],
        };
        for &d in &deps[i] {
            out_degree[d] -= 1;
            if out_degree[d] == 0 {
                queue.push_back(d);
            }
        }
    }

    latest_end
}

/// Place a timed node as late as possible subject to resource availability.
///
/// Returns `(start, end, conflicts)` where `conflicts` records the contentions
/// that forced the node earlier than its ideal (deadline-hugging) slot.
fn place_timed(
    i: usize,
    node: &TimelineNode,
    end_cap: NaiveDateTime,
    dur: Duration,
    reservations: &[Reservation],
    kitchen: &KitchenResources,
    meal: &MealTimeline,
) -> (NaiveDateTime, NaiveDateTime, Vec<Conflict>) {
    // Candidate end times, latest first: the cap, plus the start of every
    // relevant existing reservation (moving our end to a reservation's start
    // clears the overlap with it).
    let mut candidates = vec![end_cap];
    for r in reservations {
        if relevant(node, r) && r.start < end_cap {
            candidates.push(r.start);
        }
    }
    candidates.sort_by(|a, b| b.cmp(a));
    candidates.dedup();

    for &end in &candidates {
        let start = end - dur;
        if feasible(node, start, end, reservations, kitchen) {
            // If we had to move earlier than the ideal slot, record why.
            let conflicts = if end < end_cap {
                blockers(i, node, end_cap - dur, end_cap, reservations, kitchen, meal)
            } else {
                vec![]
            };
            return (start, end, conflicts);
        }
    }

    // Fallback (should be unreachable): place at the cap.
    (end_cap - dur, end_cap, vec![])
}

/// Whether a reservation is on a resource dimension the node also uses.
fn relevant(node: &TimelineNode, r: &Reservation) -> bool {
    match r.resource {
        ResourceKind::Oven => node.resource.is_oven(),
        ResourceKind::Stove => node.resource.is_stove(),
        ResourceKind::Cook => node.resource.needs_cook,
    }
}

fn overlaps(
    a_start: NaiveDateTime,
    a_end: NaiveDateTime,
    b_start: NaiveDateTime,
    b_end: NaiveDateTime,
) -> bool {
    a_start < b_end && b_start < a_end
}

/// Whether a node can occupy `[start, end)` given existing reservations.
fn feasible(
    node: &TimelineNode,
    start: NaiveDateTime,
    end: NaiveDateTime,
    reservations: &[Reservation],
    kitchen: &KitchenResources,
) -> bool {
    // Stove burners.
    if node.resource.is_stove() {
        let count = reservations
            .iter()
            .filter(|r| r.resource == ResourceKind::Stove && overlaps(start, end, r.start, r.end))
            .count() as u32;
        if count >= kitchen.burners {
            return false;
        }
    }

    // Cook attention.
    if node.resource.needs_cook {
        let count = reservations
            .iter()
            .filter(|r| r.resource == ResourceKind::Cook && overlaps(start, end, r.start, r.end))
            .count() as u32;
        if count >= kitchen.cooks {
            return false;
        }
    }

    // Oven: distinct temperature groups (within tolerance) must fit in ovens.
    if node.resource.is_oven() {
        let mut temps: Vec<i32> = reservations
            .iter()
            .filter(|r| r.resource == ResourceKind::Oven && overlaps(start, end, r.start, r.end))
            .filter_map(|r| r.oven_temp.as_ref().map(|t| t.fahrenheit))
            .collect();
        if let Some(t) = node.resource.oven_temp.as_ref() {
            temps.push(t.fahrenheit);
        }
        if oven_temp_groups(&mut temps) > kitchen.ovens as usize {
            return false;
        }
    }

    true
}

/// Number of distinct temperature clusters, where temps within
/// [`OvenTemp::COMPAT_TOLERANCE_F`] of each other join the same cluster.
fn oven_temp_groups(temps: &mut [i32]) -> usize {
    if temps.is_empty() {
        return 0;
    }
    temps.sort_unstable();
    let mut groups = 1;
    for w in temps.windows(2) {
        if (w[1] - w[0]) > OvenTemp::COMPAT_TOLERANCE_F {
            groups += 1;
        }
    }
    groups
}

/// Identify the reservations that blocked the ideal window, as conflicts.
fn blockers(
    i: usize,
    node: &TimelineNode,
    ideal_start: NaiveDateTime,
    ideal_end: NaiveDateTime,
    reservations: &[Reservation],
    kitchen: &KitchenResources,
    meal: &MealTimeline,
) -> Vec<Conflict> {
    let mut out = Vec::new();
    let a_title = meal.sources[meal.nodes[i].recipe_index].title.clone();
    let a_step = node.label.clone();

    // Oven temperature / capacity.
    if node.resource.is_oven() {
        let overlapping: Vec<&Reservation> = reservations
            .iter()
            .filter(|r| {
                r.resource == ResourceKind::Oven && overlaps(ideal_start, ideal_end, r.start, r.end)
            })
            .collect();

        // Incompatible temperature with a specific dish.
        if let Some(t) = node.resource.oven_temp.as_ref() {
            for r in &overlapping {
                if let Some(rt) = r.oven_temp.as_ref()
                    && !t.is_compatible_with(rt)
                {
                    let b = &meal.nodes[r.node];
                    out.push(Conflict {
                        kind: ConflictKind::OvenTemperature,
                        resource: ResourceKind::Oven,
                        recipe_a: a_title.clone(),
                        step_a: a_step.clone(),
                        recipe_b: meal.sources[b.recipe_index].title.clone(),
                        step_b: b.node.label.clone(),
                        detail: format!(
                            "{} needs the oven at {} while {} needs it at {} — moved earlier to free the oven",
                            a_title, t, meal.sources[b.recipe_index].title, rt
                        ),
                    });
                }
            }
        }

        // Plain capacity overload with compatible/unknown temps.
        if out.is_empty() && !overlapping.is_empty() && kitchen.ovens <= overlapping.len() as u32 {
            let r = overlapping[0];
            let b = &meal.nodes[r.node];
            out.push(Conflict {
                kind: ConflictKind::OvenCapacity,
                resource: ResourceKind::Oven,
                recipe_a: a_title.clone(),
                step_a: a_step.clone(),
                recipe_b: meal.sources[b.recipe_index].title.clone(),
                step_b: b.node.label.clone(),
                detail: format!(
                    "not enough oven space for both {} and {} at once — moved earlier",
                    a_title, meal.sources[b.recipe_index].title
                ),
            });
        }
    }

    // Burner capacity.
    if node.resource.is_stove() {
        let overlapping: Vec<&Reservation> = reservations
            .iter()
            .filter(|r| {
                r.resource == ResourceKind::Stove
                    && overlaps(ideal_start, ideal_end, r.start, r.end)
            })
            .collect();
        if overlapping.len() as u32 >= kitchen.burners {
            let r = overlapping[0];
            let b = &meal.nodes[r.node];
            out.push(Conflict {
                kind: ConflictKind::BurnerCapacity,
                resource: ResourceKind::Stove,
                recipe_a: a_title.clone(),
                step_a: a_step.clone(),
                recipe_b: meal.sources[b.recipe_index].title.clone(),
                step_b: b.node.label.clone(),
                detail: format!(
                    "all {} burner(s) busy — {} moved earlier around {}",
                    kitchen.burners, a_title, meal.sources[b.recipe_index].title
                ),
            });
        }
    }

    // Cook attention.
    if node.resource.needs_cook {
        let overlapping: Vec<&Reservation> = reservations
            .iter()
            .filter(|r| {
                r.resource == ResourceKind::Cook && overlaps(ideal_start, ideal_end, r.start, r.end)
            })
            .collect();
        if overlapping.len() as u32 >= kitchen.cooks {
            let r = overlapping[0];
            let b = &meal.nodes[r.node];
            out.push(Conflict {
                kind: ConflictKind::CookAttention,
                resource: ResourceKind::Cook,
                recipe_a: a_title.clone(),
                step_a: a_step.clone(),
                recipe_b: meal.sources[b.recipe_index].title.clone(),
                step_b: b.node.label.clone(),
                detail: format!(
                    "you can't do {} and {} hands-on at once — one shifted earlier",
                    a_step, b.node.label
                ),
            });
        }
    }

    out
}

fn dedup_conflicts(conflicts: &mut Vec<Conflict>) {
    let mut seen = std::collections::HashSet::new();
    conflicts.retain(|c| {
        let key = (c.kind, c.step_a.clone(), c.step_b.clone());
        seen.insert(key)
    });
}

/// Convenience: build scheduled single-recipe-style nodes from a meal (used by
/// callers that want a flat list). Kept lightweight; presentation lives in the
/// CLI/TUI layers.
impl ScheduledMeal {
    /// Convert to a flat list of [`ScheduledNode`]s (dropping recipe tags),
    /// ordered by start time. Useful for reusing single-recipe rendering.
    pub fn as_scheduled_nodes(&self) -> Vec<ScheduledNode> {
        self.nodes
            .iter()
            .map(|mn| ScheduledNode {
                node: mn.node.clone(),
                scheduled_start: mn.scheduled_start,
                scheduled_end: mn.scheduled_end,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, NaiveTime};
    use fond_domain::{Recipe, Step, Timer};

    use super::*;
    use crate::build::build_timeline;

    fn recipe(slug: &str, title: &str, steps: Vec<Step>) -> Recipe {
        Recipe {
            slug: slug.into(),
            title: title.into(),
            source: None,
            source_url: None,
            description: None,
            recipe_yield: None,
            prep_time: None,
            cook_time: None,
            total_time: None,
            servings: None,
            ingredients: vec![],
            steps,
            cookware: vec![],
            tags: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            raw_source: None,
        }
    }

    fn step(order: u32, body: &str, timers: Vec<Timer>) -> Step {
        Step {
            section: None,
            body: body.into(),
            timers,
            order,
        }
    }

    fn timer(name: Option<&str>, duration: Option<&str>) -> Timer {
        Timer {
            name: name.map(String::from),
            duration: duration.map(String::from),
        }
    }

    fn serve_time(hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2025, 7, 20)
            .unwrap()
            .and_time(NaiveTime::from_hms_opt(hour, minute, 0).unwrap())
    }

    #[test]
    fn merge_reindexes_dependencies() {
        let a = build_timeline(&recipe(
            "a",
            "A",
            vec![
                step(0, "Chop", vec![]),
                step(1, "Simmer", vec![timer(None, Some("10 minutes"))]),
            ],
        ));
        let b = build_timeline(&recipe(
            "b",
            "B",
            vec![step(
                0,
                "Bake at 400F",
                vec![timer(None, Some("20 minutes"))],
            )],
        ));

        let meal = merge_timelines(&[a, b]);
        assert_eq!(meal.sources.len(), 2);
        assert_eq!(meal.nodes.len(), 3);
        // Recipe B's single node is global id 2, no deps.
        assert_eq!(meal.nodes[2].node.id, NodeId(2));
        assert_eq!(meal.nodes[2].recipe_index, 1);
        assert!(meal.nodes[2].node.depends_on.is_empty());
        // Recipe A's second node depends on global id 0.
        assert_eq!(meal.nodes[1].node.depends_on, vec![NodeId(0)]);
    }

    #[test]
    fn empty_meal() {
        let meal = merge_timelines(&[]);
        let sched = schedule_meal(&meal, serve_time(19, 0), KitchenResources::default());
        assert!(sched.nodes.is_empty());
        assert!(sched.conflicts.is_empty());
        assert_eq!(sched.start_at, serve_time(19, 0));
    }

    #[test]
    fn two_independent_stove_dishes_share_burners() {
        // Two dishes each simmering 30 min; 4 burners → no conflict, both end at serve.
        let a = build_timeline(&recipe(
            "soup",
            "Soup",
            vec![step(
                0,
                "Simmer the soup",
                vec![timer(None, Some("30 minutes"))],
            )],
        ));
        let b = build_timeline(&recipe(
            "sauce",
            "Sauce",
            vec![step(
                0,
                "Simmer the sauce",
                vec![timer(None, Some("30 minutes"))],
            )],
        ));
        let meal = merge_timelines(&[a, b]);
        let sched = schedule_meal(&meal, serve_time(19, 0), KitchenResources::default());

        assert!(sched.conflicts.is_empty());
        for node in &sched.nodes {
            assert_eq!(node.scheduled_end, serve_time(19, 0));
            assert_eq!(node.scheduled_start, serve_time(18, 30));
        }
    }

    #[test]
    fn oven_temperature_conflict_reported_and_resolved() {
        // Two dishes both want the (single) oven at incompatible temps, both
        // finishing at serve. One must move earlier; conflict is reported.
        let a = build_timeline(&recipe(
            "roast",
            "Roast",
            vec![step(
                0,
                "Roast at 450F",
                vec![timer(None, Some("30 minutes"))],
            )],
        ));
        let b = build_timeline(&recipe(
            "cake",
            "Cake",
            vec![step(
                0,
                "Bake at 325F",
                vec![timer(None, Some("30 minutes"))],
            )],
        ));
        let meal = merge_timelines(&[a, b]);
        let sched = schedule_meal(&meal, serve_time(19, 0), KitchenResources::default());

        // A single coordinated timeline is produced...
        assert_eq!(sched.nodes.len(), 2);
        // ...with the contention honestly reported.
        assert_eq!(sched.conflicts.len(), 1);
        assert_eq!(sched.conflicts[0].kind, ConflictKind::OvenTemperature);
        // The two dishes do not both occupy the oven at serve time.
        let ends: Vec<_> = sched.nodes.iter().map(|n| n.scheduled_end).collect();
        assert!(ends.contains(&serve_time(19, 0)));
        // One was pushed to end at 18:30 (before the other's 18:30 start).
        assert!(
            sched
                .nodes
                .iter()
                .any(|n| n.scheduled_end == serve_time(18, 30))
        );
    }

    #[test]
    fn same_temp_dishes_share_oven() {
        let a = build_timeline(&recipe(
            "chicken",
            "Chicken",
            vec![step(
                0,
                "Roast at 400F",
                vec![timer(None, Some("40 minutes"))],
            )],
        ));
        let b = build_timeline(&recipe(
            "potatoes",
            "Potatoes",
            vec![step(
                0,
                "Roast at 400F",
                vec![timer(None, Some("40 minutes"))],
            )],
        ));
        let meal = merge_timelines(&[a, b]);
        let sched = schedule_meal(&meal, serve_time(19, 0), KitchenResources::default());
        assert!(sched.conflicts.is_empty());
        for node in &sched.nodes {
            assert_eq!(node.scheduled_end, serve_time(19, 0));
        }
    }

    #[test]
    fn three_recipes_overlapping_oven_acceptance() {
        // Acceptance scenario: three dishes with overlapping oven needs at
        // different temps produce one coordinated backward-scheduled timeline,
        // all targeting the serve time, with conflicts clearly reported.
        let r1 = build_timeline(&recipe(
            "turkey",
            "Turkey",
            vec![step(
                0,
                "Roast at 350F",
                vec![timer(None, Some("60 minutes"))],
            )],
        ));
        let r2 = build_timeline(&recipe(
            "pie",
            "Pie",
            vec![step(
                0,
                "Bake at 425F",
                vec![timer(None, Some("45 minutes"))],
            )],
        ));
        let r3 = build_timeline(&recipe(
            "bread",
            "Bread",
            vec![step(
                0,
                "Bake at 475F",
                vec![timer(None, Some("30 minutes"))],
            )],
        ));
        let meal = merge_timelines(&[r1, r2, r3]);
        let sched = schedule_meal(&meal, serve_time(18, 0), KitchenResources::default());

        // One coordinated schedule with all three dishes.
        assert_eq!(sched.nodes.len(), 3);
        // With a single oven and three incompatible temps, contention is reported.
        assert!(!sched.conflicts.is_empty());
        // No two oven dishes overlap at incompatible temps in the final schedule.
        let oven: Vec<_> = sched
            .nodes
            .iter()
            .filter(|n| n.node.resource.is_oven())
            .collect();
        for i in 0..oven.len() {
            for j in (i + 1)..oven.len() {
                let a = &oven[i];
                let b = &oven[j];
                let overlap =
                    a.scheduled_start < b.scheduled_end && b.scheduled_start < a.scheduled_end;
                if overlap {
                    let ta = a.node.resource.oven_temp.as_ref().unwrap();
                    let tb = b.node.resource.oven_temp.as_ref().unwrap();
                    assert!(ta.is_compatible_with(tb), "incompatible oven temps overlap");
                }
            }
        }
        // Everything finishes by the serve time.
        for node in &sched.nodes {
            assert!(node.scheduled_end <= serve_time(18, 0));
        }
    }

    #[test]
    fn dependencies_preserved_within_recipe() {
        let a = build_timeline(&recipe(
            "adobo",
            "Adobo",
            vec![
                step(0, "Marinate", vec![timer(Some("marinate"), Some("1 hour"))]),
                step(1, "Simmer", vec![timer(None, Some("35 minutes"))]),
            ],
        ));
        let meal = merge_timelines(&[a]);
        let sched = schedule_meal(&meal, serve_time(19, 0), KitchenResources::default());
        // Simmer 18:25→19:00, marinate 17:25→18:25.
        let simmer = sched
            .nodes
            .iter()
            .find(|n| n.node.label == "Simmer")
            .unwrap();
        let marinate = sched
            .nodes
            .iter()
            .find(|n| n.node.label == "Marinate")
            .unwrap();
        assert_eq!(simmer.scheduled_end, serve_time(19, 0));
        assert_eq!(simmer.scheduled_start, serve_time(18, 25));
        assert_eq!(marinate.scheduled_end, serve_time(18, 25));
        assert_eq!(marinate.scheduled_start, serve_time(17, 25));
    }

    #[test]
    fn burner_capacity_conflict_reported() {
        // Two burners, three dishes each needing the stove at the same time →
        // one must shift earlier and a burner-capacity conflict is reported.
        let mk = |slug: &str, title: &str| {
            build_timeline(&recipe(
                slug,
                title,
                vec![step(
                    0,
                    "Sear on the stove",
                    vec![timer(None, Some("20 minutes"))],
                )],
            ))
        };
        let meal = merge_timelines(&[mk("a", "A"), mk("b", "B"), mk("c", "C")]);
        // With only one cook, active sears also serialize; give enough cooks so
        // burners are the binding constraint under test.
        let kitchen = KitchenResources {
            burners: 2,
            cooks: 3,
            ..Default::default()
        };
        let sched = schedule_meal(&meal, serve_time(19, 0), kitchen);

        assert_eq!(sched.nodes.len(), 3);
        // At most 2 stove tasks overlap at any instant.
        let stove: Vec<_> = sched
            .nodes
            .iter()
            .filter(|n| n.node.resource.is_stove())
            .collect();
        for probe_min in 0..60 {
            let t = serve_time(18, 0) + Duration::minutes(probe_min);
            let concurrent = stove
                .iter()
                .filter(|n| n.scheduled_start <= t && t < n.scheduled_end)
                .count();
            assert!(concurrent <= 2, "more than 2 burners in use");
        }
        assert!(
            sched
                .conflicts
                .iter()
                .any(|c| c.kind == ConflictKind::BurnerCapacity)
        );
    }

    #[test]
    fn cook_attention_serializes_active_tasks() {
        // One cook, two hands-on active tasks → they cannot overlap.
        let mk = |slug: &str, title: &str| {
            build_timeline(&recipe(
                slug,
                title,
                vec![step(
                    0,
                    "Sear the meat",
                    vec![timer(None, Some("15 minutes"))],
                )],
            ))
        };
        let meal = merge_timelines(&[mk("a", "A"), mk("b", "B")]);
        let sched = schedule_meal(&meal, serve_time(19, 0), KitchenResources::default());

        assert_eq!(sched.nodes.len(), 2);
        // The two active tasks are serialized (no overlap).
        let a = &sched.nodes[0];
        let b = &sched.nodes[1];
        let overlap = a.scheduled_start < b.scheduled_end && b.scheduled_start < a.scheduled_end;
        assert!(!overlap, "active tasks overlapped with a single cook");
        assert!(
            sched
                .conflicts
                .iter()
                .any(|c| c.kind == ConflictKind::CookAttention)
        );
    }

    #[test]
    fn single_recipe_meal_matches_backward_schedule() {
        // A one-recipe meal should place nodes exactly like schedule_backward.
        let r = recipe(
            "adobo",
            "Adobo",
            vec![
                step(0, "Combine marinade", vec![]),
                step(1, "Marinate", vec![timer(Some("marinate"), Some("1 hour"))]),
                step(2, "Simmer", vec![timer(None, Some("35 minutes"))]),
            ],
        );
        let tl = build_timeline(&r);
        let single = crate::schedule::schedule_backward(&tl, serve_time(19, 0));
        let meal = merge_timelines(&[tl]);
        let sched = schedule_meal(&meal, serve_time(19, 0), KitchenResources::default());

        assert_eq!(sched.start_at, single.start_at);
        assert_eq!(sched.total_active_seconds, single.total_active_seconds);
        assert_eq!(sched.total_passive_seconds, single.total_passive_seconds);
    }
}
