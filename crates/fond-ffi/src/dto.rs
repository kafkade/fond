//! Plain data-transfer records exchanged across the FFI boundary.
//!
//! These are owned, `uniffi`-derived mirrors of internal types. Keeping them
//! separate from the internal structs means the foreign ABI does not shift
//! every time an internal type is refactored.

use chrono::SecondsFormat;
use fond_core::ingredient_class::ScalingCategory;
use fond_core::scale::{ScaledIngredient, ScaledRecipe, ScalingWarning};
use fond_domain::{Cookware, Recipe, RecipeFilter, RecipeIngredient, Step, Timer};
use fond_store::{RecipeSummary, SearchResult, TagCount};
use fond_timeline::{
    DurationSource, ScheduledNode, ScheduledTimeline, StepDuration, TaskType, Timeline,
    TimelineNode,
};

/// A recipe file that failed to parse during reindexing.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ReindexErrorDto {
    pub file: String,
    pub message: String,
}

/// Outcome of rebuilding the index from `.cook` files.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ReindexReportDto {
    /// Number of recipes successfully indexed.
    pub indexed: u32,
    /// Files that failed to parse, with error descriptions.
    pub errors: Vec<ReindexErrorDto>,
}

impl From<fond_store::ReindexReport> for ReindexReportDto {
    fn from(r: fond_store::ReindexReport) -> Self {
        Self {
            indexed: r.indexed as u32,
            errors: r
                .errors
                .into_iter()
                .map(|(file, message)| ReindexErrorDto { file, message })
                .collect(),
        }
    }
}

// ── Listing / search ──────────────────────────────────────────────

/// A lightweight recipe entry for lists and search results.
#[derive(Debug, Clone, uniffi::Record)]
pub struct RecipeSummaryDto {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub source: String,
    pub tags: Vec<String>,
    pub total_time: String,
    pub total_time_minutes: Option<u32>,
}

impl From<RecipeSummary> for RecipeSummaryDto {
    fn from(s: RecipeSummary) -> Self {
        Self {
            id: s.id,
            slug: s.slug,
            title: s.title,
            source: s.source,
            tags: s.tags,
            total_time: s.total_time,
            total_time_minutes: s.total_time_minutes,
        }
    }
}

/// A full-text search hit.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SearchResultDto {
    pub recipe_id: i64,
    pub title: String,
    pub slug: String,
    pub source: String,
    pub tags: Vec<String>,
    pub rank: f64,
}

impl From<SearchResult> for SearchResultDto {
    fn from(r: SearchResult) -> Self {
        Self {
            recipe_id: r.recipe_id,
            title: r.title,
            slug: r.slug,
            source: r.source,
            tags: r.tags,
            rank: r.rank,
        }
    }
}

/// A tag with the number of recipes carrying it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TagCountDto {
    pub name: String,
    pub count: i64,
}

impl From<TagCount> for TagCountDto {
    fn from(t: TagCount) -> Self {
        Self {
            name: t.name,
            count: t.count,
        }
    }
}

/// Optional constraints for listing/searching recipes.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct RecipeFilterDto {
    #[uniffi(default = [])]
    pub tags: Vec<String>,
    #[uniffi(default = None)]
    pub max_time_minutes: Option<u32>,
    #[uniffi(default = None)]
    pub source: Option<String>,
}

impl From<RecipeFilterDto> for RecipeFilter {
    fn from(f: RecipeFilterDto) -> Self {
        Self {
            tags: f.tags,
            max_time_minutes: f.max_time_minutes,
            source: f.source,
        }
    }
}

// ── Full recipe ───────────────────────────────────────────────────

/// An ingredient reference within a recipe.
#[derive(Debug, Clone, uniffi::Record)]
pub struct IngredientDto {
    pub name: String,
    pub quantity: Option<String>,
    pub unit: Option<String>,
    pub note: Option<String>,
    pub optional: bool,
}

impl From<RecipeIngredient> for IngredientDto {
    fn from(i: RecipeIngredient) -> Self {
        Self {
            name: i.name,
            quantity: i.quantity,
            unit: i.unit,
            note: i.note,
            optional: i.optional,
        }
    }
}

/// A timer reference extracted from a step.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TimerDto {
    pub name: Option<String>,
    pub duration: Option<String>,
}

impl From<Timer> for TimerDto {
    fn from(t: Timer) -> Self {
        Self {
            name: t.name,
            duration: t.duration,
        }
    }
}

/// A single instruction step.
#[derive(Debug, Clone, uniffi::Record)]
pub struct StepDto {
    pub section: Option<String>,
    pub body: String,
    pub timers: Vec<TimerDto>,
    pub order: u32,
}

impl From<Step> for StepDto {
    fn from(s: Step) -> Self {
        Self {
            section: s.section,
            body: s.body,
            timers: s.timers.into_iter().map(Into::into).collect(),
            order: s.order,
        }
    }
}

/// A piece of cookware referenced by a recipe.
#[derive(Debug, Clone, uniffi::Record)]
pub struct CookwareDto {
    pub name: String,
    pub quantity: Option<String>,
}

