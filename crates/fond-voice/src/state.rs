//! The voice cook-mode "brain".
//!
//! [`VoiceCookState`] holds the live cooking session — current step, running
//! timers — and turns a parsed [`VoiceCommand`] into a spoken [`VoiceResponse`].
//! It is deliberately decoupled from any speech backend or UI: the same brain
//! drives the CLI REPL today and could drive a native app tomorrow. All the
//! phrasing the user hears lives here, so it is unit-testable end to end.

use std::time::{Duration, Instant};

use fond_domain::{Recipe, RecipeIngredient};
use fond_timeline::ScheduledTimeline;
use fond_timeline::duration::{format_duration, parse_duration_str};

use crate::command::VoiceCommand;

/// A spoken reply plus any control-flow effect the caller must honor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceResponse {
    /// Text to speak aloud (and print as a graceful fallback).
    pub speech: String,
    /// When true, the caller should exit cook mode.
    pub should_quit: bool,
}

impl VoiceResponse {
    fn say(speech: impl Into<String>) -> Self {
        Self {
            speech: speech.into(),
            should_quit: false,
        }
    }

    fn quit(speech: impl Into<String>) -> Self {
        Self {
            speech: speech.into(),
            should_quit: true,
        }
    }
}

/// A running countdown timer within a voice cook session.
struct VoiceTimer {
    label: String,
    total_seconds: u64,
    started_at: Instant,
    paused_duration: Duration,
    paused_at: Option<Instant>,
    alert_fired: bool,
}

impl VoiceTimer {
    fn new(label: String, total_seconds: u64) -> Self {
        Self {
            label,
            total_seconds,
            started_at: Instant::now(),
            paused_duration: Duration::ZERO,
            paused_at: None,
            alert_fired: false,
        }
    }

    fn running_elapsed(&self) -> Duration {
        let total = if let Some(paused_at) = self.paused_at {
            paused_at.duration_since(self.started_at)
        } else {
            self.started_at.elapsed()
        };
        total.saturating_sub(self.paused_duration)
    }

    fn remaining_secs(&self) -> u64 {
        Duration::from_secs(self.total_seconds)
            .saturating_sub(self.running_elapsed())
            .as_secs()
    }

    fn is_finished(&self) -> bool {
        self.remaining_secs() == 0
    }

    fn pause(&mut self) {
        if self.paused_at.is_none() {
            self.paused_at = Some(Instant::now());
        }
    }

    fn resume(&mut self) {
        if let Some(paused_at) = self.paused_at.take() {
            self.paused_duration += paused_at.elapsed();
        }
    }
}

/// Live state for a hands-free voice cook session.
pub struct VoiceCookState {
    recipe: Recipe,
    schedule: Option<ScheduledTimeline>,
    current: usize,
    total: usize,
    completed: Vec<bool>,
    timers: Vec<VoiceTimer>,
}

impl VoiceCookState {
    /// Create a new session for `recipe`, optionally with a backward schedule.
    pub fn new(recipe: Recipe, schedule: Option<ScheduledTimeline>) -> Self {
        let total = recipe.steps.len();
        Self {
            recipe,
            schedule,
            current: 0,
            total,
            completed: vec![false; total],
            timers: Vec::new(),
        }
    }

    /// Recipe title, for the intro line.
    pub fn recipe_title(&self) -> &str {
        &self.recipe.title
    }

    /// Number of steps completed (for a closing summary / cook log).
    pub fn steps_completed(&self) -> usize {
        self.completed.iter().filter(|&&c| c).count()
    }

    /// Total steps in the recipe.
    pub fn total_steps(&self) -> usize {
        self.total
    }

    /// A spoken introduction to read when the session starts.
    pub fn intro(&self) -> String {
        if self.total == 0 {
            return format!(
                "{} has no steps to cook. Say 'quit' to exit.",
                self.recipe.title
            );
        }
        format!(
            "Starting voice cook mode for {}. {}. Say 'help' any time. {}",
            self.recipe.title,
            pluralize(self.total, "step"),
            self.announce_current()
        )
    }

