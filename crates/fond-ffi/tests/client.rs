//! Integration tests exercising the FFI surface end-to-end against a real,
//! temporary fond data directory.

use std::fs;

use fond_ffi::{FondClient, ScaleFactorDto};
use fond_store::{FondDb, reindex};
use tempfile::TempDir;

const ADOBO: &str = "\
---
title: Chicken Adobo
source: Lola
servings: 4
tags:
  - chicken
  - filipino
---

Marinate @chicken thighs{1%kg} in @soy sauce{120%ml} and @vinegar{120%ml}
for ~marinate{30%minutes}.

Brown the chicken in a #pan{} over medium-high heat.

Simmer covered for ~simmer{45%minutes} until tender.
";

const EGGS: &str = "\
---
title: Simple Scrambled Eggs
servings: 2
tags:
  - eggs
  - breakfast
---

Whisk @eggs{4} with a pinch of @salt{}.

Cook gently in a #nonstick pan{}, stirring, for ~{3%minutes}.
";

/// Build a temp data dir with a `recipes/` folder and a reindexed `fond.db`.
fn fixture() -> TempDir {
    let dir = TempDir::new().expect("temp dir");
    let recipes = dir.path().join("recipes");
    fs::create_dir_all(&recipes).unwrap();
    fs::write(recipes.join("chicken-adobo.cook"), ADOBO).unwrap();
    fs::write(recipes.join("simple-scrambled-eggs.cook"), EGGS).unwrap();

    let db = FondDb::open(&dir.path().join("fond.db")).expect("open db");
    let report = reindex(&db, &recipes).expect("reindex");
    assert_eq!(report.indexed, 2, "errors: {:?}", report.errors);
    dir
}

fn client(dir: &TempDir) -> std::sync::Arc<FondClient> {
    FondClient::new(dir.path().to_string_lossy().into_owned()).expect("client")
}

#[test]
fn opens_and_counts() {
    let dir = fixture();
    let c = client(&dir);
    assert_eq!(c.count_recipes().unwrap(), 2);
}

#[test]
fn reindex_rebuilds_from_files() {
    // Start from a data dir whose db has not been built yet.
    let dir = TempDir::new().unwrap();
    let recipes = dir.path().join("recipes");
    fs::create_dir_all(&recipes).unwrap();
    fs::write(recipes.join("chicken-adobo.cook"), ADOBO).unwrap();

    let c = client(&dir);
    assert_eq!(c.count_recipes().unwrap(), 0);

    let report = c.reindex().unwrap();
    assert_eq!(report.indexed, 1);
    assert!(report.errors.is_empty());
    assert_eq!(c.count_recipes().unwrap(), 1);
}

#[test]
fn lists_recipes_with_tags() {
    let dir = fixture();
    let c = client(&dir);
    let recipes = c.list_recipes(None).unwrap();
    assert_eq!(recipes.len(), 2);
    let adobo = recipes.iter().find(|r| r.slug == "chicken-adobo").unwrap();
    assert_eq!(adobo.title, "Chicken Adobo");
    assert!(adobo.tags.contains(&"filipino".to_string()));
}

#[test]
fn list_filter_by_tag() {
    let dir = fixture();
    let c = client(&dir);
    let filter = fond_ffi::RecipeFilterDto {
        tags: vec!["eggs".to_string()],
        max_time_minutes: None,
        source: None,
    };
    let recipes = c.list_recipes(Some(filter)).unwrap();
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0].slug, "simple-scrambled-eggs");
}

#[test]
fn search_matches_and_escapes() {
    let dir = fixture();
    let c = client(&dir);
    let hits = c.search("chicken".to_string(), None).unwrap();
    assert!(hits.iter().any(|h| h.slug == "chicken-adobo"));

    // FTS operator words must be escaped, not interpreted — should not error.
    let empty = c.search("   ".to_string(), None).unwrap();
    assert!(empty.is_empty());
}

#[test]
fn lists_tags() {
    let dir = fixture();
    let c = client(&dir);
    let tags = c.list_tags().unwrap();
    let names: Vec<_> = tags.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"chicken"));
    assert!(names.contains(&"eggs"));
}

#[test]
fn gets_full_recipe() {
    let dir = fixture();
    let c = client(&dir);
    let recipe = c.get_recipe("chicken-adobo".to_string()).unwrap().unwrap();
    assert_eq!(recipe.title, "Chicken Adobo");
    assert_eq!(recipe.servings.as_deref(), Some("4"));
    assert!(!recipe.ingredients.is_empty());
    assert!(!recipe.steps.is_empty());
    assert!(
        recipe
            .ingredients
            .iter()
            .any(|i| i.name == "chicken thighs")
    );
    // The marinate step carries a named timer.
    assert!(recipe.steps.iter().any(|s| !s.timers.is_empty()));
}

#[test]
fn unknown_recipe_is_none() {
    let dir = fixture();
    let c = client(&dir);
    assert!(
        c.get_recipe("does-not-exist".to_string())
            .unwrap()
            .is_none()
    );
}

#[test]
fn scales_by_multiplier() {
    let dir = fixture();
    let c = client(&dir);
    let scaled = c
        .scale_recipe(
            "chicken-adobo".to_string(),
            ScaleFactorDto::Multiplier { value: 2.0 },
            false,
        )
        .unwrap();
    assert_eq!(scaled.scale_factor, 2.0);
    assert!(!scaled.ingredients.is_empty());
    assert!(!scaled.rules_applied);
}

#[test]
fn scales_to_servings() {
    let dir = fixture();
    let c = client(&dir);
    let scaled = c
        .scale_recipe(
            "chicken-adobo".to_string(),
            ScaleFactorDto::ToServings { servings: 8 },
            false,
        )
        .unwrap();
    assert_eq!(scaled.scale_factor, 2.0);
}

#[test]
fn scales_with_rules() {
    let dir = fixture();
    let c = client(&dir);
    let scaled = c
        .scale_recipe(
            "chicken-adobo".to_string(),
            ScaleFactorDto::Multiplier { value: 2.0 },
            true,
        )
        .unwrap();
    assert!(scaled.rules_applied);
}

#[test]
fn builds_timeline() {
    let dir = fixture();
    let c = client(&dir);
    let timeline = c.build_timeline("chicken-adobo".to_string()).unwrap();
    assert_eq!(timeline.recipe_slug, "chicken-adobo");
    assert!(!timeline.nodes.is_empty());
}

#[test]
fn schedules_timeline_backward() {
    let dir = fixture();
    let c = client(&dir);
    let sched = c
        .schedule_timeline(
            "chicken-adobo".to_string(),
            "2026-01-31T18:30:00".to_string(),
        )
        .unwrap();
    assert_eq!(sched.serve_at, "2026-01-31T18:30:00");
    // Start time must be at or before serve time.
    assert!(sched.start_at <= sched.serve_at);
    assert!(!sched.nodes.is_empty());
}

#[test]
fn schedule_rejects_bad_datetime() {
    let dir = fixture();
    let c = client(&dir);
    let err = c.schedule_timeline("chicken-adobo".to_string(), "not-a-date".to_string());
    assert!(err.is_err());
}
