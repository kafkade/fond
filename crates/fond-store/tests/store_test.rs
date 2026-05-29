//! Integration tests for fond-store: migrations, repository, search, reindex.

use std::fs;
use std::path::Path;
use std::time::Instant;

use fond_domain::parse_cook;
use fond_store::{FondDb, RecipeRepository, reindex};
use tempfile::TempDir;

// ═══════════════════════════════════════════════════════════════════
// Fixtures
// ═══════════════════════════════════════════════════════════════════

const ADOBO: &str = "\
---
title: Classic Chicken Adobo
source: Filipino Kitchen
servings: 4
prep time: 15 min
cook time: 45 min
tags: chicken, filipino, comfort food
---

Combine @soy sauce{1/2%cup}, @white vinegar{1/2%cup}, @garlic{6%cloves}, \
and @bay leaves{3} in a #bowl{}.

Add @chicken thighs{2%lbs} to the marinade. Cover and refrigerate for \
~marinate{1%hour}.

Transfer everything to a #dutch oven{} and bring to a boil.

Reduce heat and simmer for ~{35%minutes} until chicken is cooked through.

Serve over @steamed rice{}.
";

const TOFU: &str = "\
---
title: Mapo Tofu
source: Serious Eats
servings: 4
tags: chinese, sichuan, spicy, tofu
---

Cut @firm tofu{14%oz} into cubes. Simmer in salted water for ~{10%minutes}.

Heat @vegetable oil{2%tbsp} in a #wok{}. Add @ground pork{8%oz} and \
cook for ~{5%minutes}.

Add @doubanjiang{2%tbsp} and @fermented black beans{1%tbsp}. \
Stir-fry for ~{1%minute}.

Add @chicken stock{1%cup} and @soy sauce{1%tbsp}. Bring to a boil.

Slide in the tofu. Simmer for ~{5%minutes}.

Mix @cornstarch{1%tbsp} with @water{2%tbsp}. Stir into the wok.

Garnish with @scallions{2} and @Sichuan peppercorn powder{1%tsp}.
";

const PASTA: &str = "\
---
title: Pasta alla Norma
source: Italian Grandma
servings: 4
tags: italian, pasta, eggplant, vegetarian
---

Slice @eggplant{2%medium} into rounds. Salt and drain for ~{30%minutes}.

Cook @pasta{1%lb} in #large pot{} of boiling @water{} with @salt{1%tbsp}.

Fry the eggplant in @olive oil{3%tbsp} in a #skillet{} until golden, \
~{4%minutes} per side.

Simmer @crushed tomatoes{28%oz} with @garlic{3%cloves} for ~{15%minutes}.

Toss pasta with sauce and eggplant. Top with @ricotta salata{1/2%cup} \
and @basil{}.
";

fn setup_recipes(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("chicken-adobo.cook"), ADOBO).unwrap();
    fs::write(dir.join("mapo-tofu.cook"), TOFU).unwrap();
    fs::write(dir.join("pasta-alla-norma.cook"), PASTA).unwrap();
}

