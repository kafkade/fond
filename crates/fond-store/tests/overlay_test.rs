//! Integration tests for authored-overlay sidecar export/import (ADR-012 Tier 2).

use std::path::Path;

use fond_store::FondDb;
use fond_store::overlay::{
    self, CookLogSidecar, ExportOptions, MealPlanEntrySidecar, MealPlanSidecar, NoteSidecar,
    PantrySidecar, ProfileSidecar, RatingSidecar, Side,
};

/// Build an in-memory DB seeded with a couple of recipes and a named user.
fn seed_db() -> FondDb {
    let db = FondDb::open_memory().unwrap();
    let conn = db.conn();
    conn.execute(
        "INSERT INTO recipes (slug, title, file_path) VALUES
           ('adobo', 'Chicken Adobo', 'adobo.cook'),
           ('soup', 'Tomato Soup', 'soup.cook')",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO users (id, name) VALUES (2, 'alice')", [])
        .unwrap();
    db
}

/// Write a single-record JSONL sidecar file at `dir/rel`.
fn write_sidecar<T: serde::Serialize>(dir: &Path, rel: &str, records: &[T]) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut buf = String::new();
    for r in records {
        buf.push_str(&serde_json::to_string(r).unwrap());
        buf.push('\n');
    }
    std::fs::write(path, buf).unwrap();
}

