use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for a timeline node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub usize);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Task type classification for a timeline node.
///
/// Distinguishes active (hands-on) from passive (hands-off) work,
/// and prep from cooking stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskType {
    /// Hands-on preparation: chopping, mixing, measuring.
    ActivePrep,
    /// Hands-off preparation: marinating, soaking, rising.
    PassivePrep,
    /// Hands-on cooking: searing, sautéing, stirring.
    ActiveCook,
    /// Hands-off cooking: simmering, baking, roasting.
    PassiveCook,
    /// Resting: meat resting, cooling.
    Rest,
}

impl TaskType {
    /// Whether this task type requires active attention from the cook.
    pub fn is_active(&self) -> bool {
        matches!(self, TaskType::ActivePrep | TaskType::ActiveCook)
    }

    /// Short label for display.
    pub fn label(&self) -> &'static str {
        match self {
            TaskType::ActivePrep => "prep",
            TaskType::PassivePrep => "passive",
            TaskType::ActiveCook => "cook",
            TaskType::PassiveCook => "passive",
            TaskType::Rest => "rest",
        }
    }
}

impl fmt::Display for TaskType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskType::ActivePrep => write!(f, "Active Prep"),
            TaskType::PassivePrep => write!(f, "Passive Prep"),
            TaskType::ActiveCook => write!(f, "Active Cook"),
            TaskType::PassiveCook => write!(f, "Passive Cook"),
            TaskType::Rest => write!(f, "Rest"),
        }
    }
}

/// How a duration was determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DurationSource {
    /// Extracted from a `~timer{}` Cooklang annotation.
    Timer,
    /// Parsed heuristically from step body text.
    Heuristic,
}

/// A parsed duration with provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepDuration {
    /// Duration in seconds.
    pub seconds: u64,
    /// How this duration was determined.
    pub source: DurationSource,
    /// Original text (e.g., "30 minutes").
    pub original: String,
}

/// A node in the cooking timeline DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineNode {
    /// Unique identifier within the timeline.
    pub id: NodeId,
    /// Index into the recipe's step list (provenance).
    pub step_index: u32,
    /// Human-readable label (timer name or truncated step body).
    pub label: String,
    /// Classification of the work involved.
    pub task_type: TaskType,
    /// Known duration, if any. `None` means untimed.
    pub duration: Option<StepDuration>,
    /// Nodes that must complete before this one can start.
    pub depends_on: Vec<NodeId>,
}

/// The full timeline DAG for a recipe (unscheduled).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub recipe_title: String,
    pub recipe_slug: String,
    pub nodes: Vec<TimelineNode>,
}

/// A node with computed schedule times.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledNode {
    /// The underlying timeline node.
    #[serde(flatten)]
    pub node: TimelineNode,
    /// Computed start time. Present for all nodes in a valid schedule.
    /// For untimed nodes, this equals `scheduled_end` (zero-duration position).
    pub scheduled_start: NaiveDateTime,
    /// Computed end time.
    pub scheduled_end: NaiveDateTime,
}

/// A fully scheduled cooking timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTimeline {
    pub recipe_title: String,
    pub recipe_slug: String,
    /// Target serve/finish time.
    pub serve_at: NaiveDateTime,
    /// Earliest start time across all nodes.
    pub start_at: NaiveDateTime,
    /// Sum of durations for active (hands-on) steps, in seconds.
    pub total_active_seconds: u64,
    /// Sum of durations for passive (hands-off) steps, in seconds.
    pub total_passive_seconds: u64,
    /// Scheduled nodes in topological order.
    pub nodes: Vec<ScheduledNode>,
    /// Whether any steps have unknown duration.
    pub has_untimed_steps: bool,
}
