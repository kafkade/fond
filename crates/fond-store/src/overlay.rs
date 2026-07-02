//! Authored-overlay sidecar export/import (ADR-012 Tier 2, ADR-015).
//!
//! The *authored* slice of the SQLite overlay — notes, ratings, cook logs,
//! pantry, meal plans, and dietary profiles — is genuine sync payload (unlike
//! the derived recipe index, which `fond reindex` rebuilds from `.cook` files).
//! This module serialises that slice to **plain-text JSONL sidecar files** that
//! ride the same user-controlled file-sync channel as recipes, and merges them
//! back on import.
//!
//! ## Layout (under the overlay dir, e.g. `<data_dir>/overlay/`)
//!
//! ```text
//! overlay/
//!   users/<user-slug>/
//!     notes.jsonl        # union merge (append-only), key = id (UUIDv7)
//!     ratings.jsonl      # last-writer-wins, key = recipe_slug
//!     cook-logs.jsonl    # union merge (append-only), key = id (UUIDv7)
//!     profile.jsonl      # union merge of allergen + dietary-pref membership
//!   shared/
//!     pantry.jsonl       # last-writer-wins, key = name (NOCASE)
//!     meal-plans.jsonl   # last-writer-wins per-plan snapshot, key = name
//! ```
//!
//! Each file is one JSON object per line, records sorted deterministically so
//! diffs stay minimal and human-reviewable. The `user` field inside per-user
//! records is the device-stable identity (the folder name is cosmetic); import
//! resolves or creates the local user by name (ADR-005 defers full identity
//! reconciliation, so name is the pragmatic bridge).
//!
//! ## Merge semantics
//!
//! * **Union** (notes, cook logs, profile sets): additive; a record is inserted
//!   only if absent. Deletions do not propagate — a documented, data-safe
//!   limitation consistent with append-only logs.
//! * **Last-writer-wins** (ratings, pantry, meal plans): the side with the newer
//!   `updated_at` wins (ties broken by id/name for determinism). Incoming
//!   timestamps and ids are preserved on apply, so every device converges to the
//!   same winner regardless of import order. Overwrites and skips of *differing*
//!   values are reported as conflicts — never silent data loss.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{FondDb, StoreError};

const USERS_DIR: &str = "users";
const SHARED_DIR: &str = "shared";
const NONE_USER_SLUG: &str = "_none";

// ═══════════════════════════════════════════════════════════════════
// Sidecar record types
// ═══════════════════════════════════════════════════════════════════

/// A note, keyed by its UUIDv7 `id` (union merge).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteSidecar {
    pub id: String,
    pub recipe_slug: String,
    pub user: Option<String>,
    pub body: String,
    pub created_at: String,
}

/// A rating, keyed by `(recipe_slug, user)` (last-writer-wins on `updated_at`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RatingSidecar {
    pub id: String,
    pub recipe_slug: String,
    pub user: Option<String>,
    pub score: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// A cook-log entry, keyed by its UUIDv7 `id` (union merge).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CookLogSidecar {
    pub id: String,
    pub recipe_slug: String,
    pub user: Option<String>,
    pub started_at: String,
    pub finished_at: String,
    pub steps_completed: i32,
    pub total_steps: i32,
    pub notes: String,
    pub created_at: String,
}

/// A per-user dietary profile (allergen + preference set membership; union).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileSidecar {
    pub user: String,
    pub allergens: Vec<String>,
    pub dietary_prefs: Vec<String>,
}

/// A pantry item, keyed by `name` (NOCASE; last-writer-wins on `updated_at`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PantrySidecar {
    pub name: String,
    pub present: bool,
    pub quantity: Option<String>,
    pub unit: Option<String>,
    pub expiry: Option<String>,
    pub par_level: Option<String>,
    pub updated_at: String,
}

/// A single meal-plan entry within a [`MealPlanSidecar`] snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct MealPlanEntrySidecar {
    pub plan_date: String,
    pub meal: String,
    pub recipe_slug: String,
}

