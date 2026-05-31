use serde::Deserialize;

/// A recipe extracted from schema.org/JSON-LD structured data.
///
/// All optional fields use `serde_json::Value` or `Option<Vec<String>>`
/// for tolerant deserialization — real-world schema.org data has highly
/// polymorphic field shapes.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaRecipe {
    pub name: String,

    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<serde_json::Value>,
    #[serde(default)]
    pub date_published: Option<String>,
    #[serde(default)]
    pub image: Option<serde_json::Value>,
    #[serde(default)]
    pub recipe_yield: Option<serde_json::Value>,
    #[serde(default)]
    pub prep_time: Option<String>,
    #[serde(default)]
    pub cook_time: Option<String>,
    #[serde(default)]
    pub total_time: Option<String>,
    #[serde(default)]
    pub recipe_category: Option<serde_json::Value>,
    #[serde(default)]
    pub recipe_cuisine: Option<serde_json::Value>,
    #[serde(default)]
    pub keywords: Option<serde_json::Value>,
    #[serde(default)]
    pub nutrition: Option<serde_json::Value>,
    #[serde(default)]
    pub aggregate_rating: Option<serde_json::Value>,
    #[serde(default)]
    pub recipe_ingredient: Option<Vec<String>>,
    #[serde(default)]
    pub recipe_instructions: Option<serde_json::Value>,
    #[serde(default)]
    pub video: Option<serde_json::Value>,
    #[serde(default)]
    pub suitable_for_diet: Option<serde_json::Value>,

    /// Source URL — set by the caller, not from JSON-LD.
    #[serde(skip)]
    pub source_url: Option<String>,
}

/// Extraction confidence level for import quality tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ExtractionConfidence {
    /// Recipe extracted from structured JSON-LD data.
    Structured,
    /// Recipe extracted from HTML heuristics (lower confidence).
    Fallback,
}
