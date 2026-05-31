use serde::{Deserialize, Serialize};

/// A recipe as stored in Paprika's gzipped JSON format.
///
/// All fields are optional except `name` to handle partial exports and
/// version differences. Unknown fields are captured by `extra` via
/// `serde(flatten)` for forward compatibility and lossless import.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaprikaRecipe {
    pub name: String,

    #[serde(default)]
    pub uid: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub ingredients: Option<String>,
    #[serde(default)]
    pub directions: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub servings: Option<String>,
    #[serde(default)]
    pub prep_time: Option<String>,
    #[serde(default)]
    pub cook_time: Option<String>,
    #[serde(default)]
    pub total_time: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
    /// Skipped during deserialization to avoid memory pressure from
    /// large base64 photo blobs. Photo extraction is handled separately.
    #[serde(default, skip_deserializing)]
    pub photo: Option<String>,
    #[serde(default)]
    pub photo_url: Option<String>,
    #[serde(default)]
    pub photo_hash: Option<String>,
    #[serde(default)]
    pub categories: Option<Vec<String>>,
    #[serde(default)]
    pub nutrition: Option<String>,
    #[serde(default)]
    pub rating: Option<i32>,
    #[serde(default)]
    pub difficulty: Option<String>,
    #[serde(default, rename = "yield")]
    pub recipe_yield: Option<String>,
    #[serde(default)]
    pub on_favorites: Option<bool>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub scale: Option<serde_json::Value>,

    /// Captures any fields not explicitly modeled, for forward compatibility.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Entry parse result from archive iteration.
#[derive(Debug)]
pub struct ParsedEntry {
    pub entry_name: String,
    pub result: Result<PaprikaRecipe, String>,
}