fn generate_synthetic(dir: &Path, count: usize) {
    fs::create_dir_all(dir).unwrap();
    let proteins = ["chicken", "beef", "tofu", "salmon", "shrimp"];
    let techniques = ["braised", "grilled", "roasted", "sauteed", "steamed"];

    for i in 0..count {
        let protein = proteins[i % 5];
        let technique = techniques[(i * 3) % 5];
        let title = format!("{} {} v{i}", capitalize(technique), protein);
        let content = format!(
            "---\ntitle: {title}\nservings: {}\ntags: {protein}, {technique}\n---\n\n\
             Add @{protein}{{1%lb}} to a #pan{{}}.\nCook for ~{{{i}%minutes}}.\n",
            2 + i % 4,
        );
        fs::write(dir.join(format!("recipe-{i:04}.cook")), content).unwrap();
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Migration
// ═══════════════════════════════════════════════════════════════════

#[test]
fn migration_runs_on_memory_db() {
    let db = FondDb::open_memory().expect("should open and migrate");

    let tables: Vec<String> = db
        .conn()
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert!(
        tables.contains(&"recipes".to_string()),
        "missing recipes: {tables:?}"
    );
    assert!(tables.contains(&"recipe_ingredients".to_string()));
    assert!(tables.contains(&"steps".to_string()));
    assert!(tables.contains(&"tags".to_string()));
    assert!(tables.contains(&"cookware".to_string()));
    assert!(tables.contains(&"users".to_string()));
}

#[test]
fn migration_runs_on_file_db() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("fond.db");
    let db = FondDb::open(&db_path).expect("should open file db");

    let count: i64 = db
        .conn()
        .query_row("SELECT count(*) FROM recipes", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn fts5_table_exists() {
    let db = FondDb::open_memory().unwrap();
    let count: i64 = db
        .conn()
        .query_row("SELECT count(*) FROM recipe_fts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

// ═══════════════════════════════════════════════════════════════════
// Repository CRUD
// ═══════════════════════════════════════════════════════════════════

#[test]
fn upsert_and_retrieve_by_id() {
    let db = FondDb::open_memory().unwrap();
    let repo = RecipeRepository::new(&db);

    let recipe = parse_cook(ADOBO, "chicken-adobo").unwrap();
    let id = repo
        .upsert_recipe("chicken-adobo.cook", &recipe, "hash1")
        .unwrap();

    let record = repo
        .get_recipe_by_id(id)
        .unwrap()
        .expect("should find recipe");
    assert_eq!(record.title, "Classic Chicken Adobo");
    assert_eq!(record.slug, "classic-chicken-adobo");
    assert_eq!(record.file_path, "chicken-adobo.cook");
}

#[test]
fn upsert_and_retrieve_by_slug() {
    let db = FondDb::open_memory().unwrap();
    let repo = RecipeRepository::new(&db);

    let recipe = parse_cook(ADOBO, "chicken-adobo").unwrap();
    repo.upsert_recipe("chicken-adobo.cook", &recipe, "hash1")
        .unwrap();

    let record = repo
        .get_recipe_by_slug("classic-chicken-adobo")
        .unwrap()
        .expect("should find by slug");
    assert_eq!(record.title, "Classic Chicken Adobo");
}

#[test]
fn upsert_updates_existing() {
    let db = FondDb::open_memory().unwrap();
    let repo = RecipeRepository::new(&db);

    let recipe = parse_cook(ADOBO, "chicken-adobo").unwrap();
    let id1 = repo
        .upsert_recipe("chicken-adobo.cook", &recipe, "hash1")
        .unwrap();

    let modified = ADOBO.replace("Classic Chicken Adobo", "Updated Adobo");
    let recipe2 = parse_cook(&modified, "chicken-adobo").unwrap();
    let id2 = repo
        .upsert_recipe("chicken-adobo.cook", &recipe2, "hash2")
        .unwrap();

    assert_eq!(id1, id2, "Same file_path should keep the same id");

    let record = repo.get_recipe_by_id(id1).unwrap().unwrap();
    assert_eq!(record.title, "Updated Adobo");

    assert_eq!(repo.count_recipes().unwrap(), 1, "Should still be 1 recipe");
}

#[test]
fn list_recipes_returns_all() {
    let db = FondDb::open_memory().unwrap();
    let repo = RecipeRepository::new(&db);

    let adobo = parse_cook(ADOBO, "chicken-adobo").unwrap();
    let tofu = parse_cook(TOFU, "mapo-tofu").unwrap();
    repo.upsert_recipe("chicken-adobo.cook", &adobo, "h1")
        .unwrap();
    repo.upsert_recipe("mapo-tofu.cook", &tofu, "h2").unwrap();

    let list = repo.list_recipes().unwrap();
    assert_eq!(list.len(), 2);
}

#[test]
fn list_recipes_includes_tags() {
    let db = FondDb::open_memory().unwrap();
    let repo = RecipeRepository::new(&db);

    let adobo = parse_cook(ADOBO, "chicken-adobo").unwrap();
    repo.upsert_recipe("chicken-adobo.cook", &adobo, "h1")
        .unwrap();

    let list = repo.list_recipes().unwrap();
    assert!(!list[0].tags.is_empty(), "Tags should be populated");
    assert!(list[0].tags.contains(&"chicken".to_string()));
}

// ═══════════════════════════════════════════════════════════════════
// FTS5 Search
// ═══════════════════════════════════════════════════════════════════

fn index_all_samples(db: &FondDb) {
    let repo = RecipeRepository::new(db);
    let adobo = parse_cook(ADOBO, "chicken-adobo").unwrap();
    let tofu = parse_cook(TOFU, "mapo-tofu").unwrap();
    let pasta = parse_cook(PASTA, "pasta-alla-norma").unwrap();
    repo.upsert_recipe("chicken-adobo.cook", &adobo, "h1")
        .unwrap();
    repo.upsert_recipe("mapo-tofu.cook", &tofu, "h2").unwrap();
    repo.upsert_recipe("pasta-alla-norma.cook", &pasta, "h3")
        .unwrap();
}

#[test]
fn search_by_title() {
    let db = FondDb::open_memory().unwrap();
    index_all_samples(&db);
    let repo = RecipeRepository::new(&db);

    let results = repo.search("title:adobo").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Classic Chicken Adobo");
}

#[test]
fn search_by_ingredient() {
    let db = FondDb::open_memory().unwrap();
    index_all_samples(&db);
    let repo = RecipeRepository::new(&db);

    let results = repo.search("ingredients_text:doubanjiang").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Mapo Tofu");
}

#[test]
fn search_by_tag() {
    let db = FondDb::open_memory().unwrap();
    index_all_samples(&db);
    let repo = RecipeRepository::new(&db);

    let results = repo.search("tags_text:vegetarian").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Pasta alla Norma");
}

#[test]
fn search_cross_field() {
    let db = FondDb::open_memory().unwrap();
    index_all_samples(&db);
    let repo = RecipeRepository::new(&db);

    let results = repo.search("chicken").unwrap();
    assert!(results.len() >= 2, "chicken appears in adobo + tofu stock");
}

#[test]
fn search_returns_slug() {
    let db = FondDb::open_memory().unwrap();
    index_all_samples(&db);
    let repo = RecipeRepository::new(&db);

    let results = repo.search("title:adobo").unwrap();
    assert_eq!(results[0].slug, "classic-chicken-adobo");
}

// ═══════════════════════════════════════════════════════════════════
// Reindex
// ═══════════════════════════════════════════════════════════════════

#[test]
fn reindex_from_files() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("recipes");
    setup_recipes(&recipes_dir);

    let db_path = tmp.path().join("fond.db");
    let db = FondDb::open(&db_path).unwrap();

    let report = reindex(&db, &recipes_dir).unwrap();
    assert_eq!(report.indexed, 3);
    assert!(report.errors.is_empty());

    let repo = RecipeRepository::new(&db);
    assert_eq!(repo.count_recipes().unwrap(), 3);
}

#[test]
fn reindex_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("recipes");
    setup_recipes(&recipes_dir);

    let db_path = tmp.path().join("fond.db");
    let db = FondDb::open(&db_path).unwrap();

    let r1 = reindex(&db, &recipes_dir).unwrap();
    assert_eq!(r1.indexed, 3);

    let r2 = reindex(&db, &recipes_dir).unwrap();
    assert_eq!(r2.indexed, 3);

    let repo = RecipeRepository::new(&db);
    assert_eq!(repo.count_recipes().unwrap(), 3);

    // Search still works
    let results = repo.search("adobo").unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn reindex_handles_updated_files() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("recipes");
    setup_recipes(&recipes_dir);

    let db_path = tmp.path().join("fond.db");
    let db = FondDb::open(&db_path).unwrap();
    reindex(&db, &recipes_dir).unwrap();

    // Update a file
    let updated = ADOBO.replace("Classic Chicken Adobo", "Updated Adobo");
    fs::write(recipes_dir.join("chicken-adobo.cook"), &updated).unwrap();

    reindex(&db, &recipes_dir).unwrap();

    let repo = RecipeRepository::new(&db);
    let record = repo
        .get_recipe_by_path("chicken-adobo.cook")
        .unwrap()
        .unwrap();
    assert_eq!(record.title, "Updated Adobo");
}

#[test]
fn reindex_skips_invalid_files() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("recipes");
    setup_recipes(&recipes_dir);
    fs::write(recipes_dir.join("broken.cook"), [0xFF, 0xFE, 0x00]).unwrap();

    let db = FondDb::open_memory().unwrap();
    let report = reindex(&db, &recipes_dir).unwrap();

    assert_eq!(report.indexed, 3, "Valid recipes should still index");
    assert!(!report.errors.is_empty(), "Should report error");
    assert!(report.errors.iter().any(|(f, _)| f == "broken.cook"));
}

#[test]
fn reindex_empty_directory() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("recipes");
    fs::create_dir_all(&recipes_dir).unwrap();

    let db = FondDb::open_memory().unwrap();
    let report = reindex(&db, &recipes_dir).unwrap();
    assert_eq!(report.indexed, 0);
    assert!(report.errors.is_empty());
}

#[test]
fn reindex_missing_directory() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("does-not-exist");

    let db = FondDb::open_memory().unwrap();
    let report = reindex(&db, &recipes_dir).unwrap();
    assert_eq!(report.indexed, 0);
}