    /// Announce the current step (number, body, and a timer hint if present).
    pub fn announce_current(&self) -> String {
        let Some(step) = self.recipe.steps.get(self.current) else {
            return "There are no steps in this recipe.".to_string();
        };
        let mut out = format!(
            "Step {} of {}. {}",
            self.current + 1,
            self.total,
            step.body.trim()
        );
        if let Some(secs) = self.step_timer_seconds(self.current) {
            out.push_str(&format!(
                " This step has a timer for {}. Say 'start timer' to begin it.",
                format_duration(secs)
            ));
        }
        out
    }

    /// Apply a recognized command, returning what to say.
    pub fn apply(&mut self, cmd: VoiceCommand) -> VoiceResponse {
        match cmd {
            VoiceCommand::Next => self.next_step(),
            VoiceCommand::Previous => self.prev_step(),
            VoiceCommand::Repeat => VoiceResponse::say(self.announce_current()),
            VoiceCommand::WhatsNext => VoiceResponse::say(self.whats_next()),
            VoiceCommand::GoToStep(n) => self.goto_step(n),
            VoiceCommand::HowMuch(ingredient) => VoiceResponse::say(self.how_much(&ingredient)),
            VoiceCommand::ListIngredients => VoiceResponse::say(self.list_ingredients()),
            VoiceCommand::StartTimer => self.start_current_timer(),
            VoiceCommand::SetTimer { seconds, label } => self.set_timer(seconds, label),
            VoiceCommand::StopTimer => self.stop_timers(),
            VoiceCommand::PauseTimer => self.pause_timers(),
            VoiceCommand::ResumeTimer => self.resume_timers(),
            VoiceCommand::TimerStatus => VoiceResponse::say(self.timer_status()),
            VoiceCommand::Help => VoiceResponse::say(Self::help_text()),
            VoiceCommand::Quit => VoiceResponse::quit("Ending cook mode. Enjoy your meal!"),
        }
    }

    /// Advance running timers; return an announcement for each newly-fired one.
    pub fn tick(&mut self) -> Vec<String> {
        let mut fired = Vec::new();
        for timer in &mut self.timers {
            if timer.is_finished() && !timer.alert_fired {
                timer.alert_fired = true;
                fired.push(format!("Time's up for {}!", timer.label));
            }
        }
        fired
    }

    // --- Navigation ---

    fn next_step(&mut self) -> VoiceResponse {
        if self.total == 0 {
            return VoiceResponse::say("There are no steps to advance through.");
        }
        if self.current < self.total - 1 {
            self.completed[self.current] = true;
            self.current += 1;
            VoiceResponse::say(self.announce_current())
        } else {
            self.completed[self.current] = true;
            VoiceResponse::say(
                "That was the last step. The dish is done — say 'quit' when you're ready.",
            )
        }
    }

    fn prev_step(&mut self) -> VoiceResponse {
        if self.current == 0 {
            return VoiceResponse::say(format!(
                "You're already on the first step. {}",
                self.announce_current()
            ));
        }
        self.current -= 1;
        VoiceResponse::say(self.announce_current())
    }

    fn goto_step(&mut self, n: usize) -> VoiceResponse {
        if n == 0 || n > self.total {
            return VoiceResponse::say(format!(
                "There's no step {n}. This recipe has {}.",
                pluralize(self.total, "step")
            ));
        }
        self.current = n - 1;
        VoiceResponse::say(self.announce_current())
    }

    fn whats_next(&self) -> String {
        if self.total == 0 {
            return "There are no steps in this recipe.".to_string();
        }
        if self.current + 1 < self.total
            && let Some(step) = self.recipe.steps.get(self.current + 1)
        {
            return format!("Next up, step {}: {}", self.current + 2, step.body.trim());
        }
        "You're on the last step — there's nothing after this.".to_string()
    }

    // --- Ingredients ---

