//! Tests for the recipe write path (file + index sync).

use fond_store::{
    FondDb, RecipeRepository, content_hash, delete_recipe, read_recipe_file, reindex,
    remove_old_file_after_rename, write_recipe_file,
};
use tempfile::TempDir;

const ADOBO: &str = "\
---
title: Chicken Adobo
servings: 4
tags:
  - chicken
---

Marinate @chicken thighs{1%kg} in @soy sauce{120%ml}.

Simmer for ~simmer{45%minutes}.
";

fn setup() -> (TempDir, FondDb, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let recipes = dir.path().join("recipes");
    std::fs::create_dir_all(&recipes).unwrap();
    let db = FondDb::open(&dir.path().join("fond.db")).unwrap();
    (dir, db, recipes)
}

#[test]
fn write_creates_file_and_indexes() {
    let (_dir, db, recipes) = setup();

    let result = write_recipe_file(&db, &recipes, "chicken-adobo.cook", ADOBO).unwrap();
    assert_eq!(result.recipe.title, "Chicken Adobo");
    assert_eq!(result.content_hash, content_hash(ADOBO));

    // File exists on disk with exact bytes.
    let on_disk = read_recipe_file(&recipes, "chicken-adobo.cook")
        .unwrap()
        .unwrap();
    assert_eq!(on_disk, ADOBO);

    // Indexed and queryable.
    let repo = RecipeRepository::new(&db);
    assert_eq!(repo.count_recipes().unwrap(), 1);
    let record = repo.get_recipe_by_slug("chicken-adobo").unwrap().unwrap();
    assert_eq!(record.content_hash, content_hash(ADOBO));
}

#[test]
fn overwriting_updates_index_in_place() {
    let (_dir, db, recipes) = setup();
    write_recipe_file(&db, &recipes, "chicken-adobo.cook", ADOBO).unwrap();

    let edited = ADOBO.replace("servings: 4", "servings: 8");
    write_recipe_file(&db, &recipes, "chicken-adobo.cook", &edited).unwrap();

    let repo = RecipeRepository::new(&db);
    assert_eq!(repo.count_recipes().unwrap(), 1);
    let record = repo.get_recipe_by_slug("chicken-adobo").unwrap().unwrap();
    assert_eq!(record.servings, "8");
    assert_eq!(record.content_hash, content_hash(&edited));
}

#[test]
fn delete_removes_file_and_index() {
    let (_dir, db, recipes) = setup();
    write_recipe_file(&db, &recipes, "chicken-adobo.cook", ADOBO).unwrap();

    assert!(delete_recipe(&db, &recipes, "chicken-adobo").unwrap());
    assert_eq!(RecipeRepository::new(&db).count_recipes().unwrap(), 0);
    assert!(
        read_recipe_file(&recipes, "chicken-adobo.cook")
            .unwrap()
            .is_none()
    );

    // Deleting again is a no-op.
    assert!(!delete_recipe(&db, &recipes, "chicken-adobo").unwrap());
}

#[test]
fn rename_cleans_up_old_file_and_row() {
    let (_dir, db, recipes) = setup();
    write_recipe_file(&db, &recipes, "chicken-adobo.cook", ADOBO).unwrap();

    // Write the renamed file, then clean up the old one.
    let renamed = ADOBO.replace("title: Chicken Adobo", "title: Weeknight Adobo");
    write_recipe_file(&db, &recipes, "weeknight-adobo.cook", &renamed).unwrap();
    remove_old_file_after_rename(&db, &recipes, "chicken-adobo", "chicken-adobo.cook").unwrap();

    let repo = RecipeRepository::new(&db);
    assert_eq!(repo.count_recipes().unwrap(), 1);
    assert!(
        repo.get_recipe_by_slug("weeknight-adobo")
            .unwrap()
            .is_some()
    );
    assert!(repo.get_recipe_by_slug("chicken-adobo").unwrap().is_none());
    assert!(
        read_recipe_file(&recipes, "chicken-adobo.cook")
            .unwrap()
            .is_none()
    );
}

#[test]
fn content_hash_matches_reindex_hash() {
    let (_dir, db, recipes) = setup();
    // Write via the write path, then reindex from scratch; the stored hash
    // must be identical either way.
    write_recipe_file(&db, &recipes, "chicken-adobo.cook", ADOBO).unwrap();
    let hash_after_write = RecipeRepository::new(&db)
        .get_recipe_by_slug("chicken-adobo")
        .unwrap()
        .unwrap()
        .content_hash;

    reindex(&db, &recipes).unwrap();
    let hash_after_reindex = RecipeRepository::new(&db)
        .get_recipe_by_slug("chicken-adobo")
        .unwrap()
        .unwrap()
        .content_hash;

    assert_eq!(hash_after_write, hash_after_reindex);
    assert_eq!(hash_after_write, content_hash(ADOBO));
}
