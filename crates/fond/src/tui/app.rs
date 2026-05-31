//! TUI cook mode application state and timer logic.

use std::time::{Duration, Instant};

use fond_domain::Recipe;
use fond_timeline::ScheduledTimeline;

/// Result of a cook session, used for cook-log persistence.
#[allow(dead_code)]
pub struct CookResult {
    pub recipe_title: String,
    pub recipe_slug: String,
    pub steps_completed: usize,
    pub total_steps: usize,
    pub cook_duration: Duration,
    pub completed: bool,
}

/// A running countdown timer.
pub struct RunningTimer {
    pub step_index: usize,
    pub label: String,
    pub total_seconds: u64,
    started_at: Instant,
    paused_duration: Duration,
    paused_at: Option<Instant>,
    pub alert_fired: bool,
}

impl RunningTimer {
    fn new(step_index: usize, label: String, total_seconds: u64) -> Self {
        Self {
            step_index,
            label,
            total_seconds,
            started_at: Instant::now(),
            paused_duration: Duration::ZERO,
            paused_at: None,
            alert_fired: false,
        }
    }

    /// Seconds remaining on this timer.
    pub fn remaining_secs(&self) -> u64 {
        let elapsed = self.running_elapsed();
        let total = Duration::from_secs(self.total_seconds);
        total.saturating_sub(elapsed).as_secs()
    }

    /// Fraction completed (0.0 to 1.0).
    pub fn progress(&self) -> f64 {
        if self.total_seconds == 0 {
            return 1.0;
        }
        let elapsed = self.running_elapsed().as_secs_f64();
        let total = self.total_seconds as f64;
        (elapsed / total).min(1.0)
    }

    pub fn is_finished(&self) -> bool {
        self.remaining_secs() == 0
    }

    pub fn is_paused(&self) -> bool {
        self.paused_at.is_some()
    }

    fn toggle_pause(&mut self) {
        if let Some(paused_at) = self.paused_at.take() {
            self.paused_duration += paused_at.elapsed();
        } else {
            self.paused_at = Some(Instant::now());
        }
    }

    /// Wall-clock time actually spent running (excludes paused intervals).
    fn running_elapsed(&self) -> Duration {
        let total_elapsed = if let Some(paused_at) = self.paused_at {
            paused_at.duration_since(self.started_at)
        } else {
            self.started_at.elapsed()
        };
        total_elapsed.saturating_sub(self.paused_duration)
    }
}

/// Main application state for the TUI cook mode.
pub struct CookApp {
    pub recipe: Recipe,
    pub schedule: Option<ScheduledTimeline>,
    pub current_step: usize,
    pub total_steps: usize,
    pub completed_steps: Vec<bool>,
    pub running_timers: Vec<RunningTimer>,
    pub quit_confirm: bool,
    pub bell_pending: bool,
    cook_start: Instant,
}

impl CookApp {
    pub fn new(recipe: Recipe, schedule: Option<ScheduledTimeline>) -> Self {
        let total_steps = recipe.steps.len();
        Self {
            recipe,
            schedule,
            current_step: 0,
            total_steps,
            completed_steps: vec![false; total_steps],
            running_timers: Vec::new(),
            quit_confirm: false,
            bell_pending: false,
            cook_start: Instant::now(),
        }
    }

    /// Advance to the next step, marking the current one as completed.
    pub fn next_step(&mut self) {
        if self.current_step < self.total_steps.saturating_sub(1) {
            self.completed_steps[self.current_step] = true;
            self.current_step += 1;
        }
    }

    /// Go back to the previous step.
    pub fn prev_step(&mut self) {
        if self.current_step > 0 {
            self.current_step -= 1;
        }
    }

    /// Jump to a specific step index.
    pub fn jump_to_step(&mut self, index: usize) {
        if index < self.total_steps {
            self.current_step = index;
        }
    }

    /// Toggle timer for the current step.
    ///
    /// If a timer is running for this step, pause/resume it.
    /// If no timer is running and the step has a timed node, start one.
    pub fn toggle_timer(&mut self) {
        let step_index = self.current_step;

        // Check if there's already a timer for this step
        if let Some(timer) = self
            .running_timers
            .iter_mut()
            .find(|t| t.step_index == step_index)
        {
            timer.toggle_pause();
            return;
        }

        // Start a new timer if the step has timer data
        if let Some(duration_secs) = self.step_timer_seconds(step_index) {
            let label = self.step_timer_label(step_index);
            self.running_timers
                .push(RunningTimer::new(step_index, label, duration_secs));
        }
    }

    /// Get the timer duration in seconds for a step (from timeline nodes).
    fn step_timer_seconds(&self, step_index: usize) -> Option<u64> {
        let step = self.recipe.steps.get(step_index)?;
        // Check timeline node first
        if let Some(ref sched) = self.schedule {
            for node in &sched.nodes {
                if node.node.step_index == step.order
                    && let Some(ref dur) = node.node.duration
                {
                    return Some(dur.seconds);
                }
            }
        }
        // Fall back to parsing timer from step directly
        for timer in &step.timers {
            if let Some(ref dur_str) = timer.duration
                && let Some(secs) = fond_timeline::duration::parse_duration_str(dur_str)
            {
                return Some(secs);
            }
        }
        None
    }