/// A meal plan, keyed by `name` (last-writer-wins whole-plan snapshot on
/// `updated_at`; the winning side's entry set replaces the local one).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MealPlanSidecar {
    pub name: String,
    pub start_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub entries: Vec<MealPlanEntrySidecar>,
}

// ═══════════════════════════════════════════════════════════════════
// Export
// ═══════════════════════════════════════════════════════════════════

/// Options controlling an overlay export.
#[derive(Debug, Clone, Default)]
pub struct ExportOptions {
    /// Restrict per-user overlays to this user name (by exact match). `None`
    /// exports every user. Shared overlays (pantry, meal plans) are always
    /// exported.
    pub user: Option<String>,
}

/// Counts of records written by [`export_to_dir`].
#[derive(Debug, Clone, Default, Serialize)]
pub struct ExportSummary {
    pub notes: usize,
    pub ratings: usize,
    pub cook_logs: usize,
    pub profiles: usize,
    pub pantry_items: usize,
    pub meal_plans: usize,
    pub users: usize,
}

/// Export the authored overlay to sidecar files under `dir`.
///
/// Creates `dir` (and its `users/` and `shared/` subtrees) as needed. Files are
/// written atomically (temp-then-rename) and records are sorted deterministically.
pub fn export_to_dir(
    db: &FondDb,
    dir: &Path,
    opts: &ExportOptions,
) -> Result<ExportSummary, StoreError> {
    let conn = db.conn();
    let mut summary = ExportSummary::default();

    // ── Per-user overlays, grouped by user name ────────────────────
    let notes = collect_notes(conn, opts.user.as_deref())?;
    let ratings = collect_ratings(conn, opts.user.as_deref())?;
    let cook_logs = collect_cook_logs(conn, opts.user.as_deref())?;
    let profiles = collect_profiles(conn, opts.user.as_deref())?;

    summary.notes = notes.len();
    summary.ratings = ratings.len();
    summary.cook_logs = cook_logs.len();
    summary.profiles = profiles.len();

    // Group notes/ratings/cook-logs by their (optional) user name.
    let mut notes_by_user: BTreeMap<Option<String>, Vec<NoteSidecar>> = BTreeMap::new();
    for n in notes {
        notes_by_user.entry(n.user.clone()).or_default().push(n);
    }
    let mut ratings_by_user: BTreeMap<Option<String>, Vec<RatingSidecar>> = BTreeMap::new();
    for r in ratings {
        ratings_by_user.entry(r.user.clone()).or_default().push(r);
    }
    let mut logs_by_user: BTreeMap<Option<String>, Vec<CookLogSidecar>> = BTreeMap::new();
    for c in cook_logs {
        logs_by_user.entry(c.user.clone()).or_default().push(c);
    }

    // The set of user buckets that need a directory.
    let mut user_keys: std::collections::BTreeSet<Option<String>> =
        std::collections::BTreeSet::new();
    user_keys.extend(notes_by_user.keys().cloned());
    user_keys.extend(ratings_by_user.keys().cloned());
    user_keys.extend(logs_by_user.keys().cloned());
    user_keys.extend(profiles.iter().map(|p| Some(p.user.clone())));
    summary.users = user_keys.len();

    let users_root = dir.join(USERS_DIR);
    for key in &user_keys {
        let slug = user_slug(key.as_deref());
        let user_dir = users_root.join(&slug);

        write_jsonl(
            &user_dir.join("notes.jsonl"),
            notes_by_user.get(key).map(Vec::as_slice).unwrap_or(&[]),
        )?;
        write_jsonl(
            &user_dir.join("ratings.jsonl"),
            ratings_by_user.get(key).map(Vec::as_slice).unwrap_or(&[]),
        )?;
        write_jsonl(
            &user_dir.join("cook-logs.jsonl"),
            logs_by_user.get(key).map(Vec::as_slice).unwrap_or(&[]),
        )?;

        // Profile only exists for named users.
        let profile: Vec<ProfileSidecar> = key
            .as_deref()
            .and_then(|name| profiles.iter().find(|p| p.user == name).cloned())
            .into_iter()
            .collect();
        write_jsonl(&user_dir.join("profile.jsonl"), &profile)?;
    }

    // ── Shared overlays ────────────────────────────────────────────
    let pantry = collect_pantry(conn)?;
    let meal_plans = collect_meal_plans(conn)?;
    summary.pantry_items = pantry.len();
    summary.meal_plans = meal_plans.len();

    let shared_root = dir.join(SHARED_DIR);
    write_jsonl(&shared_root.join("pantry.jsonl"), &pantry)?;
    write_jsonl(&shared_root.join("meal-plans.jsonl"), &meal_plans)?;

    Ok(summary)
}

