//! CLI integration tests for the `fond` binary.
//!
//! Each test creates a temp directory and sets `FOND_DATA_DIR` to
//! keep tests isolated from each other and from the user's real data.

use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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

#[cfg(unix)]
fn write_fake_tesseract(tmp: &TempDir, ocr_text: &str) -> std::path::PathBuf {
    let script_path = tmp.path().join("fake-tesseract.sh");
    let text_path = tmp.path().join("fake-ocr-output.txt");
    fs::write(&text_path, ocr_text).unwrap();

    let script = format!(
        "#!/bin/sh\nset -eu\noutput_base=\"$2\"\ncat \"{}\" > \"${{output_base}}.txt\"\n",
        text_path.display()
    );
    fs::write(&script_path, script).unwrap();

    let mut perms = fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).unwrap();
    script_path
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

// ──────────────────────────────────────────────────────────────
// list with filters
// ──────────────────────────────────────────────────────────────

const PASTA_COOK: &str = "\
---
title: Pasta Carbonara
servings: 4
tags:
  - italian
  - pasta
prep time: 10 min
cook time: 20 min
---

Cook @pasta{1 lb} in boiling water for ~{10 minutes}.

Whisk @eggs{3} with @pecorino{1 cup} and @black pepper{1 tsp}.

Fry @guanciale{6 oz} until crispy, ~{8 minutes}.

Toss hot pasta with egg mixture and guanciale.
";

