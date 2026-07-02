//! Cooking timeline engine: DAG model and backward scheduling.
//!
//! Converts a recipe's steps into a directed acyclic graph,
//! extracts durations from timer annotations and step text,
//! and schedules backward from a target serve time.

pub mod build;
pub mod classify;
pub mod coordinate;
pub mod duration;
pub mod infer;
pub mod model;
pub mod resource;
pub mod schedule;

pub use build::build_timeline;
pub use coordinate::{
    Conflict, ConflictKind, MealTimeline, RecipeSource, ScheduledMeal, ScheduledMealNode,
    merge_timelines, schedule_meal,
};
pub use model::*;
pub use resource::{KitchenResources, OvenTemp, ResourceKind, ResourceRequirement};
pub use schedule::schedule_backward;
