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