#[test]
fn list_filter_by_tag() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    write_fixture(&tmp, "pasta-carbonara.cook", PASTA_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["list", "--tag", "italian"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pasta Carbonara"))
        .stdout(predicate::str::contains("1 recipe(s)"));
}

#[test]
fn list_filter_by_cuisine() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    write_fixture(&tmp, "pasta-carbonara.cook", PASTA_COOK);
    fond(&tmp).arg("reindex").assert().success();

    // --cuisine is sugar for --tag
    fond(&tmp)
        .args(["list", "--cuisine", "filipino"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Chicken Adobo"))
        .stdout(predicate::str::contains("1 recipe(s)"));
}

#[test]
fn list_filter_by_max_time() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "pasta-carbonara.cook", PASTA_COOK);
    fond(&tmp).arg("reindex").assert().success();

    // Pasta carbonara: prep 10 + cook 20 = 30 min
    fond(&tmp)
        .args(["list", "--max-time", "30"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pasta Carbonara"));

    // max-time 15 should exclude it
    fond(&tmp)
        .args(["list", "--max-time", "15"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No recipes match"));
}

#[test]
fn list_filter_no_matches() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["list", "--tag", "nonexistent-tag"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No recipes match"));
}

#[test]
fn list_filter_combined_tag_and_max_time() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    write_fixture(&tmp, "pasta-carbonara.cook", PASTA_COOK);
    fond(&tmp).arg("reindex").assert().success();

    // Both have tags, but only pasta is <= 30 min
    fond(&tmp)
        .args(["list", "--max-time", "35"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pasta Carbonara"))
        .stdout(predicate::str::contains("1 recipe(s)"));
}

#[test]
fn list_filter_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    write_fixture(&tmp, "pasta-carbonara.cook", PASTA_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["--json", "list", "--tag", "italian"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"slug\": \"pasta-carbonara\""))
        .stdout(predicate::str::contains("\"tags\""));
}

// ──────────────────────────────────────────────────────────────
// search with filters
// ──────────────────────────────────────────────────────────────

#[test]
fn search_with_tag_filter() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    write_fixture(&tmp, "pasta-carbonara.cook", PASTA_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["search", "cook", "--tag", "italian"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pasta Carbonara"));
}

#[test]
fn search_with_max_time_filter() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    write_fixture(&tmp, "pasta-carbonara.cook", PASTA_COOK);
    fond(&tmp).arg("reindex").assert().success();

    // Both recipes have "cook" in step text, but only pasta <=30 min
    fond(&tmp)
        .args(["search", "cook", "--max-time", "30"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pasta Carbonara"));
}

#[test]
fn search_json_includes_tags_and_source() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["--json", "search", "chicken"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"tags\""))
        .stdout(predicate::str::contains("\"source\""));
}

// ──────────────────────────────────────────────────────────────
// tag command
// ──────────────────────────────────────────────────────────────

#[test]
fn tag_list_all() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["tag", "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("filipino"))
        .stdout(predicate::str::contains("braised"));
}

#[test]
fn tag_list_json() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["--json", "tag", "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\""))
        .stdout(predicate::str::contains("\"count\""));
}

#[test]
fn tag_show_for_recipe() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["tag", "chicken-adobo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("filipino"))
        .stdout(predicate::str::contains("braised"));
}

#[test]
fn tag_add_and_verify() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["tag", "chicken-adobo", "--add", "dinner,easy"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Added: dinner, easy"));

    // Verify the tags persisted (list should now include new tags)
    fond(&tmp)
        .args(["tag", "chicken-adobo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dinner"))
        .stdout(predicate::str::contains("easy"))
        .stdout(predicate::str::contains("filipino"));
}

#[test]
fn tag_remove_and_verify() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["tag", "chicken-adobo", "--remove", "braised"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed: braised"));

    // Verify the tag was removed
    let output = fond(&tmp).args(["tag", "chicken-adobo"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("braised"),
        "braised should be removed, got: {stdout}"
    );
    assert!(stdout.contains("filipino"), "filipino should remain");
}

#[test]
fn tag_add_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["--json", "tag", "chicken-adobo", "--add", "quick"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"added\""))
        .stdout(predicate::str::contains("\"quick\""));
}

#[test]
fn tag_survives_reindex() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    // Add a tag
    fond(&tmp)
        .args(["tag", "chicken-adobo", "--add", "weeknight"])
        .assert()
        .success();

    // Reindex — since we wrote to the .cook file, the tag should persist
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["tag", "chicken-adobo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("weeknight"));
}

#[test]
fn tag_add_then_search_finds_by_new_tag() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    // Add a tag
    fond(&tmp)
        .args(["tag", "chicken-adobo", "--add", "weeknight"])
        .assert()
        .success();

    // Search by the new tag via FTS
    fond(&tmp)
        .args(["search", "weeknight"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Chicken Adobo"));

    // Also filter by the new tag
    fond(&tmp)
        .args(["list", "--tag", "weeknight"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Chicken Adobo"));
}

// ──────────────────────────────────────────────────────────────
// import paprika
// ──────────────────────────────────────────────────────────────

/// Create a synthetic Paprika archive file for CLI tests.
fn write_paprika_archive(
    tmp: &TempDir,
    name: &str,
    recipes: &[serde_json::Value],
) -> std::path::PathBuf {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let archive_path = tmp.path().join(name);
    let buf = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(buf);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    for (i, recipe_json) in recipes.iter().enumerate() {
        let json_str = serde_json::to_string(recipe_json).unwrap();
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(json_str.as_bytes()).unwrap();
        let gzipped = encoder.finish().unwrap();

        zip.start_file(format!("recipe-{i}.paprikarecipe"), options)
            .unwrap();
        zip.write_all(&gzipped).unwrap();
    }

    let data = zip.finish().unwrap().into_inner();
    fs::write(&archive_path, &data).unwrap();
    archive_path
}

#[test]
fn import_paprika_dry_run() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    let archive = write_paprika_archive(
        &tmp,
        "export.paprikarecipes",
        &[
            serde_json::json!({
                "name": "Chicken Adobo",
                "uid": "UID-001",
                "ingredients": "2 lbs chicken\n1 cup soy sauce",
                "directions": "Cook chicken.\nServe.",
                "categories": ["Filipino"]
            }),
            serde_json::json!({
                "name": "Scrambled Eggs",
                "uid": "UID-002"
            }),
        ],
    );

    fond(&tmp)
        .args(["import", "paprika", archive.to_str().unwrap(), "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Imported: 2"))
        .stdout(predicate::str::contains("[dry-run]"));

    // No .cook files should be written in dry-run mode
    let recipes_dir = tmp.path().join("recipes");
    let files: Vec<_> = fs::read_dir(&recipes_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "cook"))
        .collect();
    assert!(
        files.is_empty(),
        "dry-run should not write files, found: {files:?}"
    );
}

#[test]
fn import_paprika_writes_files_and_indexes() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    let archive = write_paprika_archive(
        &tmp,
        "export.paprikarecipes",
        &[serde_json::json!({
            "name": "Test Recipe",
            "uid": "UID-100",
            "ingredients": "1 cup flour\n2 eggs",
            "directions": "Mix and bake.",
            "categories": ["Baking"],
            "source": "Test Kitchen",
            "source_url": "https://example.com/test"
        })],
    );

    fond(&tmp)
        .args(["import", "paprika", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Imported: 1"));

    // .cook file should exist
    let cook_file = tmp.path().join("recipes").join("test-recipe.cook");
    assert!(cook_file.exists(), "should write test-recipe.cook");

    let content = fs::read_to_string(&cook_file).unwrap();
    assert!(content.contains("title: Test Recipe"));
    assert!(content.contains("import source: paprika"));
    assert!(content.contains("paprika uid: UID-100"));

    // Recipe should be indexed and searchable
    fond(&tmp)
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Test Recipe"));
}

#[test]
fn import_paprika_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    let archive = write_paprika_archive(
        &tmp,
        "export.paprikarecipes",
        &[serde_json::json!({
            "name": "JSON Test",
            "uid": "UID-JSON"
        })],
    );

    fond(&tmp)
        .args([
            "--json",
            "import",
            "paprika",
            archive.to_str().unwrap(),
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"imported\": 1"))
        .stdout(predicate::str::contains("\"status\": \"imported\""));
}

#[test]
fn import_paprika_skips_duplicates() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    let archive = write_paprika_archive(
        &tmp,
        "export.paprikarecipes",
        &[serde_json::json!({
            "name": "My Recipe",
            "source_url": "https://example.com/recipe"
        })],
    );

    // First import
    fond(&tmp)
        .args(["import", "paprika", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Imported: 1"));

    // Second import — should skip due to duplicate source URL
    fond(&tmp)
        .args(["import", "paprika", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipped:  1"));
}

#[test]
fn import_paprika_missing_file_fails() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["import", "paprika", "nonexistent.paprikarecipes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("file not found"));
}

#[test]
fn import_help_shows_paprika_subcommand() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp)
        .args(["import", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("paprika"))
        .stdout(predicate::str::contains("photo"))
        .stdout(predicate::str::contains("url"));
}

// ──────────────────────────────────────────────────────────────
// import url
// ──────────────────────────────────────────────────────────────

#[test]
fn import_url_rejects_invalid_scheme() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["import", "url", "ftp://example.com/recipe"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("only http:// and https://"));
}

#[test]
fn import_url_rejects_file_scheme() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["import", "url", "file:///etc/passwd"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("only http:// and https://"));
}

#[test]
fn import_url_help_shows_options() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp)
        .args(["import", "url", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dry-run"))
        .stdout(predicate::str::contains("URL"));
}

#[cfg(unix)]
#[test]
fn import_photo_dry_run_reports_queued() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    let image = tmp.path().join("recipe-card.png");
    fs::write(&image, b"not-a-real-image").unwrap();
    let fake_tesseract = write_fake_tesseract(
        &tmp,
        "Weekend Pancakes\nIngredients\n2 cups flour\n1 cup milk\nDirections\nMix everything.\nCook on a griddle.\n",
    );

    fond(&tmp)
        .env("FOND_TESSERACT_BIN", &fake_tesseract)
        .args(["import", "photo", image.to_str().unwrap(), "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[dry-run] Queued:   1 recipe(s)"))
        .stdout(predicate::str::contains("Weekend Pancakes"));

    fond(&tmp)
        .args(["review", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No queued review drafts."));
}

#[cfg(unix)]
#[test]
fn import_photo_queues_and_accepts_review() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    let image = tmp.path().join("soup-card.png");
    fs::write(&image, b"not-a-real-image").unwrap();
    let fake_tesseract = write_fake_tesseract(
        &tmp,
        "Grandma Soup\nIngredients\n1 onion\n4 cups broth\nDirections\nSimmer the soup.\nServe hot.\n",
    );

    let import_output = fond(&tmp)
        .env("FOND_TESSERACT_BIN", &fake_tesseract)
        .args(["--json", "import", "photo", image.to_str().unwrap()])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let import_json: serde_json::Value =
        serde_json::from_slice(&import_output).expect("valid JSON import report");

    assert_eq!(import_json["queued"], 1);
    let review_id = import_json["details"][0]["review_id"]
        .as_str()
        .expect("review id in queued import report")
        .to_string();

    fond(&tmp)
        .args(["review", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Grandma Soup"));

    fond(&tmp)
        .args(["review", "accept", &review_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Imported: Grandma Soup"));

    assert!(
        tmp.path()
            .join("recipes")
            .join("grandma-soup.cook")
            .exists()
    );
    assert!(tmp.path().join("photos").join("review").exists());

    fond(&tmp)
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Grandma Soup"));
}

// ──────────────────────────────────────────────────────────────
// pantry
// ──────────────────────────────────────────────────────────────

#[test]
fn pantry_add_and_list() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["pantry", "add", "flour", "eggs", "butter"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Added 3 item(s)"));

    fond(&tmp)
        .args(["pantry", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("flour"))
        .stdout(predicate::str::contains("eggs"))
        .stdout(predicate::str::contains("butter"));
}

#[test]
fn pantry_add_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["--json", "pantry", "add", "flour", "eggs"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"action\""))
        .stdout(predicate::str::contains("\"add\""))
        .stdout(predicate::str::contains("\"flour\""));
}

#[test]
fn pantry_rm_and_list() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["pantry", "add", "flour", "eggs", "butter"])
        .assert()
        .success();

    fond(&tmp)
        .args(["pantry", "rm", "eggs"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed 1 item(s)"));

    // Default list should not show removed items
    let output = fond(&tmp).args(["pantry", "list"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("eggs"), "eggs should not appear: {stdout}");
    assert!(stdout.contains("flour"));
    assert!(stdout.contains("butter"));
}

#[test]
fn pantry_list_all_shows_absent() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["pantry", "add", "flour", "eggs"])
        .assert()
        .success();
    fond(&tmp).args(["pantry", "rm", "eggs"]).assert().success();

    fond(&tmp)
        .args(["pantry", "list", "--all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("eggs"))
        .stdout(predicate::str::contains("flour"));
}

#[test]
fn pantry_list_empty() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["pantry", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No items"));
}

#[test]
fn pantry_list_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["pantry", "add", "flour"])
        .assert()
        .success();

    fond(&tmp)
        .args(["--json", "pantry", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\""))
        .stdout(predicate::str::contains("\"flour\""))
        .stdout(predicate::str::contains("\"present\""));
}

#[test]
fn pantry_check_coverage() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    // Add some ingredients
    fond(&tmp)
        .args(["pantry", "add", "soy sauce", "vinegar", "garlic"])
        .assert()
        .success();

    fond(&tmp)
        .args(["pantry", "check", "chicken-adobo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("coverage"))
        .stdout(predicate::str::contains("have"));
}

#[test]
fn pantry_check_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["pantry", "add", "soy sauce"])
        .assert()
        .success();

    fond(&tmp)
        .args(["--json", "pantry", "check", "chicken-adobo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"coverage_pct\""))
        .stdout(predicate::str::contains("\"ingredients\""))
        .stdout(predicate::str::contains("\"matched\""));
}

#[test]
fn pantry_check_nonexistent_recipe() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["pantry", "check", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("recipe not found"));
}

// ──────────────────────────────────────────────────────────────
// suggest ("what can I cook?")
// ──────────────────────────────────────────────────────────────

#[test]
fn suggest_empty_pantry_shows_guidance() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["suggest"])
        .assert()
        .success()
        .stdout(predicate::str::contains("pantry is empty"))
        .stdout(predicate::str::contains("fond pantry add"));
}

