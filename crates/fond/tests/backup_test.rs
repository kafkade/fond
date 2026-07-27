//! Integration tests for `fond backup` (issue #113, ADR-023).
//!
//! Each test uses temp directories and `FOND_DATA_DIR` so it never touches the
//! user's real data. Archives are written to a separate temp dir so they are
//! never re-captured by a subsequent backup.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

const CHICKEN_COOK: &str = "\
---
title: Chicken Adobo
servings: 4
---

Combine @soy sauce{1/2 cup} and @vinegar{1/2 cup} in a bowl.

Cook @chicken thighs{2 lbs} for ~{45 minutes}.
";

const RICE_COOK: &str = "\
---
title: Steamed Rice
servings: 4
---

Rinse @rice{2 cups} and steam for ~{20 minutes}.
";

const PHOTO_REL: &str = "photos/a1/b2c3d4.jpg";
const PHOTO_BYTES: &[u8] = &[
    0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x99, 0x42,
];

/// A `fond` command pointed at `dir` as its data directory.
fn fond_at(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("fond").unwrap();
    cmd.env("FOND_DATA_DIR", dir);
    cmd
}

fn write_recipe(dir: &Path, name: &str, content: &str) {
    let recipes = dir.join("recipes");
    fs::create_dir_all(&recipes).unwrap();
    fs::write(recipes.join(name), content).unwrap();
}

fn write_photo(dir: &Path) {
    let path = dir.join(PHOTO_REL);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, PHOTO_BYTES).unwrap();
}

/// Seed a source data dir with two recipes and a photo blob, indexed.
fn seed_source(dir: &Path) {
    fond_at(dir).arg("init").assert().success();
    write_recipe(dir, "chicken-adobo.cook", CHICKEN_COOK);
    write_recipe(dir, "steamed-rice.cook", RICE_COOK);
    write_photo(dir);
    fond_at(dir).arg("reindex").assert().success();
}

// ──────────────────────────────────────────────────────────────
// create → restore round-trip (byte-identical + reindex)
// ──────────────────────────────────────────────────────────────

#[test]
fn create_then_restore_is_byte_identical_and_reindexes() {
    let src = TempDir::new().unwrap();
    let archives = TempDir::new().unwrap();
    let archive = archives.path().join("backup.fondbkp");

    seed_source(src.path());

    fond_at(src.path())
        .args(["backup", "create", "--dest"])
        .arg(&archive)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created plaintext backup"));
    assert!(archive.exists(), "archive should be written");

    // Restore into a fresh, empty data dir.
    let dst = TempDir::new().unwrap();
    fond_at(dst.path())
        .args(["backup", "restore"])
        .arg(&archive)
        .assert()
        .success()
        .stdout(predicate::str::contains("Restored"))
        .stdout(predicate::str::contains("Rebuilt index: 2 recipe(s)"));

    // Restored recipe + photo bytes are identical to the source.
    for rel in [
        "recipes/chicken-adobo.cook",
        "recipes/steamed-rice.cook",
        PHOTO_REL,
    ] {
        let a = fs::read(src.path().join(rel)).unwrap();
        let b = fs::read(dst.path().join(rel)).unwrap();
        assert_eq!(a, b, "restored {rel} should be byte-identical");
    }

    // A fresh reindex (run by restore) rebuilt fond.db, and list sees the recipes.
    assert!(dst.path().join("fond.db").exists(), "fond.db rebuilt");
    fond_at(dst.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Chicken Adobo"))
        .stdout(predicate::str::contains("Steamed Rice"));
}

// ──────────────────────────────────────────────────────────────
// tamper → restore refuses (fail closed)
// ──────────────────────────────────────────────────────────────

#[test]
fn tampered_archive_restore_fails_closed() {
    let src = TempDir::new().unwrap();
    let archives = TempDir::new().unwrap();
    let archive = archives.path().join("backup.fondbkp");

    seed_source(src.path());
    fond_at(src.path())
        .args(["backup", "create", "--dest"])
        .arg(&archive)
        .assert()
        .success();

    // Flip the final byte (part of a file body) to break its per-file hash.
    let mut bytes = fs::read(&archive).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    fs::write(&archive, &bytes).unwrap();

    let dst = TempDir::new().unwrap();
    fond_at(dst.path())
        .args(["backup", "restore"])
        .arg(&archive)
        .assert()
        .failure()
        .stderr(predicate::str::contains("verification failed"));

    // Fail closed: nothing was written to the target.
    assert!(
        !dst.path().join("recipes/chicken-adobo.cook").exists(),
        "no files should be written when verification fails"
    );
    assert!(
        !dst.path().join("fond.db").exists(),
        "no reindex should run on a failed restore"
    );
}

// ──────────────────────────────────────────────────────────────
// dry-run reports without writing
// ──────────────────────────────────────────────────────────────

#[test]
fn dry_run_restore_writes_nothing() {
    let src = TempDir::new().unwrap();
    let archives = TempDir::new().unwrap();
    let archive = archives.path().join("backup.fondbkp");

    seed_source(src.path());
    fond_at(src.path())
        .args(["backup", "create", "--dest"])
        .arg(&archive)
        .assert()
        .success();

    let dst = TempDir::new().unwrap();
    fond_at(dst.path())
        .args(["backup", "restore", "--dry-run"])
        .arg(&archive)
        .assert()
        .success()
        .stdout(predicate::str::contains("Dry run"))
        .stdout(predicate::str::contains("would be restored"));

    assert!(
        !dst.path().join("recipes/chicken-adobo.cook").exists(),
        "dry run must not write any files"
    );
    assert!(
        !dst.path().join("fond.db").exists(),
        "dry run must not reindex"
    );
}

// ──────────────────────────────────────────────────────────────
// verify drill (PASS / FAIL)
// ──────────────────────────────────────────────────────────────

#[test]
fn verify_passes_on_good_archive_and_fails_on_tamper() {
    let src = TempDir::new().unwrap();
    let archives = TempDir::new().unwrap();
    let archive = archives.path().join("backup.fondbkp");

    seed_source(src.path());
    fond_at(src.path())
        .args(["backup", "create", "--dest"])
        .arg(&archive)
        .assert()
        .success();

    fond_at(src.path())
        .args(["backup", "verify"])
        .arg(&archive)
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));

    // Verify with a source diff still passes and reports zero drift.
    fond_at(src.path())
        .args(["backup", "verify", "--against-source"])
        .arg(&archive)
        .assert()
        .success()
        .stdout(predicate::str::contains("unchanged:"));

    // Tamper → verify FAILS closed.
    let mut bytes = fs::read(&archive).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    fs::write(&archive, &bytes).unwrap();

    fond_at(src.path())
        .args(["backup", "verify"])
        .arg(&archive)
        .assert()
        .failure()
        .stderr(predicate::str::contains("FAILED"));
}