#[test]
fn reindex_preserves_overlay_tables() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("recipes");
    setup_recipes(&recipes_dir);

    let db_path = tmp.path().join("fond.db");
    let db = FondDb::open(&db_path).unwrap();

    // Insert a user (overlay data)
    db.conn()
        .execute(
            "INSERT INTO users (name) VALUES (?1)",
            rusqlite::params!["Alice"],
        )
        .unwrap();

    reindex(&db, &recipes_dir).unwrap();

    // User should still exist after reindex
    let name: String = db
        .conn()
        .query_row("SELECT name FROM users WHERE name = 'Alice'", [], |row| {
            row.get(0)
        })
        .expect("User should survive reindex");
    assert_eq!(name, "Alice");
}

// ═══════════════════════════════════════════════════════════════════
// Recovery: DB deletion + rebuild
// ═══════════════════════════════════════════════════════════════════

#[test]
fn recovery_after_db_deletion() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("recipes");
    setup_recipes(&recipes_dir);
    let db_path = tmp.path().join("fond.db");

    {
        let db = FondDb::open(&db_path).unwrap();
        let report = reindex(&db, &recipes_dir).unwrap();
        assert_eq!(report.indexed, 3);
    }

    // Delete DB
    fs::remove_file(&db_path).unwrap();
    let _ = fs::remove_file(db_path.with_extension("db-wal"));
    let _ = fs::remove_file(db_path.with_extension("db-shm"));

    // Rebuild
    let db = FondDb::open(&db_path).unwrap();
    let report = reindex(&db, &recipes_dir).unwrap();
    assert_eq!(report.indexed, 3);

    let repo = RecipeRepository::new(&db);
    let results = repo.search("tofu").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Mapo Tofu");
}

