use fond_domain::Recipe;

use crate::classify::classify_task_type;
use crate::duration::{extract_duration_from_text, parse_duration_str};
use crate::infer::infer_resource;
use crate::model::{DurationSource, NodeId, StepDuration, Timeline, TimelineNode};

/// Build an unscheduled timeline DAG from a recipe.
///
/// Each recipe step becomes one node. Dependencies default to
/// sequential order (each step depends on its predecessor).
/// Duration is extracted first from timer annotations, then
/// from heuristic text parsing. Steps with no determinable
/// duration remain untimed.
pub fn build_timeline(recipe: &Recipe) -> Timeline {
    let mut nodes = Vec::new();

    for step in &recipe.steps {
        let id = NodeId(nodes.len());

        // Extract duration: prefer timer annotations, then heuristic
        let duration = extract_step_duration(step);

        // Classify task type from timer names and body text
        let timer_names: Vec<Option<String>> = step.timers.iter().map(|t| t.name.clone()).collect();
        let task_type = classify_task_type(&step.body, &timer_names);

        // Infer the kitchen resource this step occupies (oven/stove/cook).
        let resource = infer_resource(&step.body, task_type);

        let label = build_label(step);

        // Conservative sequential dependencies
        let depends_on = if nodes.is_empty() {
            vec![]
        } else {
            vec![NodeId(nodes.len() - 1)]
        };

        nodes.push(TimelineNode {
            id,
            step_index: step.order,
            label,
            task_type,
            duration,
            resource,
            depends_on,
        });
    }

    // Propagate oven temperature forward within the recipe: a temperature
    // stated in a preheat step ("preheat to 325°F") carries to the later oven
    // steps (bake/roast) that actually occupy the oven but don't restate the
    // temperature, until a new temperature is set. Without this, the real
    // oven-occupying step carries no temperature and contention between dishes
    // at incompatible temperatures cannot be detected.
    let mut current_oven_temp: Option<crate::resource::OvenTemp> = None;
    for node in &mut nodes {
        if node.resource.is_oven() {
            match &node.resource.oven_temp {
                Some(t) => current_oven_temp = Some(t.clone()),
                None => node.resource.oven_temp = current_oven_temp.clone(),
            }
        }
    }

    Timeline {
        recipe_title: recipe.title.clone(),
        recipe_slug: recipe.slug.clone(),
        nodes,
    }
}

/// Extract duration from a step, trying timers first then heuristics.
fn extract_step_duration(step: &fond_domain::Step) -> Option<StepDuration> {
    // Try timer annotations first (most reliable)
    for timer in &step.timers {
        if let Some(ref dur_str) = timer.duration
            && let Some(seconds) = parse_duration_str(dur_str)
        {
            return Some(StepDuration {
                seconds,
                source: DurationSource::Timer,
                original: dur_str.clone(),
            });
        }
    }

    // Fall back to heuristic text extraction
    if let Some((seconds, matched)) = extract_duration_from_text(&step.body) {
        return Some(StepDuration {
            seconds,
            source: DurationSource::Heuristic,
            original: matched,
        });
    }

    None
}

