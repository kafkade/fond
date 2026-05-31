//! Cooking timeline engine: DAG model and backward scheduling.
//!
//! Converts a recipe's steps into a directed acyclic graph,
//! extracts durations from timer annotations and step text,
//! and schedules backward from a target serve time.

pub mod build;
pub mod classify;
pub mod duration;
pub mod model;
pub mod schedule;

pub use build::build_timeline;
pub use model::*;
pub use schedule::schedule_backward;
