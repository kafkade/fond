use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A parsed recipe — the domain-level representation of a `.cook` file.
///
/// Fields map to Cooklang metadata plus parsed content.  
/// `raw_source` preserves the original `.cook` text so that
/// user-authored files are never silently rewritten.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Recipe {
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
    pub ingredients: Vec<RecipeIngredient>,
    pub steps: Vec<Step>,
    pub cookware: Vec<Cookware>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Original `.cook` file content — preserved for lossless write-back.
    pub raw_source: Option<String>,
}

/// An ingredient reference within a recipe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecipeIngredient {
    pub name: String,
    pub quantity: Option<String>,
    pub unit: Option<String>,
    pub note: Option<String>,
    pub optional: bool,
}

/// A single step (instruction) within a recipe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Step {
    pub section: Option<String>,
    pub body: String,
    pub timers: Vec<Timer>,
    pub order: u32,
}

/// A timer reference extracted from a step.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Timer {
    pub name: Option<String>,
    pub duration: Option<String>,
}

/// A cookware reference extracted from a recipe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Cookware {
    pub name: String,
    pub quantity: Option<String>,
}