#[test]
fn suggest_ranks_by_coverage() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    write_fixture(&tmp, "pasta-carbonara.cook", PASTA_COOK);
    fond(&tmp).arg("reindex").assert().success();

    // Fully stock chicken adobo (4/4) but only one pasta ingredient.
    fond(&tmp)
        .args([
            "pantry",
            "add",
            "soy sauce",
            "vinegar",
            "chicken thighs",
            "garlic",
            "pasta",
        ])
        .assert()
        .success();

    // Raise the cap so the lower-coverage pasta still shows.
    let output = fond(&tmp)
        .args(["suggest", "--max-missing", "10"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    let chicken_pos = stdout.find("chicken-adobo").expect("chicken not listed");
    let pasta_pos = stdout.find("pasta-carbonara").expect("pasta not listed");
    assert!(
        chicken_pos < pasta_pos,
        "higher-coverage recipe should rank first:\n{stdout}"
    );
    assert!(
        stdout.contains("make now"),
        "fully-covered recipe should be flagged:\n{stdout}"
    );
}

#[test]
fn suggest_max_missing_filters_results() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    write_fixture(&tmp, "pasta-carbonara.cook", PASTA_COOK);
    fond(&tmp).arg("reindex").assert().success();

    // Cover 3/4 of chicken (1 missing) and 0/5 of pasta (5 missing).
    fond(&tmp)
        .args(["pantry", "add", "soy sauce", "vinegar", "garlic"])
        .assert()
        .success();

    // Default cap (2) keeps chicken (1 missing) and drops pasta (5 missing).
    fond(&tmp)
        .args(["suggest"])
        .assert()
        .success()
        .stdout(predicate::str::contains("chicken-adobo"))
        .stdout(predicate::str::contains("pasta-carbonara").not());

    // A strict cap of 0 leaves nothing (chicken still has 1 missing).
    fond(&tmp)
        .args(["suggest", "--max-missing", "0"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No recipes are within 0 missing"));
}

#[test]
fn suggest_limit_caps_results() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    write_fixture(&tmp, "pasta-carbonara.cook", PASTA_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args([
            "pantry",
            "add",
            "soy sauce",
            "vinegar",
            "chicken thighs",
            "garlic",
            "pasta",
        ])
        .assert()
        .success();

    fond(&tmp)
        .args(["suggest", "--max-missing", "10", "--limit", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 suggestion(s)"))
        .stdout(predicate::str::contains("pasta-carbonara").not());
}

#[test]
fn suggest_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["pantry", "add", "soy sauce", "vinegar", "garlic"])
        .assert()
        .success();

    fond(&tmp)
        .args(["--json", "suggest"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"coverage_pct\""))
        .stdout(predicate::str::contains("\"missing\""))
        .stdout(predicate::str::contains("\"slug\": \"chicken-adobo\""));
}

#[test]
fn suggest_json_empty_pantry_is_empty_array() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["--json", "suggest"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[]"));
}

#[test]
fn pantry_rm_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["pantry", "add", "flour"])
        .assert()
        .success();

    fond(&tmp)
        .args(["--json", "pantry", "rm", "flour"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"action\""))
        .stdout(predicate::str::contains("\"remove\""))
        .stdout(predicate::str::contains("\"flour\""));
}

