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

// ═══════════════════════════════════════════════════════════════════
// Editing / write-back
// ═══════════════════════════════════════════════════════════════════

use fond_ffi::{CookBlockDto, CookBlockKindDto, FondError, NewRecipeDto, SaveRecipeDto};

fn recipe_file(dir: &TempDir, name: &str) -> Option<String> {
    fs::read_to_string(dir.path().join("recipes").join(name)).ok()
}

#[test]
fn create_recipe_writes_file_and_indexes() {
    let dir = fixture();
    let c = client(&dir);

    let dto = NewRecipeDto {
        title: "Test Soup".to_string(),
        servings: Some("4".to_string()),
        tags: vec!["soup".to_string(), "quick".to_string()],
        description: Some("A quick test soup.".to_string()),
        source: None,
        steps: vec!["Simmer @water{1%L} with @salt{1%tsp}.".to_string()],
    };
    let created = c.create_recipe(dto).unwrap();
    assert_eq!(created.slug, "test-soup");

    // File exists on disk and re-parses to the same recipe.
    let on_disk = recipe_file(&dir, "test-soup.cook").expect("file written");
    assert!(on_disk.contains("title: Test Soup"));

    // Indexed and fetchable.
    let fetched = c.get_recipe("test-soup".to_string()).unwrap().unwrap();
    assert_eq!(fetched.servings.as_deref(), Some("4"));
    assert!(fetched.ingredients.iter().any(|i| i.name == "water"));
    assert_eq!(c.count_recipes().unwrap(), 3);
}

#[test]
fn create_recipe_rejects_duplicate_slug() {
    let dir = fixture();
    let c = client(&dir);
    let dto = NewRecipeDto {
        title: "Chicken Adobo".to_string(),
        servings: None,
        tags: vec![],
        description: None,
        source: None,
        steps: vec!["Do a thing.".to_string()],
    };
    let err = c.create_recipe(dto);
    assert!(matches!(err, Err(FondError::AlreadyExists { .. })));
}

#[test]
fn get_recipe_for_edit_exposes_raw_blocks_and_hash() {
    let dir = fixture();
    let c = client(&dir);
    let editor = c
        .get_recipe_for_edit("chicken-adobo".to_string())
        .unwrap()
        .unwrap();

    assert_eq!(editor.title, "Chicken Adobo");
    assert!(!editor.content_hash.is_empty());
    // Raw block text keeps inline Cooklang markup (unlike RecipeDto steps).
    assert!(
        editor
            .blocks
            .iter()
            .any(|b| b.text.contains("@chicken thighs{1%kg}"))
    );
    // Parsed ingredient preview is present.
    assert!(
        editor
            .ingredients
            .iter()
            .any(|i| i.name == "chicken thighs")
    );
}

#[test]
fn save_recipe_metadata_reflects_on_disk_and_reparses() {
    let dir = fixture();
    let c = client(&dir);
    let editor = c
        .get_recipe_for_edit("chicken-adobo".to_string())
        .unwrap()
        .unwrap();

    let save = SaveRecipeDto {
        slug: "chicken-adobo".to_string(),
        base_content_hash: editor.content_hash,
        title: editor.title,
        servings: Some("8".to_string()),
        description: editor.description,
        source: editor.source,
        source_url: editor.source_url,
        prep_time: editor.prep_time,
        cook_time: editor.cook_time,
        total_time: editor.total_time,
        image: editor.image,
        tags: editor.tags,
        blocks: editor.blocks,
    };
    let saved = c.save_recipe(save).unwrap();
    assert_eq!(saved.servings.as_deref(), Some("8"));

    let on_disk = recipe_file(&dir, "chicken-adobo.cook").unwrap();
    assert!(on_disk.contains("servings: 8"));
    // No recipe count change; still re-parses.
    assert_eq!(c.count_recipes().unwrap(), 2);
}

#[test]
fn save_recipe_editing_a_step_updates_ingredients() {
    let dir = fixture();
    let c = client(&dir);
    let editor = c
        .get_recipe_for_edit("simple-scrambled-eggs".to_string())
        .unwrap()
        .unwrap();

    let mut blocks = editor.blocks.clone();
    blocks.push(CookBlockDto {
        kind: CookBlockKindDto::Step,
        text: "Garnish with @chives{1%tbsp}.".to_string(),
        section: None,
    });

    let save = SaveRecipeDto {
        slug: "simple-scrambled-eggs".to_string(),
        base_content_hash: editor.content_hash,
        title: editor.title,
        servings: editor.servings,
        description: editor.description,
        source: editor.source,
        source_url: editor.source_url,
        prep_time: editor.prep_time,
        cook_time: editor.cook_time,
        total_time: editor.total_time,
        image: editor.image,
        tags: editor.tags,
        blocks,
    };
    c.save_recipe(save).unwrap();

    let fetched = c
        .get_recipe("simple-scrambled-eggs".to_string())
        .unwrap()
        .unwrap();
    assert!(fetched.ingredients.iter().any(|i| i.name == "chives"));
}

