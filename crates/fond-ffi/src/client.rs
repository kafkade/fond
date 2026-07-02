//! The [`FondClient`] interface object — the single entry point exposed to
//! foreign callers.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::NaiveDateTime;
use fond_core::scale::{ScaleFactor, ScaleOptions, scale_recipe_with};
use fond_domain::{Block, CookDocument, Recipe, escape_fts5_query, parse_cook, slugify};
use fond_store::{
    FondDb, RecipeRepository, bytes_hash, content_hash, read_recipe_file, reindex,
    remove_old_file_after_rename, write_recipe_file,
};
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
    ///
    /// When `rules` is `true`, applies the deterministic non-linear adjustment
    /// engine (sub-linear leavening, to-taste seasoning bands, pan-coating fat
    /// notes, advisory cook-time/pan suggestions). When `false` (the default),
    /// scaling is purely linear with non-linear warnings only.
    #[uniffi::method(default(rules = false))]
    pub fn scale_recipe(
        &self,
        slug: String,
        factor: ScaleFactorDto,
        rules: bool,
    ) -> Result<ScaledRecipeDto, FondError> {
        let recipe = self.load_recipe(&slug)?;
        let factor = match factor {
            ScaleFactorDto::Multiplier { value } => ScaleFactor::Multiplier(value),
            ScaleFactorDto::ToServings { servings } => ScaleFactor::ToServings(servings),
        };
        let scaled = scale_recipe_with(&recipe, factor, ScaleOptions { rules })?;
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

    // ── Editing / write-back ──────────────────────────────────────

    /// Load a recipe in editable form: raw metadata, raw body blocks, a
    /// content-hash concurrency token, and a parsed ingredient preview.
    ///
    /// Returns `None` if the slug is unknown.
    pub fn get_recipe_for_edit(&self, slug: String) -> Result<Option<RecipeEditorDto>, FondError> {
        let db = self.db.lock().expect("db mutex poisoned");
        let Some(record) = RecipeRepository::new(&db).get_recipe_by_slug(&slug)? else {
            return Ok(None);
        };
        let raw = self.current_source(&record.file_path, &record.raw_source);
        drop(db);

        let hash = content_hash(&raw);
        let stem = record.file_path.trim_end_matches(".cook");
        let recipe = parse_cook(&raw, stem)?;
        let doc = CookDocument::parse(&raw);

        Ok(Some(RecipeEditorDto {
            slug: record.slug,
            content_hash: hash,
            raw_source: raw,
            title: recipe.title,
            servings: recipe.servings,
            description: recipe.description,
            source: recipe.source,
            source_url: recipe.source_url,
            prep_time: recipe.prep_time,
            cook_time: recipe.cook_time,
            total_time: recipe.total_time,
            image: doc.get(&["image"]),
            tags: recipe.tags,
            blocks: doc.sectioned_blocks().into_iter().map(Into::into).collect(),
            ingredients: recipe.ingredients.into_iter().map(Into::into).collect(),
        }))
    }

    /// Parse editable body blocks into an ingredient list, without touching
    /// disk or the index. Powers a live "ingredients" preview while the user
    /// edits Cooklang step text — keeping the parsing logic in Rust (ADR-011).
    pub fn preview_ingredients(
        &self,
        blocks: Vec<String>,
    ) -> Result<Vec<IngredientDto>, FondError> {
        let doc = CookDocument::new_recipe("Preview", None, &[], None, None, &blocks);
        let recipe = parse_cook(&doc.emit(), "preview")?;
        Ok(recipe.ingredients.into_iter().map(Into::into).collect())
    }

    /// Create a new recipe `.cook` file from structured fields and index it.
    ///
    /// Fails with `AlreadyExists` if a recipe with the same slug already exists.
    pub fn create_recipe(&self, input: NewRecipeDto) -> Result<RecipeDto, FondError> {
        let title = input.title.trim();
        if title.is_empty() {
            return Err(FondError::InvalidArgument {
                message: "title cannot be empty".into(),
            });
        }
        let slug = slugify(title);
        if slug.is_empty() {
            return Err(FondError::InvalidArgument {
                message: "title does not produce a valid slug".into(),
            });
        }
        let file_name = format!("{slug}.cook");
        let recipes_dir = self.recipes_dir();

        let doc = CookDocument::new_recipe(
            title,
            input.servings.as_deref(),
            &input.tags,
            input.description.as_deref(),
            input.source.as_deref(),
            &input.steps,
        );
        let raw = doc.emit();

        let db = self.db.lock().expect("db mutex poisoned");
        if RecipeRepository::new(&db)
            .get_recipe_by_slug(&slug)?
            .is_some()
        {
            return Err(FondError::AlreadyExists { slug });
        }
        let result = write_recipe_file(&db, &recipes_dir, &file_name, &raw)?;
        Ok(RecipeDto::from(result.recipe))
    }

    /// Save structured edits to an existing recipe (metadata + body blocks).
    ///
    /// Uses `base_content_hash` as an optimistic-concurrency guard and renames
    /// the file if the title (hence slug) changed.
    pub fn save_recipe(&self, input: SaveRecipeDto) -> Result<RecipeDto, FondError> {
        let recipes_dir = self.recipes_dir();
        let db = self.db.lock().expect("db mutex poisoned");

        let record = RecipeRepository::new(&db)
            .get_recipe_by_slug(&input.slug)?
            .ok_or_else(|| FondError::NotFound {
                slug: input.slug.clone(),
            })?;
        let current = self.current_source(&record.file_path, &record.raw_source);
        self.check_conflict(&current, &input.base_content_hash)?;

        let mut doc = CookDocument::parse(&current);
        doc.set_scalar("title", &["title"], Some(&input.title));
        doc.set_scalar("servings", &["servings"], input.servings.as_deref());
        doc.set_scalar(
            "description",
            &["description"],
            input.description.as_deref(),
        );
        doc.set_scalar("source", &["source"], input.source.as_deref());
        doc.set_scalar(
            "source url",
            &["source url", "source_url"],
            input.source_url.as_deref(),
        );
        doc.set_scalar(
            "prep time",
            &["prep time", "prep_time"],
            input.prep_time.as_deref(),
        );
        doc.set_scalar(
            "cook time",
            &["cook time", "cook_time"],
            input.cook_time.as_deref(),
        );
        doc.set_scalar(
            "total time",
            &["total time", "total_time"],
            input.total_time.as_deref(),
        );
        doc.set_scalar("image", &["image"], input.image.as_deref());
        doc.set_tags(&input.tags);

        // Only rewrite the body when the blocks actually changed, so a
        // metadata-only save preserves step text byte-for-byte.
        let new_blocks: Vec<Block> = input.blocks.iter().map(Into::into).collect();
        if new_blocks != doc.blocks() {
            doc.set_blocks(new_blocks);
        }

        let raw = doc.emit();
        let result = self.commit_write(&db, &recipes_dir, &record.slug, &record.file_path, &raw)?;
        Ok(RecipeDto::from(result.recipe))
    }

    /// Save a full raw `.cook` source back to disk (source-editor mode).
    ///
    /// Validates that the text parses, guards against concurrent edits, and
    /// renames the file if the title changed.
    pub fn save_recipe_source(
        &self,
        slug: String,
        raw: String,
        base_content_hash: String,
    ) -> Result<RecipeDto, FondError> {
        let recipes_dir = self.recipes_dir();
        let db = self.db.lock().expect("db mutex poisoned");

        let record = RecipeRepository::new(&db)
            .get_recipe_by_slug(&slug)?
            .ok_or_else(|| FondError::NotFound { slug: slug.clone() })?;
        let current = self.current_source(&record.file_path, &record.raw_source);
        self.check_conflict(&current, &base_content_hash)?;

        let result = self.commit_write(&db, &recipes_dir, &record.slug, &record.file_path, &raw)?;
        Ok(RecipeDto::from(result.recipe))
    }

    /// Attach a photo: store its bytes content-addressed under `photos/` and
    /// record the relative path in the recipe's `image:` frontmatter.
    ///
    /// Returns the stored relative path (e.g. `photos/ab/cdef….jpg`).
    pub fn attach_photo(
        &self,
        slug: String,
        bytes: Vec<u8>,
        extension: String,
        base_content_hash: String,
    ) -> Result<String, FondError> {
        if bytes.is_empty() {
            return Err(FondError::InvalidArgument {
                message: "photo data is empty".into(),
            });
        }
        let recipes_dir = self.recipes_dir();
        let db = self.db.lock().expect("db mutex poisoned");

        let record = RecipeRepository::new(&db)
            .get_recipe_by_slug(&slug)?
            .ok_or_else(|| FondError::NotFound { slug: slug.clone() })?;
        let current = self.current_source(&record.file_path, &record.raw_source);
        self.check_conflict(&current, &base_content_hash)?;

        // Content-addressed path: photos/<first-2>/<rest>.<ext> (ADR-002).
        let hash = bytes_hash(&bytes);
        let ext = sanitize_extension(&extension);
        let (shard, rest) = hash.split_at(2);
        let rel_path = format!("photos/{shard}/{rest}.{ext}");
        let abs_path = self.data_dir.join(&rel_path);
        if let Some(parent) = abs_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&abs_path, &bytes)?;

        // Record the link in the .cook frontmatter (source of truth).
        let mut doc = CookDocument::parse(&current);
        doc.set_scalar("image", &["image"], Some(&rel_path));
        let raw = doc.emit();
        write_recipe_file(&db, &recipes_dir, &record.file_path, &raw)?;

        Ok(rel_path)
    }

    /// Delete a recipe's `.cook` file and its index row. Returns whether a
    /// recipe was removed.
    pub fn delete_recipe(&self, slug: String) -> Result<bool, FondError> {
        let recipes_dir = self.recipes_dir();
        let db = self.db.lock().expect("db mutex poisoned");
        Ok(fond_store::delete_recipe(&db, &recipes_dir, &slug)?)
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

    /// The `recipes/` directory under the data dir.
    fn recipes_dir(&self) -> PathBuf {
        self.data_dir.join("recipes")
    }

    /// Read the current source of truth: the on-disk `.cook` file if present,
    /// falling back to the DB's indexed `raw_source`.
    fn current_source(&self, file_path: &str, db_raw: &str) -> String {
        match read_recipe_file(&self.recipes_dir(), file_path) {
            Ok(Some(raw)) => raw,
            _ => db_raw.to_string(),
        }
    }

    /// Fail with `Conflict` if the current content no longer matches the base
    /// hash the caller loaded.
    fn check_conflict(&self, current: &str, base_hash: &str) -> Result<(), FondError> {
        if content_hash(current) != base_hash {
            return Err(FondError::Conflict {
                message: "the recipe changed on disk since it was loaded; reload and retry".into(),
            });
        }
        Ok(())
    }

    /// Write `raw`, renaming the file when the title-derived slug changed and
    /// cleaning up the previous file/index row.
    fn commit_write(
        &self,
        db: &FondDb,
        recipes_dir: &Path,
        old_slug: &str,
        old_file: &str,
        raw: &str,
    ) -> Result<fond_store::WriteResult, FondError> {
        let parsed = parse_cook(raw, old_file.trim_end_matches(".cook"))?;
        let new_slug = parsed.slug;
        if new_slug.is_empty() {
            return Err(FondError::InvalidArgument {
                message: "recipe title does not produce a valid slug".into(),
            });
        }
        let new_file = format!("{new_slug}.cook");

        if new_file != old_file {
            if let Some(other) = RecipeRepository::new(db).get_recipe_by_slug(&new_slug)?
                && other.file_path != old_file
            {
                return Err(FondError::AlreadyExists { slug: new_slug });
            }
            let result = write_recipe_file(db, recipes_dir, &new_file, raw)?;
            remove_old_file_after_rename(db, recipes_dir, old_slug, old_file)?;
            Ok(result)
        } else {
            Ok(write_recipe_file(db, recipes_dir, &new_file, raw)?)
        }
    }
}

/// Normalise a user-supplied file extension for content-addressed storage.
fn sanitize_extension(ext: &str) -> String {
    let cleaned: String = ext
        .trim()
        .trim_start_matches('.')
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    if cleaned.is_empty() {
        "jpg".to_string()
    } else {
        cleaned
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