#[test]
fn pantry_help_shows_subcommands() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp)
        .args(["pantry", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("add"))
        .stdout(predicate::str::contains("rm"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("check"));
}

#[test]
fn pantry_survives_reindex() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    // Add pantry items
    fond(&tmp)
        .args(["pantry", "add", "flour", "eggs"])
        .assert()
        .success();

    // Reindex — pantry should survive
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["pantry", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("flour"))
        .stdout(predicate::str::contains("eggs"));
}

#[test]
fn pantry_quoted_multi_word_items() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    // Multi-word items via shell quoting
    fond(&tmp)
        .args(["pantry", "add", "olive oil", "soy sauce"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Added 2 item(s)"));

    fond(&tmp)
        .args(["pantry", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("olive oil"))
        .stdout(predicate::str::contains("soy sauce"));
}

// ──────────────────────────────────────────────────────────────
// Grocery
// ──────────────────────────────────────────────────────────────

#[test]
fn grocery_help_shows_subcommands() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp)
        .args(["grocery", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("from-recipe"));
}

#[test]
fn grocery_from_recipe_basic() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["grocery", "from-recipe", "chicken-adobo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Grocery list for"))
        .stdout(predicate::str::contains("to buy"));
}

#[test]
fn grocery_from_recipe_json() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    let output = fond(&tmp)
        .args(["grocery", "from-recipe", "chicken-adobo", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["recipe_slug"], "chicken-adobo");
    assert!(json["items"].is_array());
    assert!(json["categories"].is_array());
    assert!(json["total_recipe_ingredients"].as_u64().unwrap() > 0);
}

#[test]
fn grocery_subtracts_pantry() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    // Get full grocery list count
    let full_output = fond(&tmp)
        .args(["grocery", "from-recipe", "chicken-adobo", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let full: serde_json::Value = serde_json::from_slice(&full_output).unwrap();
    let full_count = full["items"].as_array().unwrap().len();

    // Add pantry items
    fond(&tmp)
        .args(["pantry", "add", "soy sauce", "garlic"])
        .assert()
        .success();

    // Get reduced grocery list
    let reduced_output = fond(&tmp)
        .args(["grocery", "from-recipe", "chicken-adobo", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let reduced: serde_json::Value = serde_json::from_slice(&reduced_output).unwrap();
    let reduced_count = reduced["items"].as_array().unwrap().len();

    assert!(
        reduced_count < full_count,
        "pantry items should reduce the grocery list"
    );
    assert!(reduced["pantry_covered_count"].as_u64().unwrap() >= 2);
}

#[test]
fn grocery_include_pantry_flag() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["pantry", "add", "soy sauce"])
        .assert()
        .success();

    // Without flag - pantry items excluded
    let without = fond(&tmp)
        .args(["grocery", "from-recipe", "chicken-adobo", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let without_json: serde_json::Value = serde_json::from_slice(&without).unwrap();

    // With flag - pantry items included
    let with = fond(&tmp)
        .args([
            "grocery",
            "from-recipe",
            "chicken-adobo",
            "--include-pantry",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let with_json: serde_json::Value = serde_json::from_slice(&with).unwrap();

    assert!(
        with_json["items"].as_array().unwrap().len()
            > without_json["items"].as_array().unwrap().len()
    );

    // Find the covered item
    let covered: Vec<&serde_json::Value> = with_json["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["pantry_covered"].as_bool() == Some(true))
        .collect();
    assert!(
        !covered.is_empty(),
        "should have at least one pantry-covered item"
    );
}

#[test]
fn grocery_nonexistent_recipe() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["grocery", "from-recipe", "does-not-exist"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("recipe not found"));
}

#[test]
fn grocery_items_have_categories() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    let output = fond(&tmp)
        .args(["grocery", "from-recipe", "chicken-adobo", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let items = json["items"].as_array().unwrap();

    for item in items {
        assert!(
            item["category"].is_string(),
            "every item should have a category"
        );
        assert!(
            !item["category"].as_str().unwrap().is_empty(),
            "category should not be empty"
        );
    }
}

#[test]
fn grocery_table_shows_categories() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["grocery", "from-recipe", "chicken-adobo"])
        .assert()
        .success()
        // Table should show category grouping markers
        .stdout(predicate::str::contains("──"));
}

// ──────────────────────────────────────────────────────────────
// fond export
// ──────────────────────────────────────────────────────────────

#[test]
fn export_json_stdout_single_recipe() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["export", "--recipe", "chicken-adobo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"schema_version\": 1"))
        .stdout(predicate::str::contains("\"recipe_count\": 1"))
        .stdout(predicate::str::contains("Chicken Adobo"));
}

#[test]
fn export_json_stdout_all_recipes() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    write_fixture(
        &tmp,
        "toast.cook",
        "---\ntitle: Toast\n---\n\nPut @bread{2 slices} in the toaster.\n",
    );
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["export"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"recipe_count\": 2"))
        .stdout(predicate::str::contains("Chicken Adobo"))
        .stdout(predicate::str::contains("Toast"));
}

#[test]
fn export_json_to_file() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    let out_path = tmp.path().join("export.json");
    fond(&tmp)
        .args(["export", "--output", out_path.to_str().unwrap()])
        .assert()
        .success();

    let content = fs::read_to_string(&out_path).unwrap();
    assert!(content.contains("\"schema_version\": 1"));
    assert!(content.contains("Chicken Adobo"));
}

#[test]
fn export_json_preserves_ingredients_and_steps() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    let output = fond(&tmp)
        .args(["export", "--recipe", "chicken-adobo"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let recipes = json["recipes"].as_array().unwrap();
    assert_eq!(recipes.len(), 1);

    let recipe = &recipes[0];
    let ingredients = recipe["ingredients"].as_array().unwrap();
    assert!(ingredients.len() >= 3);

    let names: Vec<&str> = ingredients
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"soy sauce"));
    assert!(names.contains(&"chicken thighs"));

    // Steps should be present
    let steps = recipe["steps"].as_array().unwrap();
    assert!(!steps.is_empty());
}

#[test]
fn export_json_preserves_tags() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    let output = fond(&tmp)
        .args(["export", "--recipe", "chicken-adobo"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let tags = json["recipes"][0]["tags"].as_array().unwrap();
    let tag_strs: Vec<&str> = tags.iter().map(|t| t.as_str().unwrap()).collect();
    assert!(tag_strs.contains(&"filipino"));
    assert!(tag_strs.contains(&"braised"));
}

#[test]
fn export_nonexistent_recipe_fails() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["export", "--recipe", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("recipe not found"));
}