    fn how_much(&self, query: &str) -> String {
        let q = query.trim().to_lowercase();
        let q_singular = q.strip_suffix('s').unwrap_or(&q);
        let matched = self.recipe.ingredients.iter().find(|ing| {
            let name = ing.name.to_lowercase();
            let name_singular = name.strip_suffix('s').unwrap_or(&name);
            name == q
                || name.contains(&q)
                || q.contains(&name)
                || name_singular == q_singular
                || name.contains(q_singular)
        });

        match matched {
            Some(ing) => match format_amount(ing) {
                Some(amount) => format!("You need {amount} {}.", ing.name),
                None => format!(
                    "{} is in the recipe, but no amount is specified.",
                    capitalize(&ing.name)
                ),
            },
            None => format!("I don't see {query} in the ingredient list."),
        }
    }

    fn list_ingredients(&self) -> String {
        if self.recipe.ingredients.is_empty() {
            return "This recipe doesn't list any ingredients.".to_string();
        }
        let items: Vec<String> = self
            .recipe
            .ingredients
            .iter()
            .map(|ing| match format_amount(ing) {
                Some(amount) => format!("{amount} {}", ing.name),
                None => ing.name.clone(),
            })
            .collect();
        format!("This recipe needs: {}.", items.join(", "))
    }

    // --- Timers ---

    fn start_current_timer(&mut self) -> VoiceResponse {
        match self.step_timer_seconds(self.current) {
            Some(secs) => {
                let label = self.step_timer_label(self.current);
                self.timers.push(VoiceTimer::new(label.clone(), secs));
                VoiceResponse::say(format!(
                    "Timer started for {}: {}.",
                    label,
                    format_duration(secs)
                ))
            }
            None => VoiceResponse::say(
                "This step doesn't have a timer. Say something like 'set a timer for 5 minutes'.",
            ),
        }
    }

    fn set_timer(&mut self, seconds: u64, label: Option<String>) -> VoiceResponse {
        let label = label.unwrap_or_else(|| format!("step {}", self.current + 1));
        self.timers.push(VoiceTimer::new(label.clone(), seconds));
        VoiceResponse::say(format!(
            "Timer set for {} for {}.",
            format_duration(seconds),
            label
        ))
    }

    fn stop_timers(&mut self) -> VoiceResponse {
        if self.timers.is_empty() {
            return VoiceResponse::say("There are no timers running.");
        }
        let count = self.timers.len();
        self.timers.clear();
        VoiceResponse::say(if count == 1 {
            "Timer stopped.".to_string()
        } else {
            format!("Stopped {count} timers.")
        })
    }

    fn pause_timers(&mut self) -> VoiceResponse {
        let active: Vec<&mut VoiceTimer> = self
            .timers
            .iter_mut()
            .filter(|t| t.paused_at.is_none() && !t.alert_fired)
            .collect();
        if active.is_empty() {
            return VoiceResponse::say("There are no running timers to pause.");
        }
        for timer in active {
            timer.pause();
        }
        VoiceResponse::say("Timers paused.")
    }

    fn resume_timers(&mut self) -> VoiceResponse {
        let paused: Vec<&mut VoiceTimer> = self
            .timers
            .iter_mut()
            .filter(|t| t.paused_at.is_some())
            .collect();
        if paused.is_empty() {
            return VoiceResponse::say("There are no paused timers.");
        }
        for timer in paused {
            timer.resume();
        }
        VoiceResponse::say("Timers resumed.")
    }

    fn timer_status(&self) -> String {
        let live: Vec<&VoiceTimer> = self.timers.iter().filter(|t| !t.alert_fired).collect();
        if live.is_empty() {
            return "No timers are running.".to_string();
        }
        let parts: Vec<String> = live
            .iter()
            .map(|t| {
                let state = if t.paused_at.is_some() {
                    " (paused)"
                } else {
                    ""
                };
                format!(
                    "{}: {} remaining{}",
                    t.label,
                    format_duration(t.remaining_secs()),
                    state
                )
            })
            .collect();
        parts.join(". ")
    }