// ═══════════════════════════════════════════════════════════════════
// Performance
// ═══════════════════════════════════════════════════════════════════

#[test]
fn performance_1k_reindex() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("recipes");
    generate_synthetic(&recipes_dir, 1000);

    let db_path = tmp.path().join("fond.db");
    let db = FondDb::open(&db_path).unwrap();

    let start = Instant::now();
    let report = reindex(&db, &recipes_dir).unwrap();
    let elapsed = start.elapsed();

    eprintln!(
        "\n  [PERF] Reindex 1000 recipes: {:.2}s ({:.0} recipes/sec)",
        elapsed.as_secs_f64(),
        1000.0 / elapsed.as_secs_f64()
    );

    assert_eq!(report.indexed, 1000);
    assert!(
        elapsed.as_secs() < 15,
        "Reindex 1k took {elapsed:?}, expected < 15s"
    );
}

#[test]
fn performance_search_1k() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("recipes");
    generate_synthetic(&recipes_dir, 1000);

    let db_path = tmp.path().join("fond.db");
    let db = FondDb::open(&db_path).unwrap();
    reindex(&db, &recipes_dir).unwrap();

    let repo = RecipeRepository::new(&db);
    let queries = ["chicken", "grilled", "tofu"];
    let mut total = std::time::Duration::ZERO;

    for q in &queries {
        let start = Instant::now();
        let results = repo.search(q).unwrap();
        let elapsed = start.elapsed();
        total += elapsed;
        eprintln!(
            "  [PERF] Search '{q}': {:.3}ms ({} results)",
            elapsed.as_secs_f64() * 1000.0,
            results.len()
        );
        assert!(!results.is_empty());
    }

    let avg_ms = total.as_secs_f64() * 1000.0 / queries.len() as f64;
    eprintln!("  [PERF] Average search: {avg_ms:.3}ms");
    assert!(avg_ms < 100.0, "Search avg {avg_ms:.1}ms, expected < 100ms");
}