#[test]
fn save_recipe_rejects_stale_base_hash() {
    let dir = fixture();
    let c = client(&dir);
    let editor = c
        .get_recipe_for_edit("chicken-adobo".to_string())
        .unwrap()
        .unwrap();

    let save = SaveRecipeDto {
        slug: "chicken-adobo".to_string(),
        base_content_hash: "deadbeefdeadbeef".to_string(), // stale
        title: editor.title,
        servings: Some("6".to_string()),
        description: editor.description,
        source: editor.source,
        source_url: editor.source_url,
        prep_time: editor.prep_time,
        cook_time: editor.cook_time,
        total_time: editor.total_time,
        image: editor.image,
        tags: editor.tags,
        blocks: editor.blocks,
    };
    let err = c.save_recipe(save);
    assert!(matches!(err, Err(FondError::Conflict { .. })));
}

#[test]
fn save_recipe_title_change_renames_file() {
    let dir = fixture();
    let c = client(&dir);
    let editor = c
        .get_recipe_for_edit("chicken-adobo".to_string())
        .unwrap()
        .unwrap();

    let save = SaveRecipeDto {
        slug: "chicken-adobo".to_string(),
        base_content_hash: editor.content_hash,
        title: "Weeknight Adobo".to_string(),
        servings: editor.servings,
        description: editor.description,
        source: editor.source,
        source_url: editor.source_url,
        prep_time: editor.prep_time,
        cook_time: editor.cook_time,
        total_time: editor.total_time,
        image: editor.image,
        tags: editor.tags,
        blocks: editor.blocks,
    };
    let saved = c.save_recipe(save).unwrap();
    assert_eq!(saved.slug, "weeknight-adobo");

    assert!(recipe_file(&dir, "weeknight-adobo.cook").is_some());
    assert!(recipe_file(&dir, "chicken-adobo.cook").is_none());
    assert!(c.get_recipe("chicken-adobo".to_string()).unwrap().is_none());
    assert!(
        c.get_recipe("weeknight-adobo".to_string())
            .unwrap()
            .is_some()
    );
    assert_eq!(c.count_recipes().unwrap(), 2);
}

#[test]
fn save_recipe_source_writes_raw() {
    let dir = fixture();
    let c = client(&dir);
    let editor = c
        .get_recipe_for_edit("chicken-adobo".to_string())
        .unwrap()
        .unwrap();

    let new_raw = editor.raw_source.replace("servings: 4", "servings: 10");
    let saved = c
        .save_recipe_source("chicken-adobo".to_string(), new_raw, editor.content_hash)
        .unwrap();
    assert_eq!(saved.servings.as_deref(), Some("10"));
    assert!(
        recipe_file(&dir, "chicken-adobo.cook")
            .unwrap()
            .contains("servings: 10")
    );
}

#[test]
fn attach_photo_stores_bytes_and_sets_image() {
    let dir = fixture();
    let c = client(&dir);
    let editor = c
        .get_recipe_for_edit("chicken-adobo".to_string())
        .unwrap()
        .unwrap();

    let rel = c
        .attach_photo(
            "chicken-adobo".to_string(),
            b"fake-jpeg-bytes".to_vec(),
            "JPG".to_string(),
            editor.content_hash,
        )
        .unwrap();
    assert!(rel.starts_with("photos/"));
    assert!(rel.ends_with(".jpg"));

    // Photo bytes are on disk.
    assert!(dir.path().join(&rel).exists());
    // The .cook frontmatter records the image link.
    assert!(
        recipe_file(&dir, "chicken-adobo.cook")
            .unwrap()
            .contains(&format!("image: {rel}"))
    );
    // And it survives a reload.
    let reloaded = c
        .get_recipe_for_edit("chicken-adobo".to_string())
        .unwrap()
        .unwrap();
    assert_eq!(reloaded.image.as_deref(), Some(rel.as_str()));
}

#[test]
fn delete_recipe_removes_file_and_index() {
    let dir = fixture();
    let c = client(&dir);
    assert!(c.delete_recipe("chicken-adobo".to_string()).unwrap());
    assert_eq!(c.count_recipes().unwrap(), 1);
    assert!(recipe_file(&dir, "chicken-adobo.cook").is_none());
    assert!(!c.delete_recipe("chicken-adobo".to_string()).unwrap());
}

#[test]
fn preview_ingredients_parses_inline_markup() {
    let dir = fixture();
    let c = client(&dir);
    let ings = c
        .preview_ingredients(vec![
            "Whisk @eggs{3} with @milk{50%ml}.".to_string(),
            "Season with @salt{1%pinch}.".to_string(),
        ])
        .unwrap();
    let names: Vec<_> = ings.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"eggs"));
    assert!(names.contains(&"milk"));
    assert!(names.contains(&"salt"));
}