#[test]
fn export_paprika_single_recipe() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    let out_path = tmp.path().join("chicken.paprikarecipe");
    fond(&tmp)
        .args([
            "export",
            "--export-format",
            "paprika",
            "--recipe",
            "chicken-adobo",
            "--output",
            out_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Verify it's valid gzip'd JSON
    let data = fs::read(&out_path).unwrap();
    let mut decoder = flate2::read::GzDecoder::new(&data[..]);
    let mut json_str = String::new();
    std::io::Read::read_to_string(&mut decoder, &mut json_str).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(parsed["name"], "Chicken Adobo");
}

#[test]
fn export_paprika_archive() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    write_fixture(
        &tmp,
        "toast.cook",
        "---\ntitle: Toast\n---\n\nPut @bread{2 slices} in the toaster.\n",
    );
    fond(&tmp).arg("reindex").assert().success();

    let out_path = tmp.path().join("recipes.paprikarecipes");
    fond(&tmp)
        .args([
            "export",
            "--export-format",
            "paprika",
            "--output",
            out_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Verify it's a valid ZIP with gzip'd entries
    let file = fs::File::open(&out_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    assert_eq!(archive.len(), 2);

    // Each entry should be valid gzip'd JSON
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut buf).unwrap();

        let mut decoder = flate2::read::GzDecoder::new(&buf[..]);
        let mut json_str = String::new();
        std::io::Read::read_to_string(&mut decoder, &mut json_str).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(parsed["name"].as_str().is_some());
    }
}

#[test]
fn export_paprika_roundtrip() {
    // Export as Paprika and verify it can be read back
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    let out_path = tmp.path().join("roundtrip.paprikarecipes");
    fond(&tmp)
        .args([
            "export",
            "--export-format",
            "paprika",
            "--output",
            out_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Verify we can read it back with the Paprika importer
    let data = fs::read(&out_path).unwrap();
    let (recipes, _errors) = fond_import::paprika::parse_paprikarecipes_archive(&data);
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0].name, "Chicken Adobo");
    assert!(
        recipes[0]
            .ingredients
            .as_ref()
            .unwrap()
            .contains("soy sauce")
    );
}

// ──────────────────────────────────────────────────────────────
// fond cook (timeline)
// ──────────────────────────────────────────────────────────────

/// A recipe with named timers and sections for rich timeline testing.
const ADOBO_RICH: &str = "\
---
title: Rich Chicken Adobo
servings: 4
tags:
  - filipino
---

Combine @soy sauce{1/2 cup}, @vinegar{1/2 cup}, and @garlic{6 cloves} in a bowl.

Add @chicken thighs{2 lbs} to the marinade. Cover and refrigerate for at least ~marinate{1 hour}.

Transfer everything to a dutch oven and bring to a boil over high heat.

Reduce heat to medium-low, cover, and simmer for ~{35 minutes} until chicken is cooked through.

Remove the chicken and increase heat. Reduce the sauce for ~{10 minutes}.

Return chicken to the pot and coat with sauce. Serve over @steamed rice{}.
";

#[test]
fn cook_timeline_table_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "rich-chicken-adobo.cook", ADOBO_RICH);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["cook", "rich-chicken-adobo", "--serve-at", "19:00"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Cooking Timeline: Rich Chicken Adobo",
        ))
        .stdout(predicate::str::contains("Serve at 19:00"))
        .stdout(predicate::str::contains("Start"))
        .stdout(predicate::str::contains("Duration"))
        .stdout(predicate::str::contains("Marinate"));
}

#[test]
fn cook_timeline_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "rich-chicken-adobo.cook", ADOBO_RICH);
    fond(&tmp).arg("reindex").assert().success();

    let output = fond(&tmp)
        .args([
            "cook",
            "rich-chicken-adobo",
            "--serve-at",
            "19:00",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["recipe_title"], "Rich Chicken Adobo");
    assert_eq!(json["recipe_slug"], "rich-chicken-adobo");

    let nodes = json["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 6);

    // Check that timed nodes have durations
    let marinate_node = &nodes[1];
    assert_eq!(marinate_node["duration"]["seconds"], 3600);
    assert_eq!(marinate_node["duration"]["source"], "Timer");

    // Verify timing totals
    assert!(json["total_active_seconds"].as_u64().unwrap() > 0);
    assert!(json["total_passive_seconds"].as_u64().unwrap() > 0);
    assert!(json["has_untimed_steps"].as_bool().unwrap());
}

#[test]
fn cook_timeline_unknown_recipe() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["cook", "nonexistent", "--serve-at", "19:00"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no recipe found"));
}

#[test]
fn cook_timeline_invalid_time_format() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "rich-chicken-adobo.cook", ADOBO_RICH);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["cook", "rich-chicken-adobo", "--serve-at", "not-a-time"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid time format"));
}

#[test]
fn cook_timeline_simple_recipe_no_timers() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    let simple = "\
---
title: Simple Salad
---

Chop @lettuce{1 head} and @tomatoes{2}.

Toss with @olive oil{2 tbsp} and @lemon juice{1 tbsp}.

Serve immediately.
";
    write_fixture(&tmp, "simple-salad.cook", simple);
    fond(&tmp).arg("reindex").assert().success();

    // Should succeed even with no timers (all untimed)
    fond(&tmp)
        .args(["cook", "simple-salad", "--serve-at", "12:00"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Cooking Timeline: Simple Salad"))
        .stdout(predicate::str::contains("unknown duration"));
}

#[test]
fn cook_timeline_shows_time_summary() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "rich-chicken-adobo.cook", ADOBO_RICH);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["cook", "rich-chicken-adobo", "--serve-at", "19:00"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Active:"))
        .stdout(predicate::str::contains("Passive:"));
}

#[test]
fn cook_plan_flag_gives_static_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "rich-chicken-adobo.cook", ADOBO_RICH);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args([
            "cook",
            "rich-chicken-adobo",
            "--serve-at",
            "19:00",
            "--plan",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Cooking Timeline: Rich Chicken Adobo",
        ));
}

#[test]
fn cook_no_serve_at_in_plan_mode_fails() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "rich-chicken-adobo.cook", ADOBO_RICH);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["cook", "rich-chicken-adobo", "--plan"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--serve-at is required"));
}

#[test]
fn cook_help_shows_cook_command() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp)
        .args(["cook", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("interactive cook mode"))
        .stdout(predicate::str::contains("--serve-at"))
        .stdout(predicate::str::contains("--plan"));
}