/// Build a human-readable label for a timeline node.
///
/// Uses the first named timer if available, otherwise truncates
/// the step body to a reasonable length.
fn build_label(step: &fond_domain::Step) -> String {
    // Prefer named timer (e.g., "marinate", "cold proof")
    for timer in &step.timers {
        if let Some(ref name) = timer.name
            && !name.is_empty()
        {
            let mut chars = name.chars();
            let label = match chars.next() {
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                None => name.clone(),
            };
            return label;
        }
    }

    // Truncate step body
    let body = step.body.trim();
    if body.len() <= 60 {
        body.to_string()
    } else {
        // Find word boundary near 60 chars
        let truncated = &body[..60];
        match truncated.rfind(' ') {
            Some(pos) => format!("{}…", &body[..pos]),
            None => format!("{truncated}…"),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use fond_domain::{Recipe, Step, Timer};

    use super::*;

    fn make_recipe(steps: Vec<Step>) -> Recipe {
        Recipe {
            slug: "test-recipe".into(),
            title: "Test Recipe".into(),
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
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

    #[test]
    fn empty_recipe_produces_empty_timeline() {
        let recipe = make_recipe(vec![]);
        let tl = build_timeline(&recipe);
        assert!(tl.nodes.is_empty());
        assert_eq!(tl.recipe_slug, "test-recipe");
    }

    #[test]
    fn single_timed_step() {
        let recipe = make_recipe(vec![step(
            0,
            "Simmer for 35 minutes until done",
            vec![timer(None, Some("35 minutes"))],
        )]);
        let tl = build_timeline(&recipe);
        assert_eq!(tl.nodes.len(), 1);
        let node = &tl.nodes[0];
        assert_eq!(node.duration.as_ref().unwrap().seconds, 2100);
        assert!(node.depends_on.is_empty());
    }

    #[test]
    fn sequential_dependencies() {
        let recipe = make_recipe(vec![
            step(0, "Chop onions", vec![]),
            step(
                1,
                "Sauté for 5 minutes",
                vec![timer(None, Some("5 minutes"))],
            ),
            step(2, "Serve", vec![]),
        ]);
        let tl = build_timeline(&recipe);
        assert_eq!(tl.nodes.len(), 3);
        assert!(tl.nodes[0].depends_on.is_empty());
        assert_eq!(tl.nodes[1].depends_on, vec![NodeId(0)]);
        assert_eq!(tl.nodes[2].depends_on, vec![NodeId(1)]);
    }

    #[test]
    fn oven_temp_propagates_from_preheat_to_later_oven_steps() {
        let recipe = make_recipe(vec![
            step(0, "Preheat oven to 325F", vec![]),
            step(
                1,
                "Roast turkey for 180 minutes",
                vec![timer(None, Some("180 minutes"))],
            ),
            step(2, "Rest turkey for 10 minutes", vec![]),
        ]);
        let tl = build_timeline(&recipe);
        // Preheat states the temperature.
        assert!(tl.nodes[0].resource.is_oven());
        assert_eq!(
            tl.nodes[0].resource.oven_temp.as_ref().unwrap().fahrenheit,
            325
        );
        // The roast step occupies the oven but doesn't restate the temperature;
        // it must inherit 325°F so contention can be detected.
        assert!(tl.nodes[1].resource.is_oven());
        assert_eq!(
            tl.nodes[1].resource.oven_temp.as_ref().unwrap().fahrenheit,
            325
        );
        // The rest step is not an oven step and stays untouched.
        assert!(!tl.nodes[2].resource.is_oven());
    }

    #[test]
    fn named_timer_used_as_label() {
        let recipe = make_recipe(vec![step(
            0,
            "Add chicken to marinade and refrigerate",
            vec![timer(Some("marinate"), Some("1 hour"))],
        )]);
        let tl = build_timeline(&recipe);
        assert_eq!(tl.nodes[0].label, "Marinate");
    }

    #[test]
    fn untimed_step_has_no_duration() {
        let recipe = make_recipe(vec![step(0, "Chop the vegetables finely", vec![])]);
        let tl = build_timeline(&recipe);
        assert!(tl.nodes[0].duration.is_none());
    }

    #[test]
    fn heuristic_duration_from_body() {
        let recipe = make_recipe(vec![step(
            0,
            "Let stand for about 10 minutes before serving",
            vec![],
        )]);
        let tl = build_timeline(&recipe);
        let dur = tl.nodes[0].duration.as_ref().unwrap();
        assert_eq!(dur.seconds, 600);
        assert_eq!(dur.source, DurationSource::Heuristic);
    }

    #[test]
    fn timer_duration_preferred_over_heuristic() {
        let recipe = make_recipe(vec![step(
            0,
            "Cook for about 20 minutes then rest for 5 minutes",
            vec![timer(None, Some("20 minutes"))],
        )]);
        let tl = build_timeline(&recipe);
        let dur = tl.nodes[0].duration.as_ref().unwrap();
        assert_eq!(dur.seconds, 1200);
        assert_eq!(dur.source, DurationSource::Timer);
    }

    #[test]
    fn range_duration_takes_max() {
        let recipe = make_recipe(vec![step(
            0,
            "Bake until golden",
            vec![timer(None, Some("40-45 minutes"))],
        )]);
        let tl = build_timeline(&recipe);
        assert_eq!(tl.nodes[0].duration.as_ref().unwrap().seconds, 2700);
    }

    #[test]
    fn long_body_truncated_in_label() {
        let recipe = make_recipe(vec![step(
            0,
            "Slowly pour the hot cream into the egg mixture while whisking constantly to temper the eggs and prevent scrambling",
            vec![],
        )]);
        let tl = build_timeline(&recipe);
        assert!(tl.nodes[0].label.len() <= 65); // 60 + ellipsis
        assert!(tl.nodes[0].label.ends_with('…'));
    }

    #[test]
    fn chicken_adobo_timeline() {
        let recipe = make_recipe(vec![
            step(
                0,
                "Combine soy sauce, white vinegar, garlic, black peppercorns, and bay leaves in a large bowl.",
                vec![],
            ),
            step(
                1,
                "Add chicken thighs to the marinade. Cover and refrigerate for at least 1 hour or up to overnight.",
                vec![timer(Some("marinate"), Some("1 hour"))],
            ),
            step(
                2,
                "Transfer everything to a dutch oven and bring to a boil over high heat.",
                vec![],
            ),
            step(
                3,
                "Reduce heat to medium-low, cover, and simmer for 35 minutes until chicken is cooked through.",
                vec![timer(None, Some("35 minutes"))],
            ),
            step(
                4,
                "Remove the chicken and increase heat to medium-high. Reduce the sauce for 10 minutes until slightly thickened.",
                vec![timer(None, Some("10 minutes"))],
            ),
            step(
                5,
                "Return chicken to the pot and coat with the reduced sauce. Serve over steamed rice.",
                vec![],
            ),
        ]);

        let tl = build_timeline(&recipe);
        assert_eq!(tl.nodes.len(), 6);

        // Step 1: marinate → passive prep, 1 hour
        assert_eq!(tl.nodes[1].task_type, crate::model::TaskType::PassivePrep);
        assert_eq!(tl.nodes[1].duration.as_ref().unwrap().seconds, 3600);

        // Step 3: simmer → passive cook, 35 min
        assert_eq!(tl.nodes[3].task_type, crate::model::TaskType::PassiveCook);
        assert_eq!(tl.nodes[3].duration.as_ref().unwrap().seconds, 2100);

        // Step 4: reduce → active cook, 10 min
        assert_eq!(tl.nodes[4].task_type, crate::model::TaskType::ActiveCook);
        assert_eq!(tl.nodes[4].duration.as_ref().unwrap().seconds, 600);

        // All sequential
        for i in 1..6 {
            assert_eq!(tl.nodes[i].depends_on, vec![NodeId(i - 1)]);
        }
    }
}