    /// Number of timers currently tracked (running or fired). Test/telemetry aid.
    pub fn timer_count(&self) -> usize {
        self.timers.len()
    }

    fn help_text() -> String {
        "You can say: next, back, repeat, what's next, go to step three, \
         how much butter, list ingredients, set a timer for ten minutes, \
         start timer, stop timer, how much time is left, or quit."
            .to_string()
    }

    // --- Step timer inference (mirrors the TUI cook mode) ---

    fn step_timer_seconds(&self, step_index: usize) -> Option<u64> {
        let step = self.recipe.steps.get(step_index)?;
        if let Some(ref sched) = self.schedule {
            for node in &sched.nodes {
                if node.node.step_index == step.order
                    && let Some(ref dur) = node.node.duration
                {
                    return Some(dur.seconds);
                }
            }
        }
        for timer in &step.timers {
            if let Some(ref dur_str) = timer.duration
                && let Some(secs) = parse_duration_str(dur_str)
            {
                return Some(secs);
            }
        }
        None
    }

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
        format!("step {}", step_index + 1)
    }
}

/// Render an ingredient's amount as spoken text, e.g. "2 cups", "3", or `None`
/// when the recipe gives no quantity.
fn format_amount(ing: &RecipeIngredient) -> Option<String> {
    match (&ing.quantity, &ing.unit) {
        (Some(q), Some(u)) if !q.is_empty() && !u.is_empty() => Some(format!("{q} {u}")),
        (Some(q), _) if !q.is_empty() => Some(q.clone()),
        _ => None,
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// "1 step" / "3 steps".
fn pluralize(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use fond_domain::{Recipe, Step, Timer};

    use super::*;
    use crate::command::VoiceCommand;

    fn recipe() -> Recipe {
        Recipe {
            slug: "test".into(),
            title: "Test Stew".into(),
            source: None,
            source_url: None,
            description: None,
            recipe_yield: None,
            prep_time: None,
            cook_time: None,
            total_time: None,
            servings: None,
            ingredients: vec![
                RecipeIngredient {
                    name: "butter".into(),
                    quantity: Some("2".into()),
                    unit: Some("tablespoons".into()),
                    note: None,
                    optional: false,
                },
                RecipeIngredient {
                    name: "eggs".into(),
                    quantity: Some("3".into()),
                    unit: None,
                    note: None,
                    optional: false,
                },
                RecipeIngredient {
                    name: "salt".into(),
                    quantity: None,
                    unit: None,
                    note: None,
                    optional: false,
                },
            ],
            steps: vec![
                Step {
                    section: None,
                    body: "Chop the onions".into(),
                    timers: vec![],
                    order: 0,
                },
                Step {
                    section: None,
                    body: "Simmer".into(),
                    timers: vec![Timer {
                        name: Some("simmer".into()),
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

    fn state() -> VoiceCookState {
        VoiceCookState::new(recipe(), None)
    }

    #[test]
    fn intro_mentions_recipe_and_first_step() {
        let s = state();
        let intro = s.intro();
        assert!(intro.contains("Test Stew"));
        assert!(intro.contains("Step 1 of 3"));
        assert!(intro.contains("Chop the onions"));
    }

    #[test]
    fn next_advances_and_marks_complete() {
        let mut s = state();
        let r = s.apply(VoiceCommand::Next);
        assert!(r.speech.contains("Step 2 of 3"));
        assert_eq!(s.steps_completed(), 1);
    }

    #[test]
    fn next_at_end_reports_done() {
        let mut s = state();
        s.apply(VoiceCommand::GoToStep(3));
        let r = s.apply(VoiceCommand::Next);
        assert!(r.speech.contains("last step"));
        // The final step is marked complete when advancing past it.
        assert_eq!(s.steps_completed(), 1);
    }

    #[test]
    fn previous_clamps_at_first() {
        let mut s = state();
        let r = s.apply(VoiceCommand::Previous);
        assert!(r.speech.contains("first step"));
    }

    #[test]
    fn goto_out_of_range() {
        let mut s = state();
        let r = s.apply(VoiceCommand::GoToStep(9));
        assert!(r.speech.contains("no step 9"));
    }

    #[test]
    fn whats_next_previews_without_advancing() {
        let mut s = state();
        let r = s.apply(VoiceCommand::WhatsNext);
        assert!(r.speech.contains("step 2"));
        assert!(r.speech.contains("Simmer"));
        // Did not advance.
        assert_eq!(s.steps_completed(), 0);
    }

    #[test]
    fn how_much_known_ingredient() {
        let mut s = state();
        let r = s.apply(VoiceCommand::HowMuch("butter".into()));
        assert!(r.speech.contains("2 tablespoons butter"));
    }

    #[test]
    fn how_much_plural_singular_match() {
        let mut s = state();
        let r = s.apply(VoiceCommand::HowMuch("egg".into()));
        assert!(r.speech.contains("3 eggs"), "got: {}", r.speech);
    }

    #[test]
    fn how_much_no_amount() {
        let mut s = state();
        let r = s.apply(VoiceCommand::HowMuch("salt".into()));
        assert!(r.speech.contains("no amount is specified"));
    }

    #[test]
    fn how_much_unknown() {
        let mut s = state();
        let r = s.apply(VoiceCommand::HowMuch("saffron".into()));
        assert!(r.speech.contains("don't see saffron"));
    }

    #[test]
    fn list_ingredients_reads_all() {
        let mut s = state();
        let r = s.apply(VoiceCommand::ListIngredients);
        assert!(r.speech.contains("2 tablespoons butter"));
        assert!(r.speech.contains("3 eggs"));
        assert!(r.speech.contains("salt"));
    }

    #[test]
    fn start_timer_on_timed_step() {
        let mut s = state();
        s.apply(VoiceCommand::GoToStep(2));
        let r = s.apply(VoiceCommand::StartTimer);
        assert!(r.speech.contains("Timer started"));
        assert!(r.speech.contains("10 min") || r.speech.contains("10m"));
        assert_eq!(s.timer_count(), 1);
    }

    #[test]
    fn start_timer_on_untimed_step() {
        let mut s = state();
        let r = s.apply(VoiceCommand::StartTimer);
        assert!(r.speech.contains("doesn't have a timer"));
        assert_eq!(s.timer_count(), 0);
    }

    #[test]
    fn set_explicit_timer() {
        let mut s = state();
        let r = s.apply(VoiceCommand::SetTimer {
            seconds: 300,
            label: Some("pasta".into()),
        });
        assert!(r.speech.contains("pasta"));
        assert_eq!(s.timer_count(), 1);
    }

    #[test]
    fn stop_and_status_timers() {
        let mut s = state();
        assert!(
            s.apply(VoiceCommand::TimerStatus)
                .speech
                .contains("No timers")
        );
        s.apply(VoiceCommand::SetTimer {
            seconds: 300,
            label: None,
        });
        assert!(
            s.apply(VoiceCommand::TimerStatus)
                .speech
                .contains("remaining")
        );
        let r = s.apply(VoiceCommand::StopTimer);
        assert!(r.speech.contains("stopped") || r.speech.contains("Stopped"));
        assert_eq!(s.timer_count(), 0);
    }

    #[test]
    fn zero_second_timer_fires_on_tick() {
        let mut s = state();
        s.apply(VoiceCommand::SetTimer {
            seconds: 0,
            label: Some("instant".into()),
        });
        let fired = s.tick();
        assert_eq!(fired.len(), 1);
        assert!(fired[0].contains("instant"));
        // A second tick does not re-fire.
        assert!(s.tick().is_empty());
    }

    #[test]
    fn quit_sets_flag() {
        let mut s = state();
        let r = s.apply(VoiceCommand::Quit);
        assert!(r.should_quit);
    }

    #[test]
    fn help_lists_commands() {
        let mut s = state();
        let r = s.apply(VoiceCommand::Help);
        assert!(r.speech.contains("next"));
        assert!(r.speech.contains("timer"));
    }
}
