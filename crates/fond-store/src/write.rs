//! Writing `.cook` files back to disk and keeping the derived index in sync.
//!
//! `.cook` files are the source of truth (ADR-002); every write updates the
//! file first and then upserts the SQLite index. These helpers are shared by
//! the CLI-adjacent callers and the `fond-ffi` bridge so the write path has a
//! single implementation.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::db::FondDb;
use crate::error::StoreError;
use crate::repo::RecipeRepository;

/// Stable content hash of a `.cook` file's text.
///
/// Matches the hash written by [`crate::reindex`] so callers can compare a
/// freshly-read file against the indexed `content_hash` for optimistic
/// concurrency checks.
pub fn content_hash(content: &str) -> String {
    let mut h = DefaultHasher::new();
    content.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Content hash of arbitrary bytes, used for content-addressed photo storage.
pub fn bytes_hash(bytes: &[u8]) -> String {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// The result of writing and indexing a `.cook` file.
#[derive(Debug)]
pub struct WriteResult {
    /// The parsed recipe (as indexed).
    pub recipe: fond_domain::Recipe,
    /// The content hash of the written file.
    pub content_hash: String,
    /// The file name (relative to the recipes directory) that was written.
    pub file_name: String,
}

/// Write `raw` to `<recipes_dir>/<file_name>` and upsert the derived index.
///
/// The content is parsed **before** anything is written, so an invalid recipe
/// aborts without touching disk or the index.
pub fn write_recipe_file(
    db: &FondDb,
    recipes_dir: &Path,
    file_name: &str,
    raw: &str,
) -> Result<WriteResult, StoreError> {
    let stem = file_name.trim_end_matches(".cook");
    let recipe = fond_domain::parse_cook(raw, stem).map_err(|e| StoreError::Parse {
        file: file_name.to_string(),
        message: e.to_string(),
    })?;

    fs::create_dir_all(recipes_dir)?;
    fs::write(recipes_dir.join(file_name), raw)?;

    let hash = content_hash(raw);
    RecipeRepository::new(db).upsert_recipe(file_name, &recipe, &hash)?;

    Ok(WriteResult {
        recipe,
        content_hash: hash,
        file_name: file_name.to_string(),
    })
}

/// Read the current on-disk content of a recipe file, if it exists.
pub fn read_recipe_file(recipes_dir: &Path, file_name: &str) -> Result<Option<String>, StoreError> {
    let path = recipes_dir.join(file_name);
    match fs::read_to_string(&path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Delete a recipe's `.cook` file and its index row, keyed by slug.
///
/// Returns `true` if a recipe was removed.
pub fn delete_recipe(db: &FondDb, recipes_dir: &Path, slug: &str) -> Result<bool, StoreError> {
    let removed = RecipeRepository::new(db).delete_recipe_by_slug(slug)?;
    if let Some(file_path) = removed {
        let path = recipes_dir.join(&file_path);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Remove a stale `.cook` file and index row after a rename moved a recipe to a
/// new file name. Best-effort on the file; always clears the old index row.
pub fn remove_old_file_after_rename(
    db: &FondDb,
    recipes_dir: &Path,
    old_slug: &str,
    old_file_name: &str,
) -> Result<(), StoreError> {
    RecipeRepository::new(db).delete_recipe_by_slug(old_slug)?;
    let path = recipes_dir.join(old_file_name);
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}
