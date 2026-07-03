//! Community recipe **sharing** — the ownership-preserving, opt-in bundle
//! format (ROADMAP §13 Phase 8, ADR-017).
//!
//! A shared recipe travels as a self-contained `.fondshare` **bundle**: a ZIP
//! archive holding the source-of-truth `.cook` files verbatim, any linked
//! photos, and a [`ShareManifest`] describing attribution, license, and
//! provenance. Bundles are just files — they move over git, USB, email, or a
//! synced folder — so sharing never requires a central server and nothing
//! leaves a device without the user's explicit, per-action consent.
//!
//! This module is **pure**: it defines the manifest schema, deterministic
//! content digests, round-trip-safe provenance stamping, and the
//! idempotency/dedup decision used when a bundle is imported through the
//! review pipeline (ADR-010). All ZIP and filesystem I/O lives in the CLI.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

/// Current bundle manifest schema version.
pub const BUNDLE_SCHEMA_VERSION: u32 = 1;

/// Canonical file name of the manifest inside a bundle.
pub const MANIFEST_FILE: &str = "manifest.json";

/// Directory (inside a bundle) holding the `.cook` source files.
pub const RECIPES_DIR: &str = "recipes";

/// Directory (inside a bundle) holding linked photo blobs.
pub const PHOTOS_DIR: &str = "photos";

/// Conventional file extension for a share bundle.
pub const BUNDLE_EXTENSION: &str = "fondshare";

/// The `source_type` recorded on review-queue drafts created by a bundle import.
pub const SHARE_SOURCE_TYPE: &str = "shared-bundle";

/// A shareable bundle's manifest — the trust and provenance record that rides
/// alongside the `.cook` files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShareManifest {
    /// Manifest schema version (see [`BUNDLE_SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// Version of fond that produced the bundle.
    pub fond_version: String,
    /// Stable, time-ordered bundle identity (UUIDv7) — used for provenance and
    /// to make re-imports idempotent.
    pub bundle_id: String,
    /// RFC-3339 timestamp the bundle was created.
    pub created_at: String,
    /// Who assembled/shared the bundle, if they chose to attribute themselves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_by: Option<String>,
    /// The recipes carried by the bundle.
    pub recipes: Vec<ManifestRecipe>,
}

impl ShareManifest {
    /// Validate that this manifest is well-formed and a version fond can read.
    pub fn validate(&self) -> Result<(), ShareError> {
        if self.schema_version == 0 || self.schema_version > BUNDLE_SCHEMA_VERSION {
            return Err(ShareError::UnsupportedVersion {
                found: self.schema_version,
                supported: BUNDLE_SCHEMA_VERSION,
            });
        }
        if self.recipes.is_empty() {
            return Err(ShareError::EmptyBundle);
        }
        Ok(())
    }
}

/// One recipe entry within a [`ShareManifest`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManifestRecipe {
    /// Recipe slug (also the `.cook` file stem inside the bundle).
    pub slug: String,
    /// Human-readable title.
    pub title: String,
    /// Bundle-relative path to the `.cook` file, e.g. `recipes/foo.cook`.
    pub cook_file: String,
    /// Deterministic digest of the `.cook` text, for integrity + dedup.
    pub cook_sha: String,
    /// Original attributed source (e.g. cookbook or site name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Original source URL, when the recipe came from the web.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    /// License the author asserts the shared recipe carries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Free-form attribution/credit line preserved across sharing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    /// Bundle-relative photo paths linked to this recipe.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub photos: Vec<String>,
}

/// Provenance to stamp into a recipe's `.cook` frontmatter so origin travels
/// with the file itself — not just the manifest.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Provenance {
    pub source: Option<String>,
    pub source_url: Option<String>,
    pub license: Option<String>,
    pub shared_by: Option<String>,
}

impl Provenance {
    /// Build the provenance recorded for a recipe when a bundle is assembled.
    pub fn for_recipe(
        source: Option<String>,
        source_url: Option<String>,
        license: Option<String>,
        shared_by: Option<String>,
    ) -> Self {
        Self {
            source,
            source_url,
            license,
            shared_by,
        }
    }

