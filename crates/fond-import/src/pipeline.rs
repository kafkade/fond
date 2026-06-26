use serde::Serialize;

/// The result of importing a single recipe from an external source.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status")]
pub enum ImportResult {
    /// Recipe was successfully converted and is ready to write.
    #[serde(rename = "imported")]
    Imported {
        title: String,
        slug: String,
        file_name: String,
    },
    /// Recipe was queued for manual review before it can be written.
    #[serde(rename = "queued")]
    Queued {
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        review_id: Option<String>,
        reason: String,
    },
    /// Recipe was skipped because a duplicate already exists.
    #[serde(rename = "skipped")]
    Skipped { title: String, reason: String },
    /// Recipe failed to convert.
    #[serde(rename = "failed")]
    Failed { entry_name: String, error: String },
}

/// Summary report for a batch import operation.
#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    pub imported: usize,
    pub queued: usize,
    pub skipped: usize,
    pub failed: usize,
    pub total: usize,
    pub details: Vec<ImportResult>,
}

impl ImportReport {
    pub fn new() -> Self {
        Self {
            imported: 0,
            queued: 0,
            skipped: 0,
            failed: 0,
            total: 0,
            details: Vec::new(),
        }
    }

    pub fn add(&mut self, result: ImportResult) {
        self.total += 1;
        match &result {
            ImportResult::Imported { .. } => self.imported += 1,
            ImportResult::Queued { .. } => self.queued += 1,
            ImportResult::Skipped { .. } => self.skipped += 1,
            ImportResult::Failed { .. } => self.failed += 1,
        }
        self.details.push(result);
    }
}

impl Default for ImportReport {
    fn default() -> Self {
        Self::new()
    }
}

/// A recipe that has been converted and is ready to write to disk.
///
/// Contains the domain `Recipe`, the generated `.cook` file text,
/// and the target filename.
#[derive(Debug, Clone)]
pub struct PreparedRecipe {
    pub recipe: fond_domain::Recipe,
    pub cook_text: String,
    pub file_name: String,
}

/// A draft recipe that must be reviewed before becoming a canonical `.cook` file.
#[derive(Debug, Clone, Serialize)]
pub struct ReviewDraft {
    pub title: String,
    pub source_name: String,
    pub cook_text: String,
    pub raw_text: String,
    pub warnings: Vec<String>,
}