// ──────────────────────────────────────────────────────────────
// --format json
// ──────────────────────────────────────────────────────────────

#[test]
fn json_output_on_create_verify_restore() {
    let src = TempDir::new().unwrap();
    let archives = TempDir::new().unwrap();
    let archive = archives.path().join("backup.fondbkp");

    seed_source(src.path());

    let create = fond_at(src.path())
        .args(["--json", "backup", "create", "--dest"])
        .arg(&archive)
        .assert()
        .success();
    let out = String::from_utf8(create.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["encrypted"], serde_json::Value::Bool(false));
    assert!(v["manifest"]["entries"].as_array().unwrap().len() >= 3);

    let verify = fond_at(src.path())
        .args(["--json", "backup", "verify"])
        .arg(&archive)
        .assert()
        .success();
    let out = String::from_utf8(verify.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["status"], "pass");

    let dst = TempDir::new().unwrap();
    let restore = fond_at(dst.path())
        .args(["--json", "backup", "restore"])
        .arg(&archive)
        .assert()
        .success();
    let out = String::from_utf8(restore.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["restore"]["recipes"], 2);
    assert_eq!(v["reindexed"], 2);
}

// ──────────────────────────────────────────────────────────────
// encrypted round-trip (passphrase, non-interactive) + fail closed
// ──────────────────────────────────────────────────────────────

#[test]
fn encrypted_round_trip_via_passphrase_and_fails_without_key() {
    let src = TempDir::new().unwrap();
    let archives = TempDir::new().unwrap();
    let archive = archives.path().join("secret.fondbkp");

    seed_source(src.path());

    fond_at(src.path())
        .env("FOND_OVERLAY_PASSPHRASE", "correct horse battery staple")
        .args(["backup", "create", "--encrypt", "--passphrase", "--dest"])
        .arg(&archive)
        .assert()
        .success()
        .stdout(predicate::str::contains("encrypted (passphrase key)"));

    // Restore with the passphrase succeeds and rebuilds the tree.
    let dst = TempDir::new().unwrap();
    fond_at(dst.path())
        .env("FOND_OVERLAY_PASSPHRASE", "correct horse battery staple")
        .args(["backup", "restore"])
        .arg(&archive)
        .assert()
        .success()
        .stdout(predicate::str::contains("Restored"));
    assert!(dst.path().join("recipes/chicken-adobo.cook").exists());

    // Restore WITHOUT the passphrase fails closed (stdin is not a TTY here).
    let dst2 = TempDir::new().unwrap();
    fond_at(dst2.path())
        .args(["backup", "restore"])
        .arg(&archive)
        .assert()
        .failure();
    assert!(
        !dst2.path().join("recipes/chicken-adobo.cook").exists(),
        "missing key must not write plaintext"
    );
}

// ──────────────────────────────────────────────────────────────
// overlay + photos are captured and restored, reindex still merges
// ──────────────────────────────────────────────────────────────

#[test]
fn backup_captures_overlay_sidecars() {
    let src = TempDir::new().unwrap();
    let archives = TempDir::new().unwrap();
    let archive = archives.path().join("backup.fondbkp");

    seed_source(src.path());

    // Produce real overlay sidecars via the shared pantry, then export them.
    fond_at(src.path())
        .args(["pantry", "add", "salt", "pepper"])
        .assert()
        .success();
    fond_at(src.path())
        .args(["overlay", "export"])
        .assert()
        .success();
    assert!(
        src.path().join("overlay/shared/pantry.jsonl").exists(),
        "overlay export should write a pantry sidecar"
    );

    fond_at(src.path())
        .args(["--json", "backup", "create", "--dest"])
        .arg(&archive)
        .assert()
        .success();

    let bytes = fs::read(&archive).unwrap();
    // Verify JSON reports at least one overlay entry captured.
    let out = fond_at(src.path())
        .args(["--json", "backup", "verify"])
        .arg(&archive)
        .assert()
        .success();
    let _ = bytes;
    let json = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(v["entries"].as_u64().unwrap() >= 4);

    // Restore into a fresh dir; the overlay sidecar comes back byte-identical and
    // reindex merges it without error.
    let dst = TempDir::new().unwrap();
    fond_at(dst.path())
        .args(["backup", "restore"])
        .arg(&archive)
        .assert()
        .success();
    let a = fs::read(src.path().join("overlay/shared/pantry.jsonl")).unwrap();
    let b = fs::read(dst.path().join("overlay/shared/pantry.jsonl")).unwrap();
    assert_eq!(a, b, "restored overlay sidecar should be byte-identical");
}
