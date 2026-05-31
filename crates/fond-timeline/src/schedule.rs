use chrono::NaiveDateTime;

use crate::model::{ScheduledNode, ScheduledTimeline, Timeline, TimelineNode};

/// Schedule a timeline backward from a target serve time.
///
/// Given a DAG of cooking steps and a desired finish time, computes
/// the latest allowable start time for each step via reverse
/// topological traversal. Untimed steps are positioned at their
/// successor's start (zero-duration anchoring) but their duration
/// remains `None` to indicate the time is unknown.
pub fn schedule_backward(timeline: &Timeline, serve_at: NaiveDateTime) -> ScheduledTimeline {
    let n = timeline.nodes.len();

    if n == 0 {
        return ScheduledTimeline {
            recipe_title: timeline.recipe_title.clone(),
            recipe_slug: timeline.recipe_slug.clone(),
            serve_at,
            start_at: serve_at,
            total_active_seconds: 0,
            total_passive_seconds: 0,
            nodes: vec![],
            has_untimed_steps: false,
        };
    }

    // Build successor map for backward pass
    let mut successors: Vec<Vec<usize>> = vec![vec![]; n];
    for (i, node) in timeline.nodes.iter().enumerate() {
        for dep in &node.depends_on {
            successors[dep.0].push(i);
        }
    }

    let topo_order = topological_sort(&timeline.nodes);

    // Initialize: terminal nodes finish at serve_at
    let mut latest_end: Vec<NaiveDateTime> = vec![serve_at; n];
    let mut latest_start: Vec<NaiveDateTime> = vec![serve_at; n];

    // Backward pass: process in reverse topological order
    for &idx in topo_order.iter().rev() {
        // latest_end = min of all successors' latest_start
        if !successors[idx].is_empty() {
            latest_end[idx] = successors[idx]
                .iter()
                .map(|&s| latest_start[s])
                .min()
                .unwrap();
        }

        // latest_start = latest_end - duration (or same if untimed)
        if let Some(ref dur) = timeline.nodes[idx].duration {
            latest_start[idx] = latest_end[idx] - chrono::Duration::seconds(dur.seconds as i64);
        } else {
            latest_start[idx] = latest_end[idx];
        }
    }

    // Build scheduled nodes and compute totals
    let mut has_untimed = false;
    let mut total_active = 0u64;
    let mut total_passive = 0u64;

    let scheduled_nodes: Vec<ScheduledNode> = topo_order
        .iter()
        .map(|&i| {
            let node = &timeline.nodes[i];

            if node.duration.is_none() {
                has_untimed = true;
            }

            if let Some(ref dur) = node.duration {
                if node.task_type.is_active() {
                    total_active += dur.seconds;
                } else {
                    total_passive += dur.seconds;
                }
            }

            ScheduledNode {
                node: node.clone(),
                scheduled_start: latest_start[i],
                scheduled_end: latest_end[i],
            }
        })
        .collect();

    let earliest_start = topo_order
        .iter()
        .map(|&i| latest_start[i])
        .min()
        .unwrap_or(serve_at);

    ScheduledTimeline {
        recipe_title: timeline.recipe_title.clone(),
        recipe_slug: timeline.recipe_slug.clone(),
        serve_at,
        start_at: earliest_start,
        total_active_seconds: total_active,
        total_passive_seconds: total_passive,
        nodes: scheduled_nodes,
        has_untimed_steps: has_untimed,
    }
}