impl From<Cookware> for CookwareDto {
    fn from(c: Cookware) -> Self {
        Self {
            name: c.name,
            quantity: c.quantity,
        }
    }
}

/// A fully parsed recipe with ingredients, steps, and cookware.
#[derive(Debug, Clone, uniffi::Record)]
pub struct RecipeDto {
    pub slug: String,
    pub title: String,
    pub source: Option<String>,
    pub source_url: Option<String>,
    pub description: Option<String>,
    pub recipe_yield: Option<String>,
    pub prep_time: Option<String>,
    pub cook_time: Option<String>,
    pub total_time: Option<String>,
    pub servings: Option<String>,
    pub ingredients: Vec<IngredientDto>,
    pub steps: Vec<StepDto>,
    pub cookware: Vec<CookwareDto>,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Recipe> for RecipeDto {
    fn from(r: Recipe) -> Self {
        Self {
            slug: r.slug,
            title: r.title,
            source: r.source,
            source_url: r.source_url,
            description: r.description,
            recipe_yield: r.recipe_yield,
            prep_time: r.prep_time,
            cook_time: r.cook_time,
            total_time: r.total_time,
            servings: r.servings,
            ingredients: r.ingredients.into_iter().map(Into::into).collect(),
            steps: r.steps.into_iter().map(Into::into).collect(),
            cookware: r.cookware.into_iter().map(Into::into).collect(),
            tags: r.tags,
            created_at: r.created_at.to_rfc3339_opts(SecondsFormat::Secs, true),
            updated_at: r.updated_at.to_rfc3339_opts(SecondsFormat::Secs, true),
        }
    }
}

// ── Scaling ───────────────────────────────────────────────────────

/// How a recipe should be scaled.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum ScaleFactorDto {
    /// Multiply all quantities by `value` (e.g. `2.0` to double).
    Multiplier { value: f64 },
    /// Scale to a target number of servings.
    ToServings { servings: u32 },
}

/// Classification influencing how an ingredient scales.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum ScalingCategoryDto {
    Linear,
    Leavening,
    Salt,
    Spice,
    Thickener,
    Egg,
    Liquid,
    Flour,
    Fat,
}

impl From<ScalingCategory> for ScalingCategoryDto {
    fn from(c: ScalingCategory) -> Self {
        match c {
            ScalingCategory::Linear => ScalingCategoryDto::Linear,
            ScalingCategory::Leavening => ScalingCategoryDto::Leavening,
            ScalingCategory::Salt => ScalingCategoryDto::Salt,
            ScalingCategory::Spice => ScalingCategoryDto::Spice,
            ScalingCategory::Thickener => ScalingCategoryDto::Thickener,
            ScalingCategory::Egg => ScalingCategoryDto::Egg,
            ScalingCategory::Liquid => ScalingCategoryDto::Liquid,
            ScalingCategory::Flour => ScalingCategoryDto::Flour,
            ScalingCategory::Fat => ScalingCategoryDto::Fat,
        }
    }
}

/// An ingredient with original and scaled quantities.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ScaledIngredientDto {
    pub name: String,
    pub original_quantity: Option<String>,
    pub scaled_quantity: Option<String>,
    pub unit: Option<String>,
    pub note: Option<String>,
    pub optional: bool,
    pub category: ScalingCategoryDto,
    pub warning: Option<String>,
    /// Pure-linear value preserved when a non-linear rule adjusted the quantity.
    pub linear_quantity: Option<String>,
    /// Explanation of the non-linear rule applied to this line, if any.
    pub explanation: Option<String>,
}

impl From<ScaledIngredient> for ScaledIngredientDto {
    fn from(i: ScaledIngredient) -> Self {
        Self {
            name: i.name,
            original_quantity: i.original_quantity,
            scaled_quantity: i.scaled_quantity,
            unit: i.unit,
            note: i.note,
            optional: i.optional,
            category: i.category.into(),
            warning: i.warning,
            linear_quantity: i.linear_quantity,
            explanation: i.explanation,
        }
    }
}

/// A warning about a non-linear or unscalable ingredient.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ScalingWarningDto {
    pub ingredient: String,
    pub message: String,
}

impl From<ScalingWarning> for ScalingWarningDto {
    fn from(w: ScalingWarning) -> Self {
        Self {
            ingredient: w.ingredient,
            message: w.message,
        }
    }
}

/// The result of scaling a recipe.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ScaledRecipeDto {
    pub slug: String,
    pub title: String,
    pub scale_factor: f64,
    pub original_servings: Option<String>,
    pub scaled_servings: Option<String>,
    pub prep_time: Option<String>,
    pub cook_time: Option<String>,
    pub total_time: Option<String>,
    pub ingredients: Vec<ScaledIngredientDto>,
    pub warnings: Vec<ScalingWarningDto>,
    pub tags: Vec<String>,
    /// Whether the non-linear rules engine was applied.
    pub rules_applied: bool,
    /// Advisory cook-time suggestion (rules mode). The stated time is never rewritten.
    pub time_suggestion: Option<String>,
    /// Advisory pan/equipment capacity note (rules mode, large scale-ups).
    pub pan_note: Option<String>,
}