#[test]
fn round_trip_export_import_converges() {
    let src = seed_db();
    {
        let conn = src.conn();
        conn.execute(
            "INSERT INTO notes (id, recipe_slug, user_id, body, created_at)
             VALUES ('n1', 'adobo', 1, 'great', '2026-01-01 10:00:00')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ratings (id, recipe_slug, user_id, score, created_at, updated_at)
             VALUES ('r1', 'adobo', 1, 5, '2026-01-01 10:00:00', '2026-01-02 10:00:00')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO pantry_items (name, present, updated_at)
             VALUES ('garlic', 1, '2026-01-01 10:00:00')",
            [],
        )
        .unwrap();
    }

    let dir = tempfile::tempdir().unwrap();
    let summary = overlay::export_to_dir(&src, dir.path(), &ExportOptions::default()).unwrap();
    assert_eq!(summary.notes, 1);
    assert_eq!(summary.ratings, 1);
    assert_eq!(summary.pantry_items, 1);

    // Fresh device imports the sidecars.
    let dst = seed_db();
    let report = overlay::import_from_dir(&dst, dir.path()).unwrap();
    assert_eq!(report.notes_added, 1);
    assert_eq!(report.ratings_applied, 1);
    assert_eq!(report.pantry_applied, 1);
    assert_eq!(report.conflict_total(), 0);

    let score: i32 = dst
        .conn()
        .query_row(
            "SELECT score FROM ratings WHERE recipe_slug = 'adobo'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(score, 5);
}

#[test]
fn notes_union_merge_is_idempotent() {
    let db = seed_db();
    let dir = tempfile::tempdir().unwrap();
    let note = NoteSidecar {
        id: "n-xyz".into(),
        recipe_slug: "adobo".into(),
        user: Some("alice".into()),
        body: "yum".into(),
        created_at: "2026-01-01 10:00:00".into(),
    };
    write_sidecar(dir.path(), "users/alice/notes.jsonl", &[note]);

    let first = overlay::import_from_dir(&db, dir.path()).unwrap();
    assert_eq!(first.notes_added, 1);
    assert_eq!(first.notes_skipped, 0);

    // Re-import: the same id must be recognised and skipped, not duplicated.
    let second = overlay::import_from_dir(&db, dir.path()).unwrap();
    assert_eq!(second.notes_added, 0);
    assert_eq!(second.notes_skipped, 1);

    let count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn ratings_lww_newer_incoming_overwrites_and_reports_conflict() {
    let db = seed_db();
    db.conn()
        .execute(
            "INSERT INTO ratings (id, recipe_slug, user_id, score, created_at, updated_at)
             VALUES ('local', 'adobo', 1, 2, '2026-01-01 10:00:00', '2026-01-01 10:00:00')",
            [],
        )
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let incoming = RatingSidecar {
        id: "incoming".into(),
        recipe_slug: "adobo".into(),
        user: Some("default".into()),
        score: 5,
        created_at: "2026-01-01 10:00:00".into(),
        updated_at: "2026-02-01 10:00:00".into(), // newer
    };
    write_sidecar(dir.path(), "users/default/ratings.jsonl", &[incoming]);

    let report = overlay::import_from_dir(&db, dir.path()).unwrap();
    assert_eq!(report.ratings_applied, 1);
    assert_eq!(report.rating_conflicts.len(), 1);
    assert_eq!(report.rating_conflicts[0].winner, Side::Incoming);

    let score: i32 = db
        .conn()
        .query_row(
            "SELECT score FROM ratings WHERE recipe_slug = 'adobo'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(score, 5);
}

#[test]
fn ratings_lww_older_incoming_is_skipped_but_reported() {
    let db = seed_db();
    db.conn()
        .execute(
            "INSERT INTO ratings (id, recipe_slug, user_id, score, created_at, updated_at)
             VALUES ('local', 'adobo', 1, 4, '2026-01-01 10:00:00', '2026-03-01 10:00:00')",
            [],
        )
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let incoming = RatingSidecar {
        id: "incoming".into(),
        recipe_slug: "adobo".into(),
        user: Some("default".into()),
        score: 1,
        created_at: "2026-01-01 10:00:00".into(),
        updated_at: "2026-02-01 10:00:00".into(), // older
    };
    write_sidecar(dir.path(), "users/default/ratings.jsonl", &[incoming]);

    let report = overlay::import_from_dir(&db, dir.path()).unwrap();
    assert_eq!(report.ratings_applied, 0);
    assert_eq!(report.ratings_skipped, 1);
    assert_eq!(report.rating_conflicts.len(), 1);
    assert_eq!(report.rating_conflicts[0].winner, Side::Local);

    let score: i32 = db
        .conn()
        .query_row(
            "SELECT score FROM ratings WHERE recipe_slug = 'adobo'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(score, 4);
}

#[test]
fn pantry_lww_by_updated_at() {
    let db = seed_db();
    db.conn()
        .execute(
            "INSERT INTO pantry_items (name, present, updated_at)
             VALUES ('flour', 0, '2026-01-01 10:00:00')",
            [],
        )
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let incoming = PantrySidecar {
        name: "flour".into(),
        present: true,
        quantity: Some("2".into()),
        unit: Some("kg".into()),
        expiry: None,
        par_level: None,
        updated_at: "2026-02-01 10:00:00".into(),
    };
    write_sidecar(dir.path(), "shared/pantry.jsonl", &[incoming]);

    let report = overlay::import_from_dir(&db, dir.path()).unwrap();
    assert_eq!(report.pantry_applied, 1);
    assert_eq!(report.pantry_conflicts.len(), 1);
    assert_eq!(report.pantry_conflicts[0].winner, Side::Incoming);

    let (present, qty): (i32, Option<String>) = db
        .conn()
        .query_row(
            "SELECT present, quantity FROM pantry_items WHERE name = 'flour'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(present, 1);
    assert_eq!(qty.as_deref(), Some("2"));
}

#[test]
fn meal_plan_lww_replaces_entry_set_including_deletions() {
    let db = seed_db();
    {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO meal_plans (id, name, updated_at)
             VALUES (1, 'week', '2026-01-01 10:00:00')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO meal_plan_entries (meal_plan_id, plan_date, meal, recipe_slug)
             VALUES (1, '2026-01-05', 'dinner', 'adobo'),
                    (1, '2026-01-06', 'dinner', 'soup')",
            [],
        )
        .unwrap();
    }

    // Incoming (newer) drops 'soup' and keeps only 'adobo'.
    let dir = tempfile::tempdir().unwrap();
    let plan = MealPlanSidecar {
        name: "week".into(),
        start_date: None,
        created_at: "2026-01-01 10:00:00".into(),
        updated_at: "2026-02-01 10:00:00".into(),
        entries: vec![MealPlanEntrySidecar {
            plan_date: "2026-01-05".into(),
            meal: "dinner".into(),
            recipe_slug: "adobo".into(),
        }],
    };
    write_sidecar(dir.path(), "shared/meal-plans.jsonl", &[plan]);

    let report = overlay::import_from_dir(&db, dir.path()).unwrap();
    assert_eq!(report.meal_plans_applied, 1);
    assert_eq!(report.meal_plan_conflicts.len(), 1);
    assert_eq!(report.meal_plan_conflicts[0].winner, Side::Incoming);

    let slugs: Vec<String> = {
        let conn = db.conn();
        let mut stmt = conn
            .prepare("SELECT recipe_slug FROM meal_plan_entries ORDER BY recipe_slug")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(slugs, vec!["adobo".to_string()]);
}

#[test]
fn profile_sets_union_merge() {
    let db = seed_db();
    db.conn()
        .execute(
            "INSERT INTO user_allergens (user_id, allergen) VALUES (2, 'peanut')",
            [],
        )
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let profile = ProfileSidecar {
        user: "alice".into(),
        allergens: vec!["peanut".into(), "shellfish".into()],
        dietary_prefs: vec!["vegetarian".into()],
    };
    write_sidecar(dir.path(), "users/alice/profile.jsonl", &[profile]);

    let report = overlay::import_from_dir(&db, dir.path()).unwrap();
    // 'peanut' already present → only 'shellfish' + the pref are new.
    assert_eq!(report.profile_allergens_added, 1);
    assert_eq!(report.profile_prefs_added, 1);

    let allergens: Vec<String> = {
        let conn = db.conn();
        let mut stmt = conn
            .prepare("SELECT allergen FROM user_allergens WHERE user_id = 2 ORDER BY allergen")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(
        allergens,
        vec!["peanut".to_string(), "shellfish".to_string()]
    );
}

#[test]
fn import_creates_missing_user_by_name() {
    let db = seed_db();
    let dir = tempfile::tempdir().unwrap();
    let note = NoteSidecar {
        id: "n1".into(),
        recipe_slug: "adobo".into(),
        user: Some("bob".into()), // not present in seed_db
        body: "hi".into(),
        created_at: "2026-01-01 10:00:00".into(),
    };
    write_sidecar(dir.path(), "users/bob/notes.jsonl", &[note]);

    let report = overlay::import_from_dir(&db, dir.path()).unwrap();
    assert_eq!(report.users_created, 1);

    let exists: bool = db
        .conn()
        .query_row("SELECT 1 FROM users WHERE name = 'bob'", [], |_| Ok(()))
        .is_ok();
    assert!(exists);
}

#[test]
fn cook_logs_union_merge() {
    let db = seed_db();
    let dir = tempfile::tempdir().unwrap();
    let log = CookLogSidecar {
        id: "c1".into(),
        recipe_slug: "adobo".into(),
        user: Some("alice".into()),
        started_at: "2026-01-01 17:00:00".into(),
        finished_at: "2026-01-01 18:00:00".into(),
        steps_completed: 5,
        total_steps: 5,
        notes: "done".into(),
        created_at: "2026-01-01 18:00:00".into(),
    };
    write_sidecar(dir.path(), "users/alice/cook-logs.jsonl", &[log]);

    let first = overlay::import_from_dir(&db, dir.path()).unwrap();
    assert_eq!(first.cook_logs_added, 1);
    let second = overlay::import_from_dir(&db, dir.path()).unwrap();
    assert_eq!(second.cook_logs_added, 0);
    assert_eq!(second.cook_logs_skipped, 1);
}

#[test]
fn two_device_convergence_matches_acceptance() {
    // Two devices independently rate the same recipe; after each imports the
    // other's sidecar, both converge to the same winner with conflict reported.
    let dev_a = seed_db();
    let dev_b = seed_db();

    dev_a
        .conn()
        .execute(
            "INSERT INTO ratings (id, recipe_slug, user_id, score, created_at, updated_at)
             VALUES ('a', 'adobo', 1, 5, '2026-01-01 10:00:00', '2026-01-01 10:00:00')",
            [],
        )
        .unwrap();
    dev_b
        .conn()
        .execute(
            "INSERT INTO ratings (id, recipe_slug, user_id, score, created_at, updated_at)
             VALUES ('b', 'adobo', 1, 2, '2026-01-01 10:00:00', '2026-02-01 10:00:00')",
            [],
        )
        .unwrap();

    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    overlay::export_to_dir(&dev_a, dir_a.path(), &ExportOptions::default()).unwrap();
    overlay::export_to_dir(&dev_b, dir_b.path(), &ExportOptions::default()).unwrap();

    // Cross-import.
    let report_a = overlay::import_from_dir(&dev_a, dir_b.path()).unwrap();
    let report_b = overlay::import_from_dir(&dev_b, dir_a.path()).unwrap();

    // B's rating (updated_at newer) wins on both devices.
    let score_a: i32 = dev_a
        .conn()
        .query_row(
            "SELECT score FROM ratings WHERE recipe_slug='adobo'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let score_b: i32 = dev_b
        .conn()
        .query_row(
            "SELECT score FROM ratings WHERE recipe_slug='adobo'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(score_a, 2);
    assert_eq!(score_b, 2);

    // Both sides observed a conflict (never a silent overwrite).
    assert!(report_a.conflict_total() >= 1 || report_b.conflict_total() >= 1);
}