/// Kahn's algorithm for topological sorting.
fn topological_sort(nodes: &[TimelineNode]) -> Vec<usize> {
    let n = nodes.len();
    let mut in_degree = vec![0u32; n];
    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];

    for (i, node) in nodes.iter().enumerate() {
        for dep in &node.depends_on {
            adj[dep.0].push(i);
            in_degree[i] += 1;
        }
    }

    let mut queue: std::collections::VecDeque<usize> =
        (0..n).filter(|&i| in_degree[i] == 0).collect();
    let mut order = Vec::with_capacity(n);

    while let Some(idx) = queue.pop_front() {
        order.push(idx);
        for &next in &adj[idx] {
            in_degree[next] -= 1;
            if in_degree[next] == 0 {
                queue.push_back(next);
            }
        }
    }

    order
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, NaiveTime};
    use fond_domain::{Recipe, Step, Timer};

    use super::*;
    use crate::build::build_timeline;
    use crate::duration::format_duration;

    fn make_recipe(steps: Vec<Step>) -> Recipe {
        Recipe {
            slug: "test".into(),
            title: "Test".into(),
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
    fn empty_timeline_returns_serve_time() {
        let recipe = make_recipe(vec![]);
        let tl = build_timeline(&recipe);
        let sched = schedule_backward(&tl, serve_time(19, 0));
        assert_eq!(sched.start_at, serve_time(19, 0));
        assert_eq!(sched.serve_at, serve_time(19, 0));
        assert!(sched.nodes.is_empty());
    }

    #[test]
    fn single_timed_step() {
        let recipe = make_recipe(vec![step(0, "Bake", vec![timer(None, Some("30 minutes"))])]);
        let tl = build_timeline(&recipe);
        let sched = schedule_backward(&tl, serve_time(19, 0));

        assert_eq!(sched.start_at, serve_time(18, 30));
        assert_eq!(sched.nodes.len(), 1);
        assert_eq!(sched.nodes[0].scheduled_start, serve_time(18, 30));
        assert_eq!(sched.nodes[0].scheduled_end, serve_time(19, 0));
    }

    #[test]
    fn untimed_step_anchored_at_successor() {
        let recipe = make_recipe(vec![
            step(0, "Chop onions", vec![]),
            step(
                1,
                "Sauté for 10 minutes",
                vec![timer(None, Some("10 minutes"))],
            ),
        ]);
        let tl = build_timeline(&recipe);
        let sched = schedule_backward(&tl, serve_time(19, 0));

        // Untimed step: positioned right before the 10-min sauté
        assert_eq!(sched.nodes[0].scheduled_start, serve_time(18, 50));
        assert_eq!(sched.nodes[0].scheduled_end, serve_time(18, 50));
        // Timed step: 18:50 → 19:00
        assert_eq!(sched.nodes[1].scheduled_start, serve_time(18, 50));
        assert_eq!(sched.nodes[1].scheduled_end, serve_time(19, 0));
        assert_eq!(sched.start_at, serve_time(18, 50));
    }

    #[test]
    fn chicken_adobo_schedule() {
        let recipe = make_recipe(vec![
            step(0, "Combine soy sauce, vinegar, garlic in a bowl", vec![]),
            step(
                1,
                "Add chicken, cover and refrigerate",
                vec![timer(Some("marinate"), Some("1 hour"))],
            ),
            step(2, "Transfer to dutch oven and bring to a boil", vec![]),
            step(
                3,
                "Reduce heat, cover, and simmer until cooked through",
                vec![timer(None, Some("35 minutes"))],
            ),
            step(
                4,
                "Remove chicken, reduce the sauce",
                vec![timer(None, Some("10 minutes"))],
            ),
            step(5, "Return chicken, coat, serve", vec![]),
        ]);

        let tl = build_timeline(&recipe);
        let sched = schedule_backward(&tl, serve_time(19, 0));

        // Work backward from 19:00:
        // Step 5 (untimed): 19:00
        // Step 4 (10 min):  18:50 → 19:00
        // Step 3 (35 min):  18:15 → 18:50
        // Step 2 (untimed): 18:15
        // Step 1 (1 hr):    17:15 → 18:15
        // Step 0 (untimed): 17:15
        assert_eq!(sched.start_at, serve_time(17, 15));
        assert_eq!(sched.nodes[0].scheduled_start, serve_time(17, 15));
        assert_eq!(sched.nodes[1].scheduled_start, serve_time(17, 15));
        assert_eq!(sched.nodes[1].scheduled_end, serve_time(18, 15));
        assert_eq!(sched.nodes[2].scheduled_start, serve_time(18, 15));
        assert_eq!(sched.nodes[3].scheduled_start, serve_time(18, 15));
        assert_eq!(sched.nodes[3].scheduled_end, serve_time(18, 50));
        assert_eq!(sched.nodes[4].scheduled_start, serve_time(18, 50));
        assert_eq!(sched.nodes[4].scheduled_end, serve_time(19, 0));
        assert_eq!(sched.nodes[5].scheduled_start, serve_time(19, 0));

        // Time totals
        assert_eq!(sched.total_active_seconds, 600); // 10 min reduce
        assert_eq!(sched.total_passive_seconds, 5700); // 60 min marinate + 35 min simmer
        assert!(sched.has_untimed_steps);

        assert_eq!(format_duration(sched.total_active_seconds), "10 min");
        assert_eq!(format_duration(sched.total_passive_seconds), "1 hr 35 min");
    }

    #[test]
    fn all_untimed_starts_at_serve_time() {
        let recipe = make_recipe(vec![
            step(0, "Step A", vec![]),
            step(1, "Step B", vec![]),
            step(2, "Step C", vec![]),
        ]);
        let tl = build_timeline(&recipe);
        let sched = schedule_backward(&tl, serve_time(19, 0));

        assert_eq!(sched.start_at, serve_time(19, 0));
        for node in &sched.nodes {
            assert_eq!(node.scheduled_start, serve_time(19, 0));
        }
        assert!(sched.has_untimed_steps);
    }
}