impl From<ScaledRecipe> for ScaledRecipeDto {
    fn from(r: ScaledRecipe) -> Self {
        Self {
            slug: r.slug,
            title: r.title,
            scale_factor: r.scale_factor,
            original_servings: r.original_servings,
            scaled_servings: r.scaled_servings,
            prep_time: r.prep_time,
            cook_time: r.cook_time,
            total_time: r.total_time,
            ingredients: r.ingredients.into_iter().map(Into::into).collect(),
            warnings: r.warnings.into_iter().map(Into::into).collect(),
            tags: r.tags,
            rules_applied: r.rules_applied,
            time_suggestion: r.time_suggestion,
            pan_note: r.pan_note,
        }
    }
}

// ── Cooking timeline (cook mode) ──────────────────────────────────

/// Active/passive prep/cook classification of a timeline node.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum TaskTypeDto {
    ActivePrep,
    PassivePrep,
    ActiveCook,
    PassiveCook,
    Rest,
}

impl From<TaskType> for TaskTypeDto {
    fn from(t: TaskType) -> Self {
        match t {
            TaskType::ActivePrep => TaskTypeDto::ActivePrep,
            TaskType::PassivePrep => TaskTypeDto::PassivePrep,
            TaskType::ActiveCook => TaskTypeDto::ActiveCook,
            TaskType::PassiveCook => TaskTypeDto::PassiveCook,
            TaskType::Rest => TaskTypeDto::Rest,
        }
    }
}

/// How a step duration was determined.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum DurationSourceDto {
    Timer,
    Heuristic,
}

impl From<DurationSource> for DurationSourceDto {
    fn from(s: DurationSource) -> Self {
        match s {
            DurationSource::Timer => DurationSourceDto::Timer,
            DurationSource::Heuristic => DurationSourceDto::Heuristic,
        }
    }
}

/// A parsed duration with provenance.
#[derive(Debug, Clone, uniffi::Record)]
pub struct StepDurationDto {
    pub seconds: u64,
    pub source: DurationSourceDto,
    pub original: String,
}

impl From<StepDuration> for StepDurationDto {
    fn from(d: StepDuration) -> Self {
        Self {
            seconds: d.seconds,
            source: d.source.into(),
            original: d.original,
        }
    }
}

/// A node in the cooking timeline DAG.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TimelineNodeDto {
    pub id: u64,
    pub step_index: u32,
    pub label: String,
    pub task_type: TaskTypeDto,
    pub duration: Option<StepDurationDto>,
    pub depends_on: Vec<u64>,
}

impl From<TimelineNode> for TimelineNodeDto {
    fn from(n: TimelineNode) -> Self {
        Self {
            id: n.id.0 as u64,
            step_index: n.step_index,
            label: n.label,
            task_type: n.task_type.into(),
            duration: n.duration.map(Into::into),
            depends_on: n.depends_on.iter().map(|id| id.0 as u64).collect(),
        }
    }
}

/// The unscheduled timeline DAG for a recipe.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TimelineDto {
    pub recipe_title: String,
    pub recipe_slug: String,
    pub nodes: Vec<TimelineNodeDto>,
}

impl From<Timeline> for TimelineDto {
    fn from(t: Timeline) -> Self {
        Self {
            recipe_title: t.recipe_title,
            recipe_slug: t.recipe_slug,
            nodes: t.nodes.into_iter().map(Into::into).collect(),
        }
    }
}

/// A timeline node with computed start/end times (ISO 8601, no timezone).
#[derive(Debug, Clone, uniffi::Record)]
pub struct ScheduledNodeDto {
    pub node: TimelineNodeDto,
    pub scheduled_start: String,
    pub scheduled_end: String,
}

impl From<ScheduledNode> for ScheduledNodeDto {
    fn from(n: ScheduledNode) -> Self {
        Self {
            scheduled_start: n.scheduled_start.format("%Y-%m-%dT%H:%M:%S").to_string(),
            scheduled_end: n.scheduled_end.format("%Y-%m-%dT%H:%M:%S").to_string(),
            node: n.node.into(),
        }
    }
}

/// A fully scheduled cooking timeline, working backward from serve time.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ScheduledTimelineDto {
    pub recipe_title: String,
    pub recipe_slug: String,
    pub serve_at: String,
    pub start_at: String,
    pub total_active_seconds: u64,
    pub total_passive_seconds: u64,
    pub nodes: Vec<ScheduledNodeDto>,
    pub has_untimed_steps: bool,
}

impl From<ScheduledTimeline> for ScheduledTimelineDto {
    fn from(t: ScheduledTimeline) -> Self {
        Self {
            recipe_title: t.recipe_title,
            recipe_slug: t.recipe_slug,
            serve_at: t.serve_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
            start_at: t.start_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
            total_active_seconds: t.total_active_seconds,
            total_passive_seconds: t.total_passive_seconds,
            nodes: t.nodes.into_iter().map(Into::into).collect(),
            has_untimed_steps: t.has_untimed_steps,
        }
    }
}