    /// Get a label for the step's timer.
    fn step_timer_label(&self, step_index: usize) -> String {
        if let Some(step) = self.recipe.steps.get(step_index) {
            for timer in &step.timers {
                if let Some(ref name) = timer.name
                    && !name.is_empty()
                {
                    return name.clone();
                }
            }
        }
        format!("Step {}", step_index + 1)
    }

    /// Check timers for completion and fire alerts.
    pub fn tick(&mut self) {
        for timer in &mut self.running_timers {
            if timer.is_finished() && !timer.alert_fired {
                timer.alert_fired = true;
                self.bell_pending = true;
            }
        }

        // Fire terminal bell
        if self.bell_pending {
            print!("\x07"); // BEL character
            self.bell_pending = false;
        }
    }

    /// Whether the current step has a timer that can be started.
    pub fn current_step_has_timer(&self) -> bool {
        self.step_timer_seconds(self.current_step).is_some()
    }

    /// Get the running timer for the current step, if any.
    pub fn current_step_timer(&self) -> Option<&RunningTimer> {
        self.running_timers
            .iter()
            .find(|t| t.step_index == self.current_step)
    }

    /// Number of steps marked as completed.
    pub fn steps_completed_count(&self) -> usize {
        self.completed_steps.iter().filter(|&&c| c).count()
    }

    /// Total wall-clock elapsed time.
    pub fn elapsed(&self) -> Duration {
        self.cook_start.elapsed()
    }

    /// Build the cook result for persistence.
    pub fn result(&self) -> CookResult {
        let completed = self.completed_steps.iter().all(|&c| c);
        CookResult {
            recipe_title: self.recipe.title.clone(),
            recipe_slug: self.recipe.slug.clone(),
            steps_completed: self.steps_completed_count(),
            total_steps: self.total_steps,
            cook_duration: self.elapsed(),
            completed,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use fond_domain::{Recipe, Step, Timer};

    use super::*;

    fn test_recipe() -> Recipe {
        Recipe {
            slug: "test".into(),
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
            steps: vec![
                Step {
                    section: None,
                    body: "Chop onions".into(),
                    timers: vec![],
                    order: 0,
                },
                Step {
                    section: None,
                    body: "Simmer for 10 minutes".into(),
                    timers: vec![Timer {
                        name: None,
                        duration: Some("10 minutes".into()),
                    }],
                    order: 1,
                },
                Step {
                    section: None,
                    body: "Serve".into(),
                    timers: vec![],
                    order: 2,
                },
            ],
            cookware: vec![],
            tags: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            raw_source: None,
        }
    }

    #[test]
    fn initial_state() {
        let app = CookApp::new(test_recipe(), None);
        assert_eq!(app.current_step, 0);
        assert_eq!(app.total_steps, 3);
        assert_eq!(app.steps_completed_count(), 0);
        assert!(!app.quit_confirm);
    }

    #[test]
    fn navigate_forward_and_back() {
        let mut app = CookApp::new(test_recipe(), None);
        app.next_step();
        assert_eq!(app.current_step, 1);
        assert!(app.completed_steps[0]);

        app.next_step();
        assert_eq!(app.current_step, 2);
        assert!(app.completed_steps[1]);

        // Can't go past last step
        app.next_step();
        assert_eq!(app.current_step, 2);

        app.prev_step();
        assert_eq!(app.current_step, 1);

        app.prev_step();
        assert_eq!(app.current_step, 0);

        // Can't go before first step
        app.prev_step();
        assert_eq!(app.current_step, 0);
    }

    #[test]
    fn jump_to_step() {
        let mut app = CookApp::new(test_recipe(), None);
        app.jump_to_step(2);
        assert_eq!(app.current_step, 2);

        // Out of range does nothing
        app.jump_to_step(99);
        assert_eq!(app.current_step, 2);
    }

    #[test]
    fn timer_start_and_check() {
        let mut app = CookApp::new(test_recipe(), None);

        // Step 0 has no timer
        assert!(!app.current_step_has_timer());
        app.toggle_timer(); // No-op
        assert!(app.running_timers.is_empty());

        // Step 1 has a timer
        app.next_step();
        assert!(app.current_step_has_timer());
        app.toggle_timer();
        assert_eq!(app.running_timers.len(), 1);
        assert_eq!(app.running_timers[0].total_seconds, 600);
    }

    #[test]
    fn timer_pause_resume() {
        let mut app = CookApp::new(test_recipe(), None);
        app.next_step();
        app.toggle_timer(); // Start
        assert!(!app.running_timers[0].is_paused());

        app.toggle_timer(); // Pause
        assert!(app.running_timers[0].is_paused());

        app.toggle_timer(); // Resume
        assert!(!app.running_timers[0].is_paused());
    }

    #[test]
    fn result_tracks_completion() {
        let mut app = CookApp::new(test_recipe(), None);
        app.next_step(); // Complete step 0
        app.next_step(); // Complete step 1

        let result = app.result();
        assert_eq!(result.steps_completed, 2);
        assert_eq!(result.total_steps, 3);
        assert!(!result.completed);
    }
}