    fn is_empty(&self) -> bool {
        self.source.is_none()
            && self.source_url.is_none()
            && self.license.is_none()
            && self.shared_by.is_none()
    }
}

/// Errors raised while reading or planning a share bundle.
#[derive(Debug, thiserror::Error)]
pub enum ShareError {
    #[error(
        "bundle manifest schema version {found} is not supported (this fond reads up to v{supported})"
    )]
    UnsupportedVersion { found: u32, supported: u32 },

    #[error("bundle contains no recipes")]
    EmptyBundle,

    #[error("bundle manifest is invalid: {0}")]
    Manifest(String),
}

/// A deterministic, machine-stable content digest.
///
/// Uses the standard-library hasher with fixed keys, matching fond's existing
/// content-addressed photo storage — stable across runs and machines, and
/// dependency-free. Not cryptographic; it exists for integrity and dedup, not
/// tamper resistance.
pub fn cook_digest(text: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Stable dedup key for a recipe: its source URL when present (the same signal
/// the Paprika/URL importers use), otherwise a digest of its `.cook` text.
///
/// Callers pass the exact text that will be stored so incoming and
/// already-queued drafts hash identically.
pub fn dedup_key(source_url: Option<&str>, cook_text: &str) -> String {
    match source_url.map(str::trim).filter(|s| !s.is_empty()) {
        Some(url) => format!("url:{}", url.to_lowercase()),
        None => format!("sha:{}", cook_digest(cook_text)),
    }
}

/// The outcome of planning a single recipe's import from a bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportDecision {
    /// Enqueue the recipe into the review pipeline.
    Enqueue,
    /// Skip the recipe as a duplicate; carries a human-readable reason.
    Skip(String),
}

/// Decide whether an incoming shared recipe should be enqueued for review or
/// skipped as a duplicate — the idempotency guarantee for re-imports.
///
/// `cook_text` is the (provenance-stamped) text that would be stored. Dedup is
/// by source URL against the existing library and against drafts already
/// waiting in the review queue; URL-less recipes fall back to a content digest.
/// A slug collision alone is **not** a skip — the review `accept` step resolves
/// slugs, so the recipe still goes through the human gate.
pub fn plan_recipe(
    source_url: Option<&str>,
    cook_text: &str,
    existing_library_keys: &HashSet<String>,
    queued_keys: &HashSet<String>,
) -> ImportDecision {
    let key = dedup_key(source_url, cook_text);
    if existing_library_keys.contains(&key) {
        return ImportDecision::Skip("already in your library (same source)".to_string());
    }
    if queued_keys.contains(&key) {
        return ImportDecision::Skip("already waiting in the review queue".to_string());
    }
    ImportDecision::Enqueue
}

/// Stamp provenance into a `.cook` file's frontmatter, losslessly.
///
/// Existing metadata is never clobbered: a key is only written when the recipe
/// does not already carry it. The rest of the file (steps, sections, comments,
/// unknown keys) is preserved via the [`CookDocument`](fond_domain::CookDocument)
/// edit layer, so an unshared→shared→imported round-trip stays faithful.
pub fn stamp_provenance(cook_text: &str, prov: &Provenance) -> String {
    if prov.is_empty() {
        return cook_text.to_string();
    }
    let mut doc = fond_domain::CookDocument::parse(cook_text);

    if doc.get(&["source"]).is_none()
        && let Some(ref s) = prov.source
    {
        doc.set_scalar("source", &["source"], Some(s));
    }
    if doc.get(&["source url", "source_url"]).is_none()
        && let Some(ref s) = prov.source_url
    {
        doc.set_scalar("source url", &["source url", "source_url"], Some(s));
    }
    if doc.get(&["license"]).is_none()
        && let Some(ref s) = prov.license
    {
        doc.set_scalar("license", &["license"], Some(s));
    }
    if doc.get(&["shared by", "shared_by"]).is_none()
        && let Some(ref s) = prov.shared_by
    {
        doc.set_scalar("shared by", &["shared by", "shared_by"], Some(s));
    }

    doc.emit()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cook() -> &'static str {
        "---\ntitle: Chicken Adobo\n---\n\nBrown the @chicken{1%kg}.\n"
    }

    #[test]
    fn digest_is_deterministic_and_stable() {
        assert_eq!(cook_digest("hello"), cook_digest("hello"));
        assert_ne!(cook_digest("hello"), cook_digest("world"));
        // 16 lowercase hex chars (u64), zero-padded.
        let d = cook_digest("fond");
        assert_eq!(d.len(), 16);
        assert!(d.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn dedup_key_prefers_url_then_digest() {
        assert_eq!(
            dedup_key(Some("https://EXAMPLE.com/R"), "x"),
            "url:https://example.com/r"
        );
        assert_eq!(
            dedup_key(Some("   "), "x"),
            format!("sha:{}", cook_digest("x"))
        );
        assert_eq!(dedup_key(None, "x"), format!("sha:{}", cook_digest("x")));
    }

    #[test]
    fn plan_skips_library_duplicate_by_url() {
        let mut lib = HashSet::new();
        lib.insert("url:https://example.com/r".to_string());
        let decision = plan_recipe(Some("https://example.com/r"), "x", &lib, &HashSet::new());
        assert!(matches!(decision, ImportDecision::Skip(_)));
    }

    #[test]
    fn plan_skips_when_already_queued() {
        let mut queued = HashSet::new();
        queued.insert(dedup_key(None, "body"));
        let decision = plan_recipe(None, "body", &HashSet::new(), &queued);
        assert!(matches!(decision, ImportDecision::Skip(_)));
    }

    #[test]
    fn plan_enqueues_novel_recipe() {
        let decision = plan_recipe(
            Some("https://example.com/new"),
            "body",
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(decision, ImportDecision::Enqueue);
    }

    #[test]
    fn stamp_adds_missing_provenance() {
        let prov = Provenance::for_recipe(
            Some("Grandma".to_string()),
            Some("https://example.com/adobo".to_string()),
            Some("CC-BY-4.0".to_string()),
            Some("alice".to_string()),
        );
        let out = stamp_provenance(sample_cook(), &prov);
        assert!(out.contains("source: Grandma"));
        assert!(out.contains("source url: https://example.com/adobo"));
        assert!(out.contains("license: CC-BY-4.0"));
        assert!(out.contains("shared by: alice"));
        // Body preserved.
        assert!(out.contains("@chicken{1%kg}"));
        // Title preserved.
        assert!(out.contains("title: Chicken Adobo"));
    }

    #[test]
    fn stamp_never_clobbers_existing_metadata() {
        let cook = "---\ntitle: X\nsource: Original Author\n---\n\nDo a thing.\n";
        let prov = Provenance::for_recipe(
            Some("Someone Else".to_string()),
            None,
            Some("MIT".to_string()),
            None,
        );
        let out = stamp_provenance(cook, &prov);
        assert!(out.contains("source: Original Author"));
        assert!(!out.contains("Someone Else"));
        assert!(out.contains("license: MIT"));
    }

    #[test]
    fn stamp_is_noop_without_provenance() {
        let out = stamp_provenance(sample_cook(), &Provenance::default());
        assert_eq!(out, sample_cook());
    }

    #[test]
    fn manifest_validate_rejects_bad_version_and_empty() {
        let mut m = ShareManifest {
            schema_version: 999,
            fond_version: "test".to_string(),
            bundle_id: "b".to_string(),
            created_at: "now".to_string(),
            shared_by: None,
            recipes: vec![ManifestRecipe {
                slug: "r".to_string(),
                title: "R".to_string(),
                cook_file: "recipes/r.cook".to_string(),
                cook_sha: "0".to_string(),
                source: None,
                source_url: None,
                license: None,
                attribution: None,
                photos: vec![],
            }],
        };
        assert!(matches!(
            m.validate(),
            Err(ShareError::UnsupportedVersion { .. })
        ));
        m.schema_version = BUNDLE_SCHEMA_VERSION;
        assert!(m.validate().is_ok());
        m.recipes.clear();
        assert!(matches!(m.validate(), Err(ShareError::EmptyBundle)));
    }
}
