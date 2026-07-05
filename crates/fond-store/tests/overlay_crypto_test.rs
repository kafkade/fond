//! Integration tests for the encrypted sealed-bundle overlay codec (issue #103).
//!
//! These exercise the DB → seal → file → open → merge round trip and the
//! fail-closed guarantees, complementing the pure-crypto unit tests in
//! `fond_store::crypto`.

use fond_store::FondDb;
use fond_store::crypto::{KeyMaterial, KeyMode, generate_key};
use fond_store::overlay::{self, ExportOptions};

/// Build an in-memory DB seeded with a recipe, a user, and some authored data.
fn seed_db() -> FondDb {
    let db = FondDb::open_memory().unwrap();
    let conn = db.conn();
    conn.execute(
        "INSERT INTO recipes (slug, title, file_path) VALUES ('adobo', 'Chicken Adobo', 'adobo.cook')",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO users (id, name) VALUES (2, 'alice')", [])
        .unwrap();
    conn.execute(
        "INSERT INTO notes (id, recipe_slug, user_id, body, created_at)
         VALUES ('n1', 'adobo', 2, 'secret vinegar tip', '2026-01-01 10:00:00')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ratings (id, recipe_slug, user_id, score, created_at, updated_at)
         VALUES ('r1', 'adobo', 2, 5, '2026-01-01 10:00:00', '2026-01-02 10:00:00')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO pantry_items (name, present, updated_at)
         VALUES ('garlic', 1, '2026-01-01 10:00:00')",
        [],
    )
    .unwrap();
    db
}

#[test]
fn sealed_keychain_round_trip_converges() {
    let src = seed_db();
    let dir = tempfile::tempdir().unwrap();
    let path = overlay::sealed_bundle_path(dir.path());

    let key = generate_key().unwrap();
    let summary = overlay::export_sealed(
        &src,
        &path,
        &ExportOptions::default(),
        &KeyMaterial::Raw(key),
    )
    .unwrap();
    assert_eq!(summary.notes, 1);
    assert_eq!(summary.ratings, 1);
    assert_eq!(summary.pantry_items, 1);

    // The sealed file exists and its mode is discoverable without the key.
    assert!(path.exists());
    assert_eq!(
        overlay::peek_sealed_mode(dir.path()).unwrap(),
        Some(KeyMode::Keychain)
    );

    // A fresh device imports the sealed bundle with the same key.
    let dst = FondDb::open_memory().unwrap();
    dst.conn()
        .execute(
            "INSERT INTO recipes (slug, title, file_path) VALUES ('adobo', 'Chicken Adobo', 'adobo.cook')",
            [],
        )
        .unwrap();
    let report = overlay::import_sealed(&dst, &path, &KeyMaterial::Raw(key)).unwrap();
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
fn sealed_passphrase_round_trip_converges() {
    let src = seed_db();
    let dir = tempfile::tempdir().unwrap();
    let path = overlay::sealed_bundle_path(dir.path());

    overlay::export_sealed(
        &src,
        &path,
        &ExportOptions::default(),
        &KeyMaterial::Passphrase("correct horse battery".into()),
    )
    .unwrap();

    assert_eq!(
        overlay::peek_sealed_mode(dir.path()).unwrap(),
        Some(KeyMode::Passphrase)
    );

    let dst = FondDb::open_memory().unwrap();
    dst.conn()
        .execute(
            "INSERT INTO recipes (slug, title, file_path) VALUES ('adobo', 'Chicken Adobo', 'adobo.cook')",
            [],
        )
        .unwrap();
    let report = overlay::import_sealed(
        &dst,
        &path,
        &KeyMaterial::Passphrase("correct horse battery".into()),
    )
    .unwrap();
    assert_eq!(report.notes_added, 1);

    let body: String = dst
        .conn()
        .query_row("SELECT body FROM notes WHERE id = 'n1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(body, "secret vinegar tip");
}

#[test]
fn sealed_file_does_not_leak_plaintext() {
    let src = seed_db();
    let dir = tempfile::tempdir().unwrap();
    let path = overlay::sealed_bundle_path(dir.path());
    overlay::export_sealed(
        &src,
        &path,
        &ExportOptions::default(),
        &KeyMaterial::Raw(generate_key().unwrap()),
    )
    .unwrap();

    let bytes = std::fs::read(&path).unwrap();
    let haystack = String::from_utf8_lossy(&bytes);
    assert!(!haystack.contains("secret vinegar tip"));
    assert!(!haystack.contains("garlic"));
    assert!(!haystack.contains("adobo"));
}

#[test]
fn wrong_passphrase_import_fails_closed() {
    let src = seed_db();
    let dir = tempfile::tempdir().unwrap();
    let path = overlay::sealed_bundle_path(dir.path());
    overlay::export_sealed(
        &src,
        &path,
        &ExportOptions::default(),
        &KeyMaterial::Passphrase("right".into()),
    )
    .unwrap();

    let dst = FondDb::open_memory().unwrap();
    dst.conn()
        .execute(
            "INSERT INTO recipes (slug, title, file_path) VALUES ('adobo', 'Chicken Adobo', 'adobo.cook')",
            [],
        )
        .unwrap();

    // Wrong passphrase must error and write nothing.
    let err = overlay::import_sealed(&dst, &path, &KeyMaterial::Passphrase("wrong".into()));
    assert!(err.is_err());

    let notes: i64 = dst
        .conn()
        .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(notes, 0, "no plaintext must be written on a failed decrypt");
}

#[test]
fn peek_mode_none_when_no_bundle() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(overlay::peek_sealed_mode(dir.path()).unwrap(), None);
}