// ──────────────────────────────────────────────────────────────
// fond scale
// ──────────────────────────────────────────────────────────────

/// A recipe with diverse ingredient types for scaling tests.
const SCALING_RECIPE: &str = "\
---
title: Scaling Test
servings: 4
---

Mix @flour{2 cups} and @baking powder{1 tsp} in a bowl.

Add @butter{1/2 cup} and @salt{1/4 tsp} to the mixture.

Stir in @milk{1 cup} and @vanilla extract{1 tsp}.
";

#[test]
fn scale_to_multiplier() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "scaling-test.cook", SCALING_RECIPE);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["scale", "scaling-test", "--to", "2x"])
        .assert()
        .success()
        .stdout(predicate::str::contains("×2"))
        .stdout(predicate::str::contains("4 cups")) // flour scaled
        .stdout(predicate::str::contains("1 cup")) // butter scaled
        .stdout(predicate::str::contains("2 cup")); // milk scaled
}

#[test]
fn scale_to_servings() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "scaling-test.cook", SCALING_RECIPE);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["scale", "scaling-test", "--servings", "8"])
        .assert()
        .success()
        .stdout(predicate::str::contains("4 → 8")) // servings display
        .stdout(predicate::str::contains("4 cups")); // flour doubled
}

#[test]
fn scale_shows_warnings() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "scaling-test.cook", SCALING_RECIPE);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["scale", "scaling-test", "--to", "3x"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Scaling Warnings"))
        .stdout(predicate::str::contains("baking powder"))
        .stdout(predicate::str::contains("salt"))
        .stdout(predicate::str::contains("vanilla extract"));
}

#[test]
fn scale_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "scaling-test.cook", SCALING_RECIPE);
    fond(&tmp).arg("reindex").assert().success();

    let out = fond(&tmp)
        .args(["scale", "scaling-test", "--to", "2", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(parsed["scale_factor"], 2.0);
    assert_eq!(parsed["title"], "Scaling Test");
    assert!(parsed["ingredients"].is_array());
    assert!(parsed["warnings"].is_array());
}

#[test]
fn scale_no_factor_fails() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "scaling-test.cook", SCALING_RECIPE);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["scale", "scaling-test"])
        .assert()
        .failure();
}

#[test]
fn scale_unknown_recipe_fails() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["scale", "nonexistent", "--to", "2x"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no recipe found"));
}

#[test]
fn scale_servings_without_metadata_fails() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    // Recipe without servings metadata
    let no_servings = "\
---
title: No Servings
---

Add @flour{1 cup}.
";
    write_fixture(&tmp, "no-servings.cook", no_servings);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["scale", "no-servings", "--servings", "8"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no servings metadata"));
}

#[test]
fn scale_half() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "scaling-test.cook", SCALING_RECIPE);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["scale", "scaling-test", "--to", "0.5x"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 cup")) // flour halved
        .stdout(predicate::str::contains("1/4 cup")); // butter halved
}

#[test]
fn scale_help() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp)
        .args(["scale", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--to"))
        .stdout(predicate::str::contains("--servings"))
        .stdout(predicate::str::contains("--rules"));
}

#[test]
fn scale_rules_adjusts_leavening_sublinearly() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "scaling-test.cook", SCALING_RECIPE);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["scale", "scaling-test", "--to", "2x", "--rules"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rule-based non-linear"))
        .stdout(predicate::str::contains("Adjustments"))
        .stdout(predicate::str::contains("sub-linear"))
        // Leavening is NOT the linear 2 tsp; linear reference is shown.
        .stdout(predicate::str::contains("(linear: 2 tsp)"))
        // Flour still scales linearly.
        .stdout(predicate::str::contains("4 cups"));
}

#[test]
fn scale_rules_seasoning_band_and_time_suggestion() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    // A recipe with a cook time to exercise the advisory suggestion.
    let timed = "\
---
title: Braise Test
servings: 4
cook_time: 2 hours
---

Season @beef chuck{2 lbs} with @salt{1 tsp} and simmer in @beef stock{3 cups}.
";
    write_fixture(&tmp, "braise-test.cook", timed);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["scale", "braise-test", "--to", "2x", "--rules"])
        .assert()
        .success()
        .stdout(predicate::str::contains("to-taste band"))
        .stdout(predicate::str::contains("Cook Time"))
        .stdout(predicate::str::contains("NOT auto-scaled"))
        .stdout(predicate::str::contains("Pan / Equipment"));
}

