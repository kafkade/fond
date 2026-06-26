//! The [`FondClient`] interface object — the single entry point exposed to
//! foreign callers.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::NaiveDateTime;
use fond_core::scale::{ScaleFactor, scale_recipe};
use fond_domain::{Recipe, escape_fts5_query, parse_cook};
use fond_store::{FondDb, RecipeRepository, reindex};
use fond_timeline::{build_timeline, schedule_backward};

use crate::dto::*;
use crate::error::FondError;

/// A handle onto a fond data directory.
///
/// Opens (and migrates) the SQLite index at `<data_dir>/fond.db` and reads
/// `.cook` source from the recipe records. All access is serialized through a
/// `Mutex` because `rusqlite::Connection` is `!Send`; this matches the
/// single-household, low-concurrency assumption fond is built on.
#[derive(uniffi::Object)]
pub struct FondClient {
    db: Mutex<FondDb>,
    data_dir: PathBuf,
}

#[uniffi::export]
impl FondClient {
    /// Open the fond database under `data_dir`, creating and migrating it if
    /// necessary.
    #[uniffi::constructor]
    pub fn new(data_dir: String) -> Result<Arc<Self>, FondError> {
        let data_dir = PathBuf::from(data_dir);
        let db_path = data_dir.join("fond.db");
        let db = FondDb::open(&db_path)?;
        Ok(Arc::new(Self {
            db: Mutex::new(db),
            data_dir,
        }))
    }

    /// Number of indexed recipes.
    pub fn count_recipes(&self) -> Result<i64, FondError> {
        let db = self.db.lock().expect("db mutex poisoned");
        Ok(RecipeRepository::new(&db).count_recipes()?)
    }

    /// Rebuild the derived index from the `.cook` files in `<data_dir>/recipes`.
    ///
    /// The SQLite database is a disposable index — this regenerates it from the
    /// source-of-truth files. Apps call this after seeding bundled recipes.
    pub fn reindex(&self) -> Result<ReindexReportDto, FondError> {
        let recipes_dir = self.data_dir.join("recipes");
        let db = self.db.lock().expect("db mutex poisoned");
        Ok(reindex(&db, &recipes_dir)?.into())
    }

    /// List recipes, optionally constrained by tags / max time / source.
    pub fn list_recipes(
        &self,
        filter: Option<RecipeFilterDto>,
    ) -> Result<Vec<RecipeSummaryDto>, FondError> {
        let filter = filter.unwrap_or_default().into();
        let db = self.db.lock().expect("db mutex poisoned");
        let recipes = RecipeRepository::new(&db).list_recipes_filtered(&filter)?;
        Ok(recipes.into_iter().map(Into::into).collect())
    }

    /// Full-text search across recipes, optionally filtered.
    ///
    /// The query is escaped for FTS5, so callers can pass raw user input.
    pub fn search(
        &self,
        query: String,
        filter: Option<RecipeFilterDto>,
    ) -> Result<Vec<SearchResultDto>, FondError> {
        let escaped = escape_fts5_query(&query);
        if escaped.is_empty() {
            return Ok(Vec::new());
        }
        let filter = filter.unwrap_or_default().into();
        let db = self.db.lock().expect("db mutex poisoned");
        let results = RecipeRepository::new(&db).search_filtered(&escaped, &filter)?;
        Ok(results.into_iter().map(Into::into).collect())
    }

    /// All tags with their recipe counts, most-used first.
    pub fn list_tags(&self) -> Result<Vec<TagCountDto>, FondError> {
        let db = self.db.lock().expect("db mutex poisoned");
        let tags = RecipeRepository::new(&db).list_tags()?;
        Ok(tags.into_iter().map(Into::into).collect())
    }

    /// Fetch and parse a single recipe by slug, including ingredients, steps,
    /// and cookware. Returns `None` if the slug is unknown.
    pub fn get_recipe(&self, slug: String) -> Result<Option<RecipeDto>, FondError> {
        let db = self.db.lock().expect("db mutex poisoned");
        let Some(record) = RecipeRepository::new(&db).get_recipe_by_slug(&slug)? else {
            return Ok(None);
        };
        drop(db);

        let recipe = parse_cook(&record.raw_source, &record.slug)?;
        let mut dto = RecipeDto::from(recipe);
        // The DB record carries the authoritative index timestamps.
        if !record.created_at.is_empty() {
            dto.created_at = record.created_at;
        }
        if !record.updated_at.is_empty() {
            dto.updated_at = record.updated_at;
        }
        Ok(Some(dto))
    }

    /// Scale a recipe's ingredient quantities. Times are never modified.
    pub fn scale_recipe(
        &self,
        slug: String,
        factor: ScaleFactorDto,
    ) -> Result<ScaledRecipeDto, FondError> {
        let recipe = self.load_recipe(&slug)?;
        let factor = match factor {
            ScaleFactorDto::Multiplier { value } => ScaleFactor::Multiplier(value),
            ScaleFactorDto::ToServings { servings } => ScaleFactor::ToServings(servings),
        };
        let scaled = scale_recipe(&recipe, factor)?;
        Ok(scaled.into())
    }

    /// Build the unscheduled cooking-timeline DAG for a recipe.
    pub fn build_timeline(&self, slug: String) -> Result<TimelineDto, FondError> {
        let recipe = self.load_recipe(&slug)?;
        Ok(build_timeline(&recipe).into())
    }

    /// Schedule a recipe's timeline backward from a target serve time.
    ///
    /// `serve_at` is an ISO 8601 local datetime, e.g. `2026-01-31T18:30:00`
    /// or `2026-01-31T18:30`.
    pub fn schedule_timeline(
        &self,
        slug: String,
        serve_at: String,
    ) -> Result<ScheduledTimelineDto, FondError> {
        let recipe = self.load_recipe(&slug)?;
        let serve_at = parse_naive_datetime(&serve_at)?;
        let timeline = build_timeline(&recipe);
        Ok(schedule_backward(&timeline, serve_at).into())
    }
}

impl FondClient {
    /// Load and parse a recipe by slug, erroring with `NotFound` if absent.
    fn load_recipe(&self, slug: &str) -> Result<Recipe, FondError> {
        let db = self.db.lock().expect("db mutex poisoned");
        let record = RecipeRepository::new(&db)
            .get_recipe_by_slug(slug)?
            .ok_or_else(|| FondError::NotFound {
                slug: slug.to_string(),
            })?;
        drop(db);
        Ok(parse_cook(&record.raw_source, &record.slug)?)
    }
}

/// Parse an ISO 8601 local datetime, tolerating a missing seconds component.
fn parse_naive_datetime(s: &str) -> Result<NaiveDateTime, FondError> {
    let s = s.trim();
    for fmt in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Ok(dt);
        }
    }
    Err(FondError::InvalidArgument {
        message: format!("could not parse serve_at '{s}' as an ISO 8601 datetime"),
    })
}