fn collect_notes(
    conn: &rusqlite::Connection,
    user: Option<&str>,
) -> Result<Vec<NoteSidecar>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT n.id, n.recipe_slug, u.name, n.body, n.created_at
         FROM notes n LEFT JOIN users u ON u.id = n.user_id
         WHERE (?1 IS NULL OR u.name = ?1)
         ORDER BY n.recipe_slug, n.id",
    )?;
    let rows = stmt
        .query_map(params![user], |row| {
            Ok(NoteSidecar {
                id: row.get(0)?,
                recipe_slug: row.get(1)?,
                user: row.get(2)?,
                body: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn collect_ratings(
    conn: &rusqlite::Connection,
    user: Option<&str>,
) -> Result<Vec<RatingSidecar>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT rt.id, rt.recipe_slug, u.name, rt.score, rt.created_at, rt.updated_at
         FROM ratings rt LEFT JOIN users u ON u.id = rt.user_id
         WHERE (?1 IS NULL OR u.name = ?1)
         ORDER BY rt.recipe_slug, u.name",
    )?;
    let rows = stmt
        .query_map(params![user], |row| {
            Ok(RatingSidecar {
                id: row.get(0)?,
                recipe_slug: row.get(1)?,
                user: row.get(2)?,
                score: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn collect_cook_logs(
    conn: &rusqlite::Connection,
    user: Option<&str>,
) -> Result<Vec<CookLogSidecar>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.recipe_slug, u.name, c.started_at, c.finished_at,
                c.steps_completed, c.total_steps, c.notes, c.created_at
         FROM cook_logs c LEFT JOIN users u ON u.id = c.user_id
         WHERE (?1 IS NULL OR u.name = ?1)
         ORDER BY c.started_at, c.id",
    )?;
    let rows = stmt
        .query_map(params![user], |row| {
            Ok(CookLogSidecar {
                id: row.get(0)?,
                recipe_slug: row.get(1)?,
                user: row.get(2)?,
                started_at: row.get(3)?,
                finished_at: row.get(4)?,
                steps_completed: row.get(5)?,
                total_steps: row.get(6)?,
                notes: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn collect_profiles(
    conn: &rusqlite::Connection,
    user: Option<&str>,
) -> Result<Vec<ProfileSidecar>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, name FROM users
         WHERE (?1 IS NULL OR name = ?1)
         ORDER BY name",
    )?;
    let users: Vec<(i64, String)> = stmt
        .query_map(params![user], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut profiles = Vec::new();
    for (id, name) in users {
        let allergens = string_column(
            conn,
            "SELECT allergen FROM user_allergens WHERE user_id = ?1 ORDER BY allergen",
            id,
        )?;
        let prefs = string_column(
            conn,
            "SELECT pref FROM user_dietary_prefs WHERE user_id = ?1 ORDER BY pref",
            id,
        )?;
        // Skip users with no authored profile data to keep sidecars minimal.
        if allergens.is_empty() && prefs.is_empty() {
            continue;
        }
        profiles.push(ProfileSidecar {
            user: name,
            allergens,
            dietary_prefs: prefs,
        });
    }
    Ok(profiles)
}

fn collect_pantry(conn: &rusqlite::Connection) -> Result<Vec<PantrySidecar>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT name, present, quantity, unit, expiry, par_level, updated_at
         FROM pantry_items ORDER BY name COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(PantrySidecar {
                name: row.get(0)?,
                present: row.get::<_, i32>(1)? != 0,
                quantity: row.get(2)?,
                unit: row.get(3)?,
                expiry: row.get(4)?,
                par_level: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn collect_meal_plans(conn: &rusqlite::Connection) -> Result<Vec<MealPlanSidecar>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, start_date, created_at, updated_at
         FROM meal_plans ORDER BY name",
    )?;
    let plans: Vec<(i64, String, Option<String>, String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut result = Vec::new();
    for (id, name, start_date, created_at, updated_at) in plans {
        let mut entry_stmt = conn.prepare(
            "SELECT plan_date, meal, recipe_slug FROM meal_plan_entries
             WHERE meal_plan_id = ?1
             ORDER BY plan_date, meal, recipe_slug",
        )?;
        let entries = entry_stmt
            .query_map(params![id], |row| {
                Ok(MealPlanEntrySidecar {
                    plan_date: row.get(0)?,
                    meal: row.get(1)?,
                    recipe_slug: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        result.push(MealPlanSidecar {
            name,
            start_date,
            created_at,
            updated_at,
            entries,
        });
    }
    Ok(result)
}

fn string_column(
    conn: &rusqlite::Connection,
    sql: &str,
    user_id: i64,
) -> Result<Vec<String>, StoreError> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map(params![user_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Count the local authored-overlay rows without writing any files.
///
/// Used by `fond overlay status` to show what would be exported.
pub fn local_summary(db: &FondDb) -> Result<ExportSummary, StoreError> {
    let conn = db.conn();
    Ok(ExportSummary {
        notes: collect_notes(conn, None)?.len(),
        ratings: collect_ratings(conn, None)?.len(),
        cook_logs: collect_cook_logs(conn, None)?.len(),
        profiles: collect_profiles(conn, None)?.len(),
        pantry_items: collect_pantry(conn)?.len(),
        meal_plans: collect_meal_plans(conn)?.len(),
        users: count_users(conn)?,
    })
}

fn count_users(conn: &rusqlite::Connection) -> Result<usize, StoreError> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
    Ok(n as usize)
}

// ═══════════════════════════════════════════════════════════════════
// Import + merge
// ═══════════════════════════════════════════════════════════════════

/// A rating whose value differed between local and incoming state.
#[derive(Debug, Clone, Serialize)]
pub struct RatingConflict {
    pub recipe_slug: String,
    pub user: Option<String>,
    pub local_score: i32,
    pub incoming_score: i32,
    /// Which side won the last-writer-wins comparison.
    pub winner: Side,
}

/// A pantry item whose state differed between local and incoming.
#[derive(Debug, Clone, Serialize)]
pub struct PantryConflict {
    pub name: String,
    pub local_present: bool,
    pub incoming_present: bool,
    pub winner: Side,
}

/// A meal plan whose entry set differed between local and incoming.
#[derive(Debug, Clone, Serialize)]
pub struct MealPlanConflict {
    pub name: String,
    pub winner: Side,
}

/// The winning side of a last-writer-wins conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// The value already in the local database was kept.
    Local,
    /// The value from the imported sidecar was applied.
    Incoming,
}

/// Outcome of an overlay import — what was applied, skipped, and conflicted.
///
/// Surfaced to the user so last-writer-wins overwrites are never silent.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MergeReport {
    pub users_created: usize,

    pub notes_added: usize,
    pub notes_skipped: usize,

    pub ratings_applied: usize,
    pub ratings_skipped: usize,
    pub rating_conflicts: Vec<RatingConflict>,

    pub cook_logs_added: usize,
    pub cook_logs_skipped: usize,

    pub pantry_applied: usize,
    pub pantry_skipped: usize,
    pub pantry_conflicts: Vec<PantryConflict>,

    pub meal_plans_applied: usize,
    pub meal_plans_skipped: usize,
    pub meal_plan_conflicts: Vec<MealPlanConflict>,

    pub profile_allergens_added: usize,
    pub profile_prefs_added: usize,

    /// Lines that could not be parsed and were skipped (never applied).
    pub malformed_lines: usize,
}

impl MergeReport {
    /// Total number of records applied (inserted or updated) across all overlays.
    pub fn applied_total(&self) -> usize {
        self.notes_added
            + self.ratings_applied
            + self.cook_logs_added
            + self.pantry_applied
            + self.meal_plans_applied
            + self.profile_allergens_added
            + self.profile_prefs_added
    }

    /// Total number of reported conflicts across all last-writer-wins overlays.
    pub fn conflict_total(&self) -> usize {
        self.rating_conflicts.len() + self.pantry_conflicts.len() + self.meal_plan_conflicts.len()
    }
}

/// Import authored overlays from sidecar files under `dir`, merging into the DB.
///
/// Returns a [`MergeReport`]. Missing files/directories are treated as empty
/// (a fresh device with no sidecars imports nothing). The whole import runs in a
/// single transaction: it either fully applies or leaves the DB untouched.
pub fn import_from_dir(db: &FondDb, dir: &Path) -> Result<MergeReport, StoreError> {
    let mut report = MergeReport::default();
    if !dir.exists() {
        return Ok(report);
    }

    // ── Read all sidecar records up front (outside the write txn) ──
    let mut notes: Vec<NoteSidecar> = Vec::new();
    let mut ratings: Vec<RatingSidecar> = Vec::new();
    let mut cook_logs: Vec<CookLogSidecar> = Vec::new();
    let mut profiles: Vec<ProfileSidecar> = Vec::new();

    let users_root = dir.join(USERS_DIR);
    if users_root.is_dir() {
        for entry in std::fs::read_dir(&users_root)? {
            let user_dir = entry?.path();
            if !user_dir.is_dir() {
                continue;
            }
            notes.extend(read_jsonl(&user_dir.join("notes.jsonl"), &mut report)?);
            ratings.extend(read_jsonl(&user_dir.join("ratings.jsonl"), &mut report)?);
            cook_logs.extend(read_jsonl(&user_dir.join("cook-logs.jsonl"), &mut report)?);
            profiles.extend(read_jsonl(&user_dir.join("profile.jsonl"), &mut report)?);
        }
    }

    let shared_root = dir.join(SHARED_DIR);
    let pantry: Vec<PantrySidecar> = read_jsonl(&shared_root.join("pantry.jsonl"), &mut report)?;
    let meal_plans: Vec<MealPlanSidecar> =
        read_jsonl(&shared_root.join("meal-plans.jsonl"), &mut report)?;

    // ── Apply everything atomically ────────────────────────────────
    let conn = db.conn();
    let tx = conn.unchecked_transaction()?;

    for note in &notes {
        merge_note(&tx, note, &mut report)?;
    }
    for rating in &ratings {
        merge_rating(&tx, rating, &mut report)?;
    }
    for log in &cook_logs {
        merge_cook_log(&tx, log, &mut report)?;
    }
    for profile in &profiles {
        merge_profile(&tx, profile, &mut report)?;
    }
    for item in &pantry {
        merge_pantry(&tx, item, &mut report)?;
    }
    for plan in &meal_plans {
        merge_meal_plan(&tx, plan, &mut report)?;
    }

    tx.commit()?;
    Ok(report)
}

/// Resolve a user name to a local `user_id`, creating the user if needed.
///
/// `None` (unscoped rows) resolves to `None` (NULL `user_id`).
fn resolve_user_id(
    tx: &rusqlite::Connection,
    name: Option<&str>,
    report: &mut MergeReport,
) -> Result<Option<i64>, StoreError> {
    let Some(name) = name else {
        return Ok(None);
    };
    let existing: Option<i64> = tx
        .query_row("SELECT id FROM users WHERE name = ?1", params![name], |r| {
            r.get(0)
        })
        .optional()?;
    if let Some(id) = existing {
        return Ok(Some(id));
    }
    tx.execute("INSERT INTO users (name) VALUES (?1)", params![name])?;
    report.users_created += 1;
    Ok(Some(tx.last_insert_rowid()))
}

fn merge_note(
    tx: &rusqlite::Connection,
    note: &NoteSidecar,
    report: &mut MergeReport,
) -> Result<(), StoreError> {
    let exists: bool = tx
        .query_row(
            "SELECT 1 FROM notes WHERE id = ?1",
            params![note.id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if exists {
        report.notes_skipped += 1;
        return Ok(());
    }
    let user_id = resolve_user_id(tx, note.user.as_deref(), report)?;
    tx.execute(
        "INSERT INTO notes (id, recipe_slug, user_id, body, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            note.id,
            note.recipe_slug,
            user_id,
            note.body,
            note.created_at
        ],
    )?;
    report.notes_added += 1;
    Ok(())
}

fn merge_rating(
    tx: &rusqlite::Connection,
    rating: &RatingSidecar,
    report: &mut MergeReport,
) -> Result<(), StoreError> {
    let user_id = resolve_user_id(tx, rating.user.as_deref(), report)?;

    let local: Option<(String, i32, String)> = tx
        .query_row(
            "SELECT id, score, updated_at FROM ratings
             WHERE recipe_slug = ?1 AND user_id IS ?2",
            params![rating.recipe_slug, user_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;

    match local {
        None => {
            tx.execute(
                "INSERT INTO ratings (id, recipe_slug, user_id, score, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    rating.id,
                    rating.recipe_slug,
                    user_id,
                    rating.score,
                    rating.created_at,
                    rating.updated_at,
                ],
            )?;
            report.ratings_applied += 1;
        }
        Some((local_id, local_score, local_updated)) => {
            let incoming_wins = (rating.updated_at.as_str(), rating.id.as_str())
                > (local_updated.as_str(), local_id.as_str());
            let differs = local_score != rating.score;

            if incoming_wins {
                tx.execute(
                    "UPDATE ratings
                     SET id = ?1, score = ?2, created_at = ?3, updated_at = ?4
                     WHERE recipe_slug = ?5 AND user_id IS ?6",
                    params![
                        rating.id,
                        rating.score,
                        rating.created_at,
                        rating.updated_at,
                        rating.recipe_slug,
                        user_id,
                    ],
                )?;
                report.ratings_applied += 1;
                if differs {
                    report.rating_conflicts.push(RatingConflict {
                        recipe_slug: rating.recipe_slug.clone(),
                        user: rating.user.clone(),
                        local_score,
                        incoming_score: rating.score,
                        winner: Side::Incoming,
                    });
                }
            } else {
                report.ratings_skipped += 1;
                if differs {
                    report.rating_conflicts.push(RatingConflict {
                        recipe_slug: rating.recipe_slug.clone(),
                        user: rating.user.clone(),
                        local_score,
                        incoming_score: rating.score,
                        winner: Side::Local,
                    });
                }
            }
        }
    }
    Ok(())
}

fn merge_cook_log(
    tx: &rusqlite::Connection,
    log: &CookLogSidecar,
    report: &mut MergeReport,
) -> Result<(), StoreError> {
    let exists: bool = tx
        .query_row(
            "SELECT 1 FROM cook_logs WHERE id = ?1",
            params![log.id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if exists {
        report.cook_logs_skipped += 1;
        return Ok(());
    }
    let user_id = resolve_user_id(tx, log.user.as_deref(), report)?;
    tx.execute(
        "INSERT INTO cook_logs
           (id, recipe_slug, user_id, started_at, finished_at,
            steps_completed, total_steps, notes, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            log.id,
            log.recipe_slug,
            user_id,
            log.started_at,
            log.finished_at,
            log.steps_completed,
            log.total_steps,
            log.notes,
            log.created_at,
        ],
    )?;
    report.cook_logs_added += 1;
    Ok(())
}

fn merge_profile(
    tx: &rusqlite::Connection,
    profile: &ProfileSidecar,
    report: &mut MergeReport,
) -> Result<(), StoreError> {
    let Some(user_id) = resolve_user_id(tx, Some(&profile.user), report)? else {
        return Ok(());
    };
    for allergen in &profile.allergens {
        let n = tx.execute(
            "INSERT OR IGNORE INTO user_allergens (user_id, allergen) VALUES (?1, ?2)",
            params![user_id, allergen],
        )?;
        report.profile_allergens_added += n;
    }
    for pref in &profile.dietary_prefs {
        let n = tx.execute(
            "INSERT OR IGNORE INTO user_dietary_prefs (user_id, pref) VALUES (?1, ?2)",
            params![user_id, pref],
        )?;
        report.profile_prefs_added += n;
    }
    Ok(())
}

fn merge_pantry(
    tx: &rusqlite::Connection,
    item: &PantrySidecar,
    report: &mut MergeReport,
) -> Result<(), StoreError> {
    let local: Option<(i32, String)> = tx
        .query_row(
            "SELECT present, updated_at FROM pantry_items WHERE name = ?1 COLLATE NOCASE",
            params![item.name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    let present_int = i32::from(item.present);

    match local {
        None => {
            tx.execute(
                "INSERT INTO pantry_items
                   (name, present, quantity, unit, expiry, par_level, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    item.name,
                    present_int,
                    item.quantity,
                    item.unit,
                    item.expiry,
                    item.par_level,
                    item.updated_at,
                ],
            )?;
            report.pantry_applied += 1;
        }
        Some((local_present, local_updated)) => {
            let incoming_wins = item.updated_at.as_str() > local_updated.as_str();
            let differs = (local_present != 0) != item.present;

            if incoming_wins {
                tx.execute(
                    "UPDATE pantry_items
                     SET present = ?1, quantity = ?2, unit = ?3, expiry = ?4,
                         par_level = ?5, updated_at = ?6
                     WHERE name = ?7 COLLATE NOCASE",
                    params![
                        present_int,
                        item.quantity,
                        item.unit,
                        item.expiry,
                        item.par_level,
                        item.updated_at,
                        item.name,
                    ],
                )?;
                report.pantry_applied += 1;
                if differs {
                    report.pantry_conflicts.push(PantryConflict {
                        name: item.name.clone(),
                        local_present: local_present != 0,
                        incoming_present: item.present,
                        winner: Side::Incoming,
                    });
                }
            } else {
                report.pantry_skipped += 1;
                if differs {
                    report.pantry_conflicts.push(PantryConflict {
                        name: item.name.clone(),
                        local_present: local_present != 0,
                        incoming_present: item.present,
                        winner: Side::Local,
                    });
                }
            }
        }
    }
    Ok(())
}

fn merge_meal_plan(
    tx: &rusqlite::Connection,
    plan: &MealPlanSidecar,
    report: &mut MergeReport,
) -> Result<(), StoreError> {
    let local: Option<(i64, String)> = tx
        .query_row(
            "SELECT id, updated_at FROM meal_plans WHERE LOWER(name) = LOWER(?1)",
            params![plan.name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    match local {
        None => {
            tx.execute(
                "INSERT INTO meal_plans (name, start_date, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![plan.name, plan.start_date, plan.created_at, plan.updated_at],
            )?;
            let plan_id = tx.last_insert_rowid();
            insert_plan_entries(tx, plan_id, &plan.entries)?;
            report.meal_plans_applied += 1;
        }
        Some((plan_id, local_updated)) => {
            let incoming_wins = plan.updated_at.as_str() > local_updated.as_str();
            let differs = local_entries_differ(tx, plan_id, &plan.entries)?;

            if incoming_wins {
                tx.execute(
                    "UPDATE meal_plans SET start_date = ?1, updated_at = ?2 WHERE id = ?3",
                    params![plan.start_date, plan.updated_at, plan_id],
                )?;
                tx.execute(
                    "DELETE FROM meal_plan_entries WHERE meal_plan_id = ?1",
                    params![plan_id],
                )?;
                insert_plan_entries(tx, plan_id, &plan.entries)?;
                report.meal_plans_applied += 1;
                if differs {
                    report.meal_plan_conflicts.push(MealPlanConflict {
                        name: plan.name.clone(),
                        winner: Side::Incoming,
                    });
                }
            } else {
                report.meal_plans_skipped += 1;
                if differs {
                    report.meal_plan_conflicts.push(MealPlanConflict {
                        name: plan.name.clone(),
                        winner: Side::Local,
                    });
                }
            }
        }
    }
    Ok(())
}

fn insert_plan_entries(
    tx: &rusqlite::Connection,
    plan_id: i64,
    entries: &[MealPlanEntrySidecar],
) -> Result<(), StoreError> {
    for e in entries {
        tx.execute(
            "INSERT OR IGNORE INTO meal_plan_entries (meal_plan_id, plan_date, meal, recipe_slug)
             VALUES (?1, ?2, ?3, ?4)",
            params![plan_id, e.plan_date, e.meal, e.recipe_slug],
        )?;
    }
    Ok(())
}

fn local_entries_differ(
    tx: &rusqlite::Connection,
    plan_id: i64,
    incoming: &[MealPlanEntrySidecar],
) -> Result<bool, StoreError> {
    let mut stmt = tx.prepare(
        "SELECT plan_date, meal, recipe_slug FROM meal_plan_entries
         WHERE meal_plan_id = ?1
         ORDER BY plan_date, meal, recipe_slug",
    )?;
    let mut local: Vec<MealPlanEntrySidecar> = stmt
        .query_map(params![plan_id], |row| {
            Ok(MealPlanEntrySidecar {
                plan_date: row.get(0)?,
                meal: row.get(1)?,
                recipe_slug: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    local.sort();

    let mut sorted_incoming = incoming.to_vec();
    sorted_incoming.sort();

    Ok(local != sorted_incoming)
}

// ═══════════════════════════════════════════════════════════════════
// JSONL file helpers
// ═══════════════════════════════════════════════════════════════════

/// Slugify a user name for use as a directory name. Cosmetic only — the `user`
/// field inside each record is the authoritative identity on import.
fn user_slug(name: Option<&str>) -> String {
    let Some(name) = name else {
        return NONE_USER_SLUG.to_string();
    };
    let mut slug = String::new();
    let mut prev_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        NONE_USER_SLUG.to_string()
    } else {
        slug
    }
}

/// Write records as JSONL to `path`, atomically (temp-then-rename).
fn write_jsonl<T: Serialize>(path: &Path, records: &[T]) -> Result<(), StoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut buf = String::new();
    for rec in records {
        let line = serde_json::to_string(rec).map_err(|e| StoreError::Database {
            message: format!("failed to serialize sidecar record: {e}"),
        })?;
        buf.push_str(&line);
        buf.push('\n');
    }

    let tmp = tmp_path(path);
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(buf.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Read JSONL records from `path`. Missing files yield an empty vec; malformed
/// lines are skipped and counted in `report.malformed_lines`.
fn read_jsonl<T: for<'de> Deserialize<'de>>(
    path: &Path,
    report: &mut MergeReport,
) -> Result<Vec<T>, StoreError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<T>(line) {
            Ok(rec) => out.push(rec),
            Err(_) => report.malformed_lines += 1,
        }
    }
    Ok(out)
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}