#[test]
fn scale_rules_json_has_new_fields() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "scaling-test.cook", SCALING_RECIPE);
    fond(&tmp).arg("reindex").assert().success();

    let out = fond(&tmp)
        .args(["scale", "scaling-test", "--to", "2", "--rules", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(parsed["rules_applied"], true);
    let bp = parsed["ingredients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "baking powder")
        .expect("baking powder present");
    assert!(bp["explanation"].is_string());
    assert_eq!(bp["linear_quantity"], "2");
}

#[test]
fn scale_without_rules_stays_linear() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "scaling-test.cook", SCALING_RECIPE);
    fond(&tmp).arg("reindex").assert().success();

    let out = fond(&tmp)
        .args(["scale", "scaling-test", "--to", "2", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(parsed["rules_applied"], false);
    let bp = parsed["ingredients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "baking powder")
        .unwrap();
    // Pure linear: baking powder doubles to 2 tsp, no rule fields present.
    assert_eq!(bp["scaled_quantity"], "2");
    assert!(bp.get("explanation").is_none());
    assert!(bp.get("linear_quantity").is_none());
}

// ──────────────────────────────────────────────────────────────
// fond note
// ──────────────────────────────────────────────────────────────

#[test]
fn note_add_and_list() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    // Add a note
    fond(&tmp)
        .args(["note", "chicken-adobo", "Used less vinegar"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Note added"));

    // List notes
    fond(&tmp)
        .args(["note", "chicken-adobo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Used less vinegar"));
}

#[test]
fn note_add_json() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    let out = fond(&tmp)
        .args(["note", "chicken-adobo", "Great dish!", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(parsed["note"], "Great dish!");
    assert!(parsed["id"].is_string());
}

#[test]
fn note_delete() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    // Add a note and get its ID
    let out = fond(&tmp)
        .args(["note", "chicken-adobo", "Delete me", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let note_id = parsed["id"].as_str().unwrap().to_string();

    // Delete it
    fond(&tmp)
        .args(["note", "chicken-adobo", "--delete", &note_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("deleted"));

    // List should be empty
    fond(&tmp)
        .args(["note", "chicken-adobo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No notes"));
}

#[test]
fn note_unknown_recipe_fails() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["note", "nonexistent", "Some text"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no recipe found"));
}

// ──────────────────────────────────────────────────────────────
// fond rate
// ──────────────────────────────────────────────────────────────

#[test]
fn rate_recipe() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["rate", "chicken-adobo", "4"])
        .assert()
        .success()
        .stdout(predicate::str::contains("★★★★☆"));
}

#[test]
fn rate_updates_on_rerate() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["rate", "chicken-adobo", "3"])
        .assert()
        .success();

    fond(&tmp)
        .args(["rate", "chicken-adobo", "5"])
        .assert()
        .success()
        .stdout(predicate::str::contains("★★★★★"));

    // Show should reflect the latest
    fond(&tmp)
        .args(["rate", "chicken-adobo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("★★★★★"));
}

#[test]
fn rate_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    let out = fond(&tmp)
        .args(["rate", "chicken-adobo", "4", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(parsed["score"], 4);
}

#[test]
fn rate_invalid_score_fails() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["rate", "chicken-adobo", "0"])
        .assert()
        .failure();

    fond(&tmp)
        .args(["rate", "chicken-adobo", "6"])
        .assert()
        .failure();
}

#[test]
fn rate_show_no_rating() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args(["rate", "chicken-adobo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No rating"));
}

// ──────────────────────────────────────────────────────────────
// fond scoreboard
// ──────────────────────────────────────────────────────────────

#[test]
fn scoreboard_empty() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .arg("scoreboard")
        .assert()
        .success()
        .stdout(predicate::str::contains("Most Cooked"))
        .stdout(predicate::str::contains("Highest Rated"))
        .stdout(predicate::str::contains("Recent Activity"));
}

#[test]
fn scoreboard_with_data() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "chicken-adobo.cook", CHICKEN_COOK);
    fond(&tmp).arg("reindex").assert().success();

    // Rate and add a note to populate scoreboard
    fond(&tmp)
        .args(["rate", "chicken-adobo", "5"])
        .assert()
        .success();

    fond(&tmp)
        .args(["note", "chicken-adobo", "Family favorite"])
        .assert()
        .success();

    fond(&tmp)
        .arg("scoreboard")
        .assert()
        .success()
        .stdout(predicate::str::contains("Chicken Adobo"))
        .stdout(predicate::str::contains("5.0/5"));
}

#[test]
fn scoreboard_json() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    let out = fond(&tmp)
        .args(["scoreboard", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert!(parsed["most_cooked"].is_array());
    assert!(parsed["highest_rated"].is_array());
    assert!(parsed["recent_activity"].is_array());
}

#[test]
fn scoreboard_since_filter() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["scoreboard", "--since", "2099-01-01"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No cook logs yet"))
        .stdout(predicate::str::contains("No ratings yet"))
        .stdout(predicate::str::contains("No activity yet"));
}

#[test]
fn scoreboard_help() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp)
        .args(["scoreboard", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--since"))
        .stdout(predicate::str::contains("--limit"));
}

// ──────────────────────────────────────────────────────────────
// substitute
// ──────────────────────────────────────────────────────────────

const PANCAKES_COOK: &str = "\
---
title: Buttermilk Pancakes
tags:
  - breakfast
  - baking
---

Mix @all-purpose flour{2%cups}, @baking powder{2%tsp}, @sugar{2%tbsp}, and @buttermilk{2%cups}.

Cook on a griddle.
";

#[test]
fn substitute_buttermilk_ranked_sourced_with_caveat() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp)
        .args(["substitute", "buttermilk"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Substitutions for: buttermilk"))
        .stdout(predicate::str::contains("milk + lemon juice"))
        .stdout(predicate::str::contains("King Arthur Baking Company"))
        .stdout(predicate::str::contains("baking soda"))
        .stdout(predicate::str::contains("Advisory only"));
}

#[test]
fn substitute_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp)
        .args(["--json", "substitute", "buttermilk"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"canonical\": \"buttermilk\""))
        .stdout(predicate::str::contains(
            "\"substitute\": \"milk + lemon juice\"",
        ))
        .stdout(predicate::str::contains("\"rank\": 1"))
        .stdout(predicate::str::contains("\"disclaimer\""));
}

#[test]
fn substitute_context_prioritizes_sauteing() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp)
        .args(["substitute", "butter", "--context", "sauteing"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Context: sauteing"))
        .stdout(predicate::str::contains("olive oil or neutral oil"));
}

#[test]
fn substitute_unknown_ingredient_lists_available() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp)
        .args(["substitute", "unobtanium"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No curated substitutions"))
        .stdout(predicate::str::contains("Available ingredients:"))
        .stdout(predicate::str::contains("buttermilk"));
}

#[test]
fn substitute_recipe_infers_baking_context() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "buttermilk-pancakes.cook", PANCAKES_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args([
            "substitute",
            "buttermilk",
            "--recipe",
            "buttermilk-pancakes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("inferred from recipe"))
        .stdout(predicate::str::contains("Baking notes:"));
}

#[test]
fn substitute_explicit_context_overrides_recipe() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    write_fixture(&tmp, "buttermilk-pancakes.cook", PANCAKES_COOK);
    fond(&tmp).arg("reindex").assert().success();

    fond(&tmp)
        .args([
            "substitute",
            "buttermilk",
            "--recipe",
            "buttermilk-pancakes",
            "--context",
            "general",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Context: general"))
        .stdout(predicate::str::contains("inferred").not());
}

#[test]
fn substitute_recipe_not_found_errors() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .args(["substitute", "buttermilk", "--recipe", "does-not-exist"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no recipe found"));
}

#[test]
fn substitute_help() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp)
        .args(["substitute", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--context"))
        .stdout(predicate::str::contains("--recipe"));
}

// ───────────────────────────────────────────────────────────────────
// doctor
// ───────────────────────────────────────────────────────────────────

#[test]
fn doctor_clean_dir_reports_ok() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();

    fond(&tmp)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("[ok]"))
        .stdout(predicate::str::contains("No file-sync tool detected"));
}

#[test]
fn doctor_warns_when_db_in_synced_folder() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    // Simulate a Syncthing-managed folder around the data dir.
    fs::create_dir_all(tmp.path().join(".stfolder")).unwrap();

    fond(&tmp)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("[warning]"))
        .stdout(predicate::str::contains("Syncthing"))
        .stdout(predicate::str::contains("fond reindex"));
}

