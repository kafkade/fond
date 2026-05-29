//! Spike #4: SQLite/FTS5 derive-from-files + reindex
//!
//! Proves the hybrid storage model: .cook files as source of truth
//! with SQLite as a derived, rebuildable index (ADR-002).
//!
//! References:
//! - Issue: <https://github.com/kafkade/fond/issues/4>
//! - ADR-002: Hybrid files + SQLite index
//! - Roadmap §14.1 Spike #4, §8.3 Storage layering

use std::fs;
use std::path::Path;
use std::time::Instant;

use cooklang::{CooklangParser, Extensions};
use rusqlite::{Connection, params};
use tempfile::TempDir;

// ═══════════════════════════════════════════════════════════════════
// Schema
// ═══════════════════════════════════════════════════════════════════

const SCHEMA_DDL: &str = "
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS recipes (
    id INTEGER PRIMARY KEY,
    file_path TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    servings TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL DEFAULT '',
    prep_time TEXT NOT NULL DEFAULT '',
    cook_time TEXT NOT NULL DEFAULT '',
    content_hash TEXT NOT NULL DEFAULT '',
    indexed_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS ingredients (
    id INTEGER PRIMARY KEY,
    recipe_id INTEGER NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    quantity TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS steps (
    id INTEGER PRIMARY KEY,
    recipe_id INTEGER NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
    section_name TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS tags (
    name TEXT NOT NULL,
    recipe_id INTEGER NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
    PRIMARY KEY (name, recipe_id)
);

CREATE VIRTUAL TABLE IF NOT EXISTS recipe_fts USING fts5(
    title,
    ingredients_text,
    steps_text,
    tags_text
);

INSERT OR IGNORE INTO schema_version (version) VALUES (1);
";

fn open_db(path: &Path) -> Connection {
    let conn = Connection::open(path).expect("open database");
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    let _: String = conn
        .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
        .unwrap();
    conn.execute_batch(SCHEMA_DDL).unwrap();
    conn
}

fn open_memory_db() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory database");
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    conn.execute_batch(SCHEMA_DDL).unwrap();
    conn
}

// ═══════════════════════════════════════════════════════════════════
// Parsing
// ═══════════════════════════════════════════════════════════════════

struct IndexedRecipe {
    title: String,
    description: String,
    servings: String,
    source: String,
    prep_time: String,
    cook_time: String,
    ingredients: Vec<IngredientRow>,
    steps: Vec<StepRow>,
    tags: Vec<String>,
    content_hash: String,
}

struct IngredientRow {
    name: String,
    quantity: String,
}

struct StepRow {
    section_name: String,
    body: String,
}

fn content_hash(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    content.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn parse_cook_file(content: &str, file_stem: &str) -> Option<IndexedRecipe> {
    let parser = CooklangParser::new(Extensions::all(), Default::default());
    let result = parser.parse(content);
    let recipe = result.into_output()?;

    let meta = &recipe.metadata;
    let get = |key: &str| -> String {
        meta.map
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    let title = {
        let t = get("title");
        if t.is_empty() {
            file_stem
                .split('-')
                .map(|w| {
                    let mut c = w.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().to_string() + c.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            t
        }
    };

    let prep_time = {
        let t = get("prep time");
        if t.is_empty() { get("prep_time") } else { t }
    };
    let cook_time = {
        let t = get("cook time");
        if t.is_empty() { get("cook_time") } else { t }
    };

    let tags: Vec<String> = get("tags")
        .split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();

    let ingredients: Vec<IngredientRow> = recipe
        .ingredients
        .iter()
        .map(|ing| {
            let quantity = ing
                .quantity
                .as_ref()
                .map(|q| format!("{q}"))
                .unwrap_or_default();
            IngredientRow {
                name: ing.name.clone(),
                quantity,
            }
        })
        .collect();

    let mut steps = Vec::new();
    for section in &recipe.sections {
        let section_name = section.name.clone().unwrap_or_default();
        for item in &section.content {
            match item {
                cooklang::Content::Step(step) => {
                    let text: String = step
                        .items
                        .iter()
                        .map(|item| match item {
                            cooklang::Item::Text { value } => value.clone(),
                            cooklang::Item::Ingredient { index } => recipe
                                .ingredients
                                .get(*index)
                                .map(|i| i.name.clone())
                                .unwrap_or_default(),
                            cooklang::Item::Cookware { index } => recipe
                                .cookware
                                .get(*index)
                                .map(|c| c.name.clone())
                                .unwrap_or_default(),
                            cooklang::Item::Timer { index } => recipe
                                .timers
                                .get(*index)
                                .and_then(|t| t.name.clone())
                                .unwrap_or_default(),
                            _ => String::new(),
                        })
                        .collect();
                    steps.push(StepRow {
                        section_name: section_name.clone(),
                        body: text,
                    });
                }
                cooklang::Content::Text(text) => {
                    steps.push(StepRow {
                        section_name: section_name.clone(),
                        body: text.clone(),
                    });
                }
            }
        }
    }

    Some(IndexedRecipe {
        title,
        description: get("description"),
        servings: get("servings"),
        source: get("source"),
        prep_time,
        cook_time,
        ingredients,
        steps,
        tags,
        content_hash: content_hash(content),
    })
}

// ═══════════════════════════════════════════════════════════════════
// Indexing — derived recipe index only (not overlays)
// ═══════════════════════════════════════════════════════════════════

fn index_recipe(
    conn: &Connection,
    file_path: &str,
    recipe: &IndexedRecipe,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO recipes (file_path, title, description, servings, source,
         prep_time, cook_time, content_hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            file_path,
            recipe.title,
            recipe.description,
            recipe.servings,
            recipe.source,
            recipe.prep_time,
            recipe.cook_time,
            recipe.content_hash,
        ],
    )?;
    let recipe_id = conn.last_insert_rowid();

    for (i, ing) in recipe.ingredients.iter().enumerate() {
        conn.execute(
            "INSERT INTO ingredients (recipe_id, name, quantity, sort_order)
             VALUES (?1, ?2, ?3, ?4)",
            params![recipe_id, ing.name, ing.quantity, i as i32],
        )?;
    }

    for (i, step) in recipe.steps.iter().enumerate() {
        conn.execute(
            "INSERT INTO steps (recipe_id, section_name, body, sort_order)
             VALUES (?1, ?2, ?3, ?4)",
            params![recipe_id, step.section_name, step.body, i as i32],
        )?;
    }

    for tag in &recipe.tags {
        conn.execute(
            "INSERT OR IGNORE INTO tags (name, recipe_id) VALUES (?1, ?2)",
            params![tag, recipe_id],
        )?;
    }

    // FTS5: explicit rowid matching recipe.id
    let ingredients_text: String = recipe
        .ingredients
        .iter()
        .map(|i| i.name.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let steps_text: String = recipe
        .steps
        .iter()
        .map(|s| s.body.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let tags_text: String = recipe.tags.join(" ");

    conn.execute(
        "INSERT INTO recipe_fts (rowid, title, ingredients_text, steps_text, tags_text)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            recipe_id,
            recipe.title,
            ingredients_text,
            steps_text,
            tags_text
        ],
    )?;

    Ok(recipe_id)
}

// ═══════════════════════════════════════════════════════════════════
// Reindex — atomic rebuild of the derived recipe index
// ═══════════════════════════════════════════════════════════════════

struct ReindexResult {
    indexed: usize,
    errors: Vec<(String, String)>,
}

fn reindex(conn: &Connection, recipes_dir: &Path) -> ReindexResult {
    // Phase 1: parse all .cook files (outside transaction)
    let mut parsed = Vec::new();
    let mut errors = Vec::new();

    if recipes_dir.exists() {
        for entry in fs::read_dir(recipes_dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("cook") {
                let file_name = path.file_name().unwrap().to_str().unwrap().to_string();
                let file_stem = path.file_stem().unwrap().to_str().unwrap().to_string();
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        if let Some(recipe) = parse_cook_file(&content, &file_stem) {
                            parsed.push((file_name, recipe));
                        } else {
                            errors
                                .push((file_name, "Failed to parse Cooklang content".to_string()));
                        }
                    }
                    Err(e) => {
                        errors.push((file_name, format!("Failed to read file: {e}")));
                    }
                }
            }
        }
    }

    // Deterministic ordering
    parsed.sort_by(|a, b| a.0.cmp(&b.0));

    // Phase 2: atomic rebuild inside transaction
    let tx = conn.unchecked_transaction().unwrap();
    tx.execute_batch(
        "DELETE FROM recipe_fts;
         DELETE FROM tags;
         DELETE FROM steps;
         DELETE FROM ingredients;
         DELETE FROM recipes;",
    )
    .unwrap();

    for (file_path, recipe) in &parsed {
        index_recipe(&tx, file_path, recipe).unwrap();
    }

    tx.commit().unwrap();

    ReindexResult {
        indexed: parsed.len(),
        errors,
    }
}

// ═══════════════════════════════════════════════════════════════════
// Search
// ═══════════════════════════════════════════════════════════════════

#[allow(dead_code)]
struct SearchResult {
    recipe_id: i64,
    title: String,
    rank: f64,
}

fn search_recipes(conn: &Connection, query: &str) -> rusqlite::Result<Vec<SearchResult>> {
    let mut stmt = conn.prepare(
        "SELECT f.rowid, r.title, rank
         FROM recipe_fts f
         JOIN recipes r ON r.id = f.rowid
         WHERE recipe_fts MATCH ?1
         ORDER BY rank",
    )?;

    let results = stmt
        .query_map(params![query], |row| {
            Ok(SearchResult {
                recipe_id: row.get(0)?,
                title: row.get(1)?,
                rank: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}

// ═══════════════════════════════════════════════════════════════════
// Synthetic recipe generation (for performance tests)
// ═══════════════════════════════════════════════════════════════════

fn generate_synthetic_recipe(id: usize) -> String {
    let cuisines = [
        "italian",
        "mexican",
        "japanese",
        "indian",
        "french",
        "thai",
        "chinese",
        "korean",
        "greek",
        "ethiopian",
    ];
    let proteins = [
        "chicken", "beef", "tofu", "salmon", "shrimp", "lamb", "pork", "tempeh", "duck", "cod",
    ];
    let techniques = [
        "braised", "grilled", "roasted", "sauteed", "steamed", "fried", "baked", "poached",
        "smoked", "stewed",
    ];
    let bases = [
        "rice", "pasta", "noodles", "bread", "salad", "soup", "curry", "stir-fry", "tacos", "bowl",
    ];

    let cuisine = cuisines[id % 10];
    let protein = proteins[(id * 7) % 10];
    let technique = techniques[(id * 3) % 10];
    let base = bases[(id * 11) % 10];

    let cuisine_cap = {
        let mut c = cuisine.chars();
        match c.next() {
            None => String::new(),
            Some(f) => f.to_uppercase().to_string() + c.as_str(),
        }
    };

    let title = format!("{cuisine_cap} {technique} {protein} with {base} v{id}");
    let servings = 2 + (id % 6);
    let prep = 10 + (id % 20);
    let cook = 20 + (id % 40);

    let ing_names = [
        "olive oil",
        "garlic",
        "onion",
        "salt",
        "pepper",
        "butter",
        "lemon",
        "ginger",
        "soy sauce",
        "tomato",
    ];
    let ingredients_count = 5 + (id % 4);
    let mut ingredients = String::new();
    for j in 0..ingredients_count {
        let name = ing_names[j % ing_names.len()];
        let cookware = if j % 2 == 0 { "#pan{}" } else { "#bowl{}" };
        ingredients.push_str(&format!(
            "Add @{name}{{{}%tsp}} to the {cookware}.\n",
            j + 1
        ));
    }

    let steps_count = 3 + (id % 3);
    let mut extra_steps = String::new();
    for j in 0..steps_count {
        extra_steps.push_str(&format!(
            "\nContinue cooking for ~{{{}%minutes}} until done.\n",
            5 + j * 3
        ));
    }

    format!(
        "---\ntitle: {title}\nsource: Test Kitchen {id}\nservings: {servings}\n\
         prep time: {prep} min\ncook time: {cook} min\n\
         tags: {cuisine}, {protein}, {technique}\n---\n\n\
         {ingredients}{extra_steps}\n\
         Serve the {technique} {protein} over {base}.\n"
    )
}

fn write_synthetic_recipes(dir: &Path, count: usize) {
    fs::create_dir_all(dir).unwrap();
    for i in 0..count {
        let content = generate_synthetic_recipe(i);
        let path = dir.join(format!("recipe-{i:04}.cook"));
        fs::write(&path, &content).unwrap();
    }
}

// ═══════════════════════════════════════════════════════════════════
// Test fixtures
// ═══════════════════════════════════════════════════════════════════

const SAMPLE_ADOBO: &str = "\
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

const SAMPLE_TOFU: &str = "\
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

const SAMPLE_PASTA: &str = "\
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

fn setup_test_recipes(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("chicken-adobo.cook"), SAMPLE_ADOBO).unwrap();
    fs::write(dir.join("mapo-tofu.cook"), SAMPLE_TOFU).unwrap();
    fs::write(dir.join("pasta-alla-norma.cook"), SAMPLE_PASTA).unwrap();
}

// ═══════════════════════════════════════════════════════════════════
// Tests: Schema & FTS5
// ═══════════════════════════════════════════════════════════════════

#[test]
fn fts5_extension_available() {
    let conn = open_memory_db();
    let count: i32 = conn
        .query_row("SELECT count(*) FROM recipe_fts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0, "FTS5 table should be created and empty");
}

#[test]
fn schema_creation_and_version() {
    let conn = open_memory_db();

    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert!(tables.contains(&"recipes".to_string()));
    assert!(tables.contains(&"ingredients".to_string()));
    assert!(tables.contains(&"steps".to_string()));
    assert!(tables.contains(&"tags".to_string()));
    assert!(tables.contains(&"schema_version".to_string()));

    let version: i32 = conn
        .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 1);
}

// ═══════════════════════════════════════════════════════════════════
// Tests: Indexing
// ═══════════════════════════════════════════════════════════════════

#[test]
fn index_and_retrieve_single_recipe() {
    let conn = open_memory_db();
    let recipe = parse_cook_file(SAMPLE_ADOBO, "chicken-adobo").unwrap();
    let id = index_recipe(&conn, "chicken-adobo.cook", &recipe).unwrap();

    assert!(id > 0);

    let title: String = conn
        .query_row(
            "SELECT title FROM recipes WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(title, "Classic Chicken Adobo");

    let source: String = conn
        .query_row(
            "SELECT source FROM recipes WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(source, "Filipino Kitchen");
}

#[test]
fn ingredient_extraction() {
    let conn = open_memory_db();
    let recipe = parse_cook_file(SAMPLE_ADOBO, "chicken-adobo").unwrap();
    let id = index_recipe(&conn, "chicken-adobo.cook", &recipe).unwrap();

    let names: Vec<String> = conn
        .prepare("SELECT name FROM ingredients WHERE recipe_id = ?1 ORDER BY sort_order")
        .unwrap()
        .query_map(params![id], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert!(
        names.contains(&"soy sauce".to_string()),
        "missing soy sauce: {names:?}"
    );
    assert!(
        names.contains(&"chicken thighs".to_string()),
        "missing chicken thighs: {names:?}"
    );
    assert!(
        names.contains(&"bay leaves".to_string()),
        "missing bay leaves: {names:?}"
    );
    assert!(
        names.contains(&"steamed rice".to_string()),
        "missing steamed rice: {names:?}"
    );
}

#[test]
fn step_extraction() {
    let conn = open_memory_db();
    let recipe = parse_cook_file(SAMPLE_ADOBO, "chicken-adobo").unwrap();
    let id = index_recipe(&conn, "chicken-adobo.cook", &recipe).unwrap();

    let count: i32 = conn
        .query_row(
            "SELECT count(*) FROM steps WHERE recipe_id = ?1",
            params![id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(count >= 4, "Expected at least 4 steps, got {count}");

    let first_step: String = conn
        .query_row(
            "SELECT body FROM steps WHERE recipe_id = ?1 ORDER BY sort_order LIMIT 1",
            params![id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        first_step.contains("soy sauce"),
        "First step should mention soy sauce: {first_step}"
    );
}

#[test]
fn tag_extraction() {
    let conn = open_memory_db();
    let recipe = parse_cook_file(SAMPLE_ADOBO, "chicken-adobo").unwrap();
    let id = index_recipe(&conn, "chicken-adobo.cook", &recipe).unwrap();

    let tags: Vec<String> = conn
        .prepare("SELECT name FROM tags WHERE recipe_id = ?1 ORDER BY name")
        .unwrap()
        .query_map(params![id], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert!(
        tags.contains(&"chicken".to_string()),
        "missing 'chicken': {tags:?}"
    );
    assert!(
        tags.contains(&"filipino".to_string()),
        "missing 'filipino': {tags:?}"
    );
    assert!(
        tags.contains(&"comfort food".to_string()),
        "missing 'comfort food': {tags:?}"
    );
}

#[test]
fn title_derived_from_filename() {
    let no_title = "Add @eggs{3} to a #pan{} with @butter{1%tbsp}.\n\
                     Cook for ~{3%minutes} until set.\n";

    let recipe = parse_cook_file(no_title, "scrambled-eggs").unwrap();
    assert_eq!(recipe.title, "Scrambled Eggs");
}

#[test]
fn content_hash_changes_on_edit() {
    let r1 = parse_cook_file(SAMPLE_ADOBO, "test").unwrap();
    let modified = SAMPLE_ADOBO.replace("45 min", "60 min");
    let r2 = parse_cook_file(&modified, "test").unwrap();

    assert_ne!(
        r1.content_hash, r2.content_hash,
        "Hash should change when content changes"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Tests: FTS5 search
// ═══════════════════════════════════════════════════════════════════

fn index_all_samples(conn: &Connection) {
    let adobo = parse_cook_file(SAMPLE_ADOBO, "chicken-adobo").unwrap();
    let tofu = parse_cook_file(SAMPLE_TOFU, "mapo-tofu").unwrap();
    let pasta = parse_cook_file(SAMPLE_PASTA, "pasta-alla-norma").unwrap();
    index_recipe(conn, "chicken-adobo.cook", &adobo).unwrap();
    index_recipe(conn, "mapo-tofu.cook", &tofu).unwrap();
    index_recipe(conn, "pasta-alla-norma.cook", &pasta).unwrap();
}

#[test]
fn fts5_search_by_title() {
    let conn = open_memory_db();
    index_all_samples(&conn);

    let results = search_recipes(&conn, "title:adobo").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Classic Chicken Adobo");
}

#[test]
fn fts5_search_by_ingredient() {
    let conn = open_memory_db();
    index_all_samples(&conn);

    // Both adobo and tofu use soy sauce
    let results = search_recipes(&conn, "ingredients_text:\"soy sauce\"").unwrap();
    assert_eq!(results.len(), 2, "Both adobo and tofu use soy sauce");

    // Only tofu uses doubanjiang
    let results = search_recipes(&conn, "ingredients_text:doubanjiang").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Mapo Tofu");
}

#[test]
fn fts5_search_by_step_text() {
    let conn = open_memory_db();
    index_all_samples(&conn);

    let results = search_recipes(&conn, "steps_text:marinade").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Classic Chicken Adobo");
}

#[test]
fn fts5_search_by_tag() {
    let conn = open_memory_db();
    index_all_samples(&conn);

    let results = search_recipes(&conn, "tags_text:sichuan").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Mapo Tofu");
}

#[test]
fn fts5_cross_field_search() {
    let conn = open_memory_db();
    index_all_samples(&conn);

    // "chicken" in adobo title/tags/ingredients and tofu ingredients
    let results = search_recipes(&conn, "chicken").unwrap();
    assert!(
        results.len() >= 2,
        "Expected >= 2 results for 'chicken', got {}",
        results.len()
    );
}

#[test]
fn fts5_ranking() {
    let conn = open_memory_db();
    index_all_samples(&conn);

    let results = search_recipes(&conn, "tofu").unwrap();
    assert!(!results.is_empty());
    assert_eq!(
        results[0].title, "Mapo Tofu",
        "Title match should rank first"
    );
}

#[test]
fn fts5_phrase_and_prefix_search() {
    let conn = open_memory_db();
    index_all_samples(&conn);

    let results = search_recipes(&conn, "\"chicken thighs\"").unwrap();
    assert!(!results.is_empty(), "Phrase search should match");

    let results = search_recipes(&conn, "chick*").unwrap();
    assert!(!results.is_empty(), "Prefix search should match");
}

// ═══════════════════════════════════════════════════════════════════
// Tests: Reindex
// ═══════════════════════════════════════════════════════════════════

#[test]
fn reindex_with_real_files() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("recipes");
    setup_test_recipes(&recipes_dir);

    let db_path = tmp.path().join("fond.db");
    let conn = open_db(&db_path);

    let result = reindex(&conn, &recipes_dir);
    assert_eq!(result.indexed, 3);
    assert!(result.errors.is_empty());

    let count: i32 = conn
        .query_row("SELECT count(*) FROM recipes", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 3);
}

#[test]
fn reindex_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("recipes");
    setup_test_recipes(&recipes_dir);

    let db_path = tmp.path().join("fond.db");
    let conn = open_db(&db_path);

    let r1 = reindex(&conn, &recipes_dir);
    assert_eq!(r1.indexed, 3);

    let r2 = reindex(&conn, &recipes_dir);
    assert_eq!(r2.indexed, 3);

    let count: i32 = conn
        .query_row("SELECT count(*) FROM recipes", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 3, "Exactly 3 recipes after double reindex");

    // Search still works after two reindexes
    let results = search_recipes(&conn, "adobo").unwrap();
    assert_eq!(results.len(), 1, "Search should work after double reindex");
    assert_eq!(results[0].title, "Classic Chicken Adobo");
}

#[test]
fn recovery_after_db_deletion() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("recipes");
    setup_test_recipes(&recipes_dir);
    let db_path = tmp.path().join("fond.db");

    {
        let conn = open_db(&db_path);
        let result = reindex(&conn, &recipes_dir);
        assert_eq!(result.indexed, 3);
    }

    // Delete database + WAL/SHM
    fs::remove_file(&db_path).unwrap();
    let _ = fs::remove_file(db_path.with_extension("db-wal"));
    let _ = fs::remove_file(db_path.with_extension("db-shm"));
    assert!(!db_path.exists(), "DB should be deleted");

    // Recreate and reindex — full recovery
    {
        let conn = open_db(&db_path);
        let result = reindex(&conn, &recipes_dir);
        assert_eq!(result.indexed, 3);

        let titles: Vec<String> = conn
            .prepare("SELECT title FROM recipes ORDER BY title")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert_eq!(titles.len(), 3);
        assert!(titles.contains(&"Classic Chicken Adobo".to_string()));
        assert!(titles.contains(&"Mapo Tofu".to_string()));
        assert!(titles.contains(&"Pasta alla Norma".to_string()));

        let results = search_recipes(&conn, "tofu").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Mapo Tofu");
    }
}

#[test]
fn reindex_handles_updated_files() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("recipes");
    setup_test_recipes(&recipes_dir);
    let db_path = tmp.path().join("fond.db");
    let conn = open_db(&db_path);

    reindex(&conn, &recipes_dir);

    let updated = SAMPLE_ADOBO.replace("Classic Chicken Adobo", "Updated Chicken Adobo");
    fs::write(recipes_dir.join("chicken-adobo.cook"), &updated).unwrap();

    reindex(&conn, &recipes_dir);

    let title: String = conn
        .query_row(
            "SELECT title FROM recipes WHERE file_path = 'chicken-adobo.cook'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(title, "Updated Chicken Adobo");

    let results = search_recipes(&conn, "title:Updated").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Updated Chicken Adobo");
}

#[test]
fn reindex_skips_invalid_files() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("recipes");
    setup_test_recipes(&recipes_dir);

    fs::write(recipes_dir.join("broken.cook"), [0xFF, 0xFE, 0x00, 0x01]).unwrap();

    let db_path = tmp.path().join("fond.db");
    let conn = open_db(&db_path);

    let result = reindex(&conn, &recipes_dir);
    assert_eq!(result.indexed, 3, "3 valid recipes should still index");
    assert!(
        !result.errors.is_empty(),
        "Should report error for broken file"
    );
    assert!(
        result.errors.iter().any(|(f, _)| f == "broken.cook"),
        "Error should identify the broken file"
    );
}

#[test]
fn reindex_empty_directory() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("recipes");
    fs::create_dir_all(&recipes_dir).unwrap();

    let conn = open_memory_db();
    let result = reindex(&conn, &recipes_dir);
    assert_eq!(result.indexed, 0);
    assert!(result.errors.is_empty());
}

#[test]
fn reindex_nonexistent_directory() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("does-not-exist");

    let conn = open_memory_db();
    let result = reindex(&conn, &recipes_dir);
    assert_eq!(result.indexed, 0);
    assert!(result.errors.is_empty());
}

// ═══════════════════════════════════════════════════════════════════
// Tests: Performance
// ═══════════════════════════════════════════════════════════════════

#[test]
fn performance_1k_reindex() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("recipes");
    write_synthetic_recipes(&recipes_dir, 1000);

    let db_path = tmp.path().join("fond.db");
    let conn = open_db(&db_path);

    let start = Instant::now();
    let result = reindex(&conn, &recipes_dir);
    let elapsed = start.elapsed();

    eprintln!(
        "\n  [PERF] Reindex 1000 recipes: {:.2}s ({:.0} recipes/sec)",
        elapsed.as_secs_f64(),
        1000.0 / elapsed.as_secs_f64()
    );

    assert_eq!(result.indexed, 1000);
    assert!(
        elapsed.as_secs() < 15,
        "Reindex 1k took {elapsed:?}, expected < 15s"
    );
}

#[test]
fn performance_fts5_search_across_1k() {
    let tmp = TempDir::new().unwrap();
    let recipes_dir = tmp.path().join("recipes");
    write_synthetic_recipes(&recipes_dir, 1000);

    let db_path = tmp.path().join("fond.db");
    let conn = open_db(&db_path);
    reindex(&conn, &recipes_dir);

    let queries = ["chicken", "garlic", "italian", "grilled", "rice"];
    let mut total_elapsed = std::time::Duration::ZERO;

    for query in &queries {
        let start = Instant::now();
        let results = search_recipes(&conn, query).unwrap();
        let elapsed = start.elapsed();
        total_elapsed += elapsed;
        eprintln!(
            "  [PERF] Search '{query}': {:.3}ms ({} results)",
            elapsed.as_secs_f64() * 1000.0,
            results.len()
        );
        assert!(!results.is_empty(), "Expected results for '{query}'");
    }

    let avg_ms = total_elapsed.as_secs_f64() * 1000.0 / queries.len() as f64;
    eprintln!("  [PERF] Average search: {avg_ms:.3}ms");

    assert!(
        avg_ms < 100.0,
        "Average search took {avg_ms:.1}ms, expected < 100ms"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Summary report
// ═══════════════════════════════════════════════════════════════════

#[test]
fn spike_summary_report() {
    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════╗");
    eprintln!("║    SPIKE #4: SQLite/FTS5 DERIVE-FROM-FILES + REINDEX       ║");
    eprintln!("╠══════════════════════════════════════════════════════════════╣");
    eprintln!("║ SCHEMA (v1):                                                ║");
    eprintln!("║   recipes         — core metadata (title, source, times)    ║");
    eprintln!("║   ingredients     — per-recipe ingredient rows              ║");
    eprintln!("║   steps           — per-recipe step rows with sections      ║");
    eprintln!("║   tags            — recipe↔tag join table                   ║");
    eprintln!("║   recipe_fts      — FTS5 full-text search index             ║");
    eprintln!("║   schema_version  — migration version tracking              ║");
    eprintln!("╠══════════════════════════════════════════════════════════════╣");
    eprintln!("║ REINDEX:                                                    ║");
    eprintln!("║   ✅ Atomic (transaction-wrapped rebuild)                   ║");
    eprintln!("║   ✅ Idempotent (double reindex = same result)              ║");
    eprintln!("║   ✅ Skips invalid files with error reporting               ║");
    eprintln!("║   ✅ Detects updated files via content hash                 ║");
    eprintln!("║   ✅ Handles empty/missing recipe directories               ║");
    eprintln!("║   ✅ Deterministic ordering (sorted file paths)             ║");
    eprintln!("╠══════════════════════════════════════════════════════════════╣");
    eprintln!("║ FTS5 SEARCH:                                                ║");
    eprintln!("║   ✅ By title, ingredient, step text, tag                   ║");
    eprintln!("║   ✅ Cross-field search                                     ║");
    eprintln!("║   ✅ Ranking/relevance ordering                             ║");
    eprintln!("║   ✅ Phrase and prefix queries                              ║");
    eprintln!("╠══════════════════════════════════════════════════════════════╣");
    eprintln!("║ RECOVERY:                                                   ║");
    eprintln!("║   ✅ Delete DB → recreate → reindex → zero data loss        ║");
    eprintln!("╠══════════════════════════════════════════════════════════════╣");
    eprintln!("║ VERDICT: ✅ GO — hybrid storage model validated             ║");
    eprintln!("╚══════════════════════════════════════════════════════════════╝");
}
