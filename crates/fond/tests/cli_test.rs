//! CLI integration tests for the `fond` binary.
//!
//! Each test creates a temp directory and sets `FOND_DATA_DIR` to
//! keep tests isolated from each other and from the user's real data.

use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// Build a `fond` command pointed at a temp dir.
fn fond(tmp: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("fond").unwrap();
    cmd.env("FOND_DATA_DIR", tmp.path());
    cmd
}

/// Write a .cook fixture into the recipes directory.
fn write_fixture(tmp: &TempDir, name: &str, content: &str) {
    let recipes = tmp.path().join("recipes");
    fs::create_dir_all(&recipes).unwrap();
    fs::write(recipes.join(name), content).unwrap();
}

const CHICKEN_COOK: &str = "\
---
title: Chicken Adobo
servings: 4
tags:
  - filipino
  - braised
---

Combine @soy sauce{1/2 cup} and @vinegar{1/2 cup} in a bowl.

Add @chicken thighs{2 lbs} and marinate for ~{30 minutes}.

Cook over medium heat with @garlic{6 cloves} for ~{45 minutes}.
";

// ──────────────────────────────────────────────────────────────
// --help and version
// ──────────────────────────────────────────────────────────────

#[test]
fn help_flag_shows_usage() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp)
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("local-first personal cooking"));
}

#[test]
fn version_flag_shows_version() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp)
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("fond"));
}

// ──────────────────────────────────────────────────────────────
// init
// ──────────────────────────────────────────────────────────────

#[test]
fn init_creates_directories() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialised fond at"));

    assert!(tmp.path().join("recipes").exists());
}

// ──────────────────────────────────────────────────────────────
// reindex
// ──────────────────────────────────────────────────────────────

#[test]
fn reindex_empty_dir() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    fond(&tmp)
        .arg("reindex")
        .assert()
        .success()
        .stdout(predicate::str::contains("Reindexed 0"));
}

#[test]
fn reindex_with_fixture() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);

    fond(&tmp)
        .arg("reindex")
        .assert()
        .success()
        .stdout(predicate::str::contains("Reindexed 1"));
}

#[test]
fn reindex_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);

    fond(&tmp)
        .args(["--json", "reindex"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"indexed\": 1"));
}

// ──────────────────────────────────────────────────────────────
// list
// ──────────────────────────────────────────────────────────────

#[test]
fn list_empty_shows_message() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    fond(&tmp)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("No recipes indexed"));
}

#[test]
fn list_after_reindex_shows_recipe() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Chicken Adobo"))
        .stdout(predicate::str::contains("chicken-adobo"))
        .stdout(predicate::str::contains("1 recipe(s)"));
}

#[test]
fn list_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["--json", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"slug\": \"chicken-adobo\""))
        .stdout(predicate::str::contains("\"title\": \"Chicken Adobo\""));
}

// ──────────────────────────────────────────────────────────────
// view
// ──────────────────────────────────────────────────────────────

#[test]
fn view_existing_recipe() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["view", "chicken-adobo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("# Chicken Adobo"))
        .stdout(predicate::str::contains("soy sauce"));
}

#[test]
fn view_missing_recipe_fails() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["view", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no recipe found with slug"));
}

#[test]
fn view_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["--json", "view", "chicken-adobo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"title\": \"Chicken Adobo\""))
        .stdout(predicate::str::contains("\"slug\": \"chicken-adobo\""));
}

// ──────────────────────────────────────────────────────────────
// search
// ──────────────────────────────────────────────────────────────

#[test]
fn search_finds_by_title() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["search", "adobo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Chicken Adobo"));
}

#[test]
fn search_finds_by_ingredient() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["search", "vinegar"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Chicken Adobo"));
}

#[test]
fn search_no_results() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["search", "xylophone"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No results"));
}

#[test]
fn search_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["--json", "search", "chicken"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"slug\": \"chicken-adobo\""));
}

// ──────────────────────────────────────────────────────────────
// add
// ──────────────────────────────────────────────────────────────

#[test]
fn add_from_file() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    // Write a .cook file outside the recipes dir
    let source = tmp.path().join("external.cook");
    fs::write(
        &source,
        "---\ntitle: Test Recipe\n---\n\nAdd @salt{1 tsp}.\n",
    )
    .unwrap();

    fond(&tmp)
        .args(["add", "--file", source.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Added: Test Recipe"));

    // The file should now be in recipes/
    assert!(tmp.path().join("recipes").join("external.cook").exists());

    // And it should be indexed (list should find it)
    fond(&tmp)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Test Recipe"));
}

#[test]
fn add_from_file_rejects_non_cook() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    let source = tmp.path().join("readme.txt");
    fs::write(&source, "just text").unwrap();

    fond(&tmp)
        .args(["add", "--file", source.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("expected a .cook file"));
}

#[test]
fn add_from_file_rejects_collision() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(
        &tmp,
        "duplicate.cook",
        "---\ntitle: Original\n---\n\nStep 1.\n",
    );

    let source = tmp.path().join("duplicate.cook");
    fs::write(&source, "---\ntitle: Duplicate\n---\n\nStep 1.\n").unwrap();

    fond(&tmp)
        .args(["add", "--file", source.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn add_title_json_creates_file() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["--json", "add", "--title", "Pasta Carbonara"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"slug\": \"pasta-carbonara\""))
        .stdout(predicate::str::contains("\"action\": \"added\""));

    assert!(
        tmp.path()
            .join("recipes")
            .join("pasta-carbonara.cook")
            .exists()
    );
}

#[test]
fn add_json_without_inputs_fails() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["--json", "add"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("non-interactive"));
}

// ──────────────────────────────────────────────────────────────
// rm
// ──────────────────────────────────────────────────────────────

#[test]
fn rm_with_yes_removes_recipe() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["rm", "chicken-adobo", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed: Chicken Adobo"));

    assert!(
        !tmp.path()
            .join("recipes")
            .join("chicken-adobo.cook")
            .exists()
    );

    // list should now be empty
    fond(&tmp)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("No recipes indexed"));
}

#[test]
fn rm_missing_recipe_fails() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["rm", "nonexistent", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no recipe found with slug"));
}

#[test]
fn rm_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["--json", "rm", "chicken-adobo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"action\": \"removed\""))
        .stdout(predicate::str::contains("\"slug\": \"chicken-adobo\""));
}

// ──────────────────────────────────────────────────────────────
// completions
// ──────────────────────────────────────────────────────────────

#[test]
fn completions_generates_bash() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp)
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete"));
}

#[test]
fn completions_generates_powershell() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp)
        .args(["completions", "powershell"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fond"));
}

// ──────────────────────────────────────────────────────────────
// format flags
// ──────────────────────────────────────────────────────────────

#[test]
fn format_flag_json_is_equivalent_to_json_flag() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    // --format json
    let out1 = fond(&tmp)
        .args(["--format", "json", "list"])
        .output()
        .unwrap();

    // --json
    let out2 = fond(&tmp).args(["--json", "list"]).output().unwrap();

    assert_eq!(out1.stdout, out2.stdout);
}