#[test]
fn doctor_json_output() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    fs::create_dir_all(tmp.path().join(".stfolder")).unwrap();

    let output = fond(&tmp)
        .args(["--json", "doctor"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();

    assert_eq!(json["synced_folder_detected"], true);
    assert_eq!(json["ok"], false);
    assert_eq!(json["signals"][0]["tool"], "Syncthing");
}

// ═══════════════════════════════════════════════════════════════════
// share (community bundles — ADR-017)
// ═══════════════════════════════════════════════════════════════════

const SHARE_COOK: &str = "\
---
title: Shared Adobo
source url: https://example.com/adobo
servings: 4
---

Brown the @chicken{1%kg} in @soy sauce{60%ml}.
";

/// Export a bundle from a temp library and return its path.
fn export_bundle(tmp: &TempDir, out: &std::path::Path) {
    fond(tmp).arg("init").assert().success();
    write_fixture(tmp, "shared-adobo.cook", SHARE_COOK);
    fond(tmp).arg("reindex").assert().success();
    fond(tmp)
        .args(["share", "export", "--recipe", "shared-adobo"])
        .args(["--license", "CC-BY-4.0", "--author", "alice"])
        .arg("-o")
        .arg(out)
        .assert()
        .success();
}

#[test]
fn share_export_requires_target() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    fond(&tmp)
        .args(["share", "export"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--recipe").or(predicate::str::contains("--all")));
}

#[test]
fn share_export_stamps_provenance_losslessly() {
    let tmp = TempDir::new().unwrap();
    let bundle = tmp.path().join("adobo.fondshare");
    export_bundle(&tmp, &bundle);
    assert!(bundle.exists());

    // Read the .cook back out of the zip and confirm provenance + fidelity.
    let file = fs::File::open(&bundle).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut cook = String::new();
    {
        use std::io::Read;
        archive
            .by_name("recipes/shared-adobo.cook")
            .unwrap()
            .read_to_string(&mut cook)
            .unwrap();
    }
    assert!(cook.contains("license: CC-BY-4.0"));
    assert!(cook.contains("shared by: alice"));
    assert!(cook.contains("source url: https://example.com/adobo"));
    // Original content untouched.
    assert!(cook.contains("@chicken{1%kg}"));
    assert!(cook.contains("servings: 4"));
}

#[test]
fn share_inspect_shows_attribution() {
    let tmp = TempDir::new().unwrap();
    let bundle = tmp.path().join("adobo.fondshare");
    export_bundle(&tmp, &bundle);

    fond(&tmp)
        .args(["share", "inspect"])
        .arg(&bundle)
        .assert()
        .success()
        .stdout(predicate::str::contains("Shared Adobo"))
        .stdout(predicate::str::contains("CC-BY-4.0"));
}

#[test]
fn share_import_queues_for_review_with_attribution() {
    let src = TempDir::new().unwrap();
    let bundle = src.path().join("adobo.fondshare");
    export_bundle(&src, &bundle);

    // Import into a fresh, separate library.
    let dst = TempDir::new().unwrap();
    fond(&dst).arg("init").assert().success();
    fond(&dst)
        .args(["share", "import"])
        .arg(&bundle)
        .assert()
        .success()
        .stdout(predicate::str::contains("Queued"));

    // The recipe is NOT written directly — it waits in the review queue.
    fond(&dst).args(["view", "shared-adobo"]).assert().failure();

    // Accept it through the normal review pipeline.
    let output = fond(&dst)
        .args(["--json", "review", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let id = json[0]["id"].as_str().unwrap().to_string();
    assert_eq!(json[0]["source_type"], "shared-bundle");

    fond(&dst)
        .args(["review", "accept", &id])
        .assert()
        .success();

    // Now it exists, with attribution preserved in the file.
    fond(&dst).args(["view", "shared-adobo"]).assert().success();
    let cook = fs::read_to_string(dst.path().join("recipes").join("shared-adobo.cook")).unwrap();
    assert!(cook.contains("source url: https://example.com/adobo"));
    assert!(cook.contains("license: CC-BY-4.0"));
}

#[test]
fn share_import_is_idempotent() {
    let src = TempDir::new().unwrap();
    let bundle = src.path().join("adobo.fondshare");
    export_bundle(&src, &bundle);

    let dst = TempDir::new().unwrap();
    fond(&dst).arg("init").assert().success();
    fond(&dst)
        .args(["share", "import"])
        .arg(&bundle)
        .assert()
        .success();

    // Second import finds it already queued and skips it.
    fond(&dst)
        .args(["share", "import"])
        .arg(&bundle)
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipped"));
}

#[test]
fn share_import_dry_run_writes_nothing() {
    let src = TempDir::new().unwrap();
    let bundle = src.path().join("adobo.fondshare");
    export_bundle(&src, &bundle);

    let dst = TempDir::new().unwrap();
    fond(&dst).arg("init").assert().success();
    fond(&dst)
        .args(["share", "import", "--dry-run"])
        .arg(&bundle)
        .assert()
        .success()
        .stdout(predicate::str::contains("[dry-run]"));

    // Nothing landed in the review queue.
    fond(&dst)
        .args(["review", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No queued review drafts"));
}

#[test]
fn share_publish_requires_consent_and_writes_index() {
    let tmp = TempDir::new().unwrap();
    let bundle = tmp.path().join("adobo.fondshare");
    export_bundle(&tmp, &bundle);

    let outbox = tmp.path().join("index");

    // Without --yes and no interactive stdin, publish aborts and writes nothing.
    fond(&tmp)
        .args(["share", "publish"])
        .arg(&bundle)
        .args(["--to"])
        .arg(&outbox)
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::str::contains("Aborted"));
    assert!(!outbox.join("adobo.fondshare").exists());

    // With explicit consent it copies into the static index.
    fond(&tmp)
        .args(["share", "publish"])
        .arg(&bundle)
        .args(["--to"])
        .arg(&outbox)
        .arg("--yes")
        .assert()
        .success();
    assert!(outbox.join("adobo.fondshare").exists());
}

#[test]
fn share_import_rejects_non_bundle() {
    let tmp = TempDir::new().unwrap();
    fond(&tmp).arg("init").assert().success();
    let bogus = tmp.path().join("not-a-bundle.fondshare");
    fs::write(&bogus, b"not a zip").unwrap();
    fond(&tmp)
        .args(["share", "import"])
        .arg(&bogus)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a valid .fondshare bundle"));
}
