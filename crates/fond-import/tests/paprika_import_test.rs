//! Integration tests for the Paprika import pipeline.
//!
//! These tests exercise the full flow: parsing an archive → converting
//! to domain recipes → generating .cook text → validating round-trip.

use std::io::Write;

use flate2::Compression;
use flate2::write::GzEncoder;
use zip::write::SimpleFileOptions;

use fond_import::paprika::{
    convert_paprika_batch, parse_paprikarecipe, parse_paprikarecipes_archive, read_paprika_file,
};

// ─────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────

fn gzip_json(json: &str) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(json.as_bytes()).unwrap();
    encoder.finish().unwrap()
}

fn create_archive(recipes: &[(&str, &str)]) -> Vec<u8> {
    let buf = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(buf);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    for (filename, json) in recipes {
        let gzipped = gzip_json(json);
        zip.start_file(filename.to_string(), options).unwrap();
        zip.write_all(&gzipped).unwrap();
    }

    zip.finish().unwrap().into_inner()
}

fn full_recipe_json() -> String {
    serde_json::json!({
        "name": "Classic Chicken Adobo",
        "uid": "A1B2C3D4-E5F6-7890-ABCD-EF1234567890",
        "description": "A tangy Filipino braised chicken dish",
        "ingredients": "2 lbs chicken thighs\n1/2 cup soy sauce\n1/2 cup white vinegar\n6 cloves garlic, crushed\n3 bay leaves\n1 tsp black peppercorns\n2 tbsp cooking oil\nsteamed rice, for serving",
        "directions": "Combine chicken, soy sauce, vinegar, garlic, bay leaves, and peppercorns in a bowl. Marinate for at least 1 hour.\nHeat oil in a Dutch oven over medium-high heat.\nRemove chicken from marinade, reserving liquid. Sear chicken until golden, about 3 minutes per side.\nPour in reserved marinade. Bring to a boil, then reduce to a simmer.\nCover and cook for 35 minutes until chicken is tender.\nUncover and cook 10 more minutes to reduce sauce.\nServe over steamed rice.",
        "notes": "For extra flavor, let the chicken marinate overnight in the fridge.\nSome versions add coconut milk for a creamier sauce.",
        "servings": "4",
        "prep_time": "15 min + marinating",
        "cook_time": "50 min",
        "total_time": "1 hr 5 min",
        "source": "Lola's Kitchen",
        "source_url": "https://example.com/chicken-adobo",
        "categories": ["Filipino", "Chicken", "Main Course"],
        "nutrition": "Calories: 450, Protein: 35g, Fat: 28g",
        "rating": 5,
        "difficulty": "Easy",
        "yield": "4 servings",
        "on_favorites": true,
        "created": "2024-03-15 10:30:00",
        "hash": "abc123def456"
    })
    .to_string()
}

fn minimal_recipe_json() -> String {
    serde_json::json!({
        "name": "Quick Scrambled Eggs"
    })
    .to_string()
}

fn recipe_with_sections_json() -> String {
    serde_json::json!({
        "name": "Birria Tacos",
        "uid": "SECTIONS-9012",
        "ingredients": "For the Birria:\n3 lbs beef chuck\n4 guajillo chiles\n2 ancho chiles\n1 onion\n\nFor the Consommé:\n4 cups beef broth\n1 tbsp oregano\n\nFor Assembly:\n12 corn tortillas\n1 cup Oaxaca cheese\ncilantro and onion for garnish",
        "directions": "Toast and rehydrate chiles.\nBlend chiles with onion and spices.\nBraise beef in chile sauce for 3 hours.\nStrain braising liquid for consommé.\nShred beef and fill tortillas with cheese.\nDip tortillas in consommé and griddle until crispy.",
        "categories": ["Mexican", "Tacos"],
        "rating": 5
    })
    .to_string()
}

// ─────────────────────────────────────────────────────────────────
// Archive parsing integration tests
// ─────────────────────────────────────────────────────────────────

#[test]
fn parse_archive_and_convert_batch() {
    let archive = create_archive(&[
        ("adobo.paprikarecipe", &full_recipe_json()),
        ("eggs.paprikarecipe", &minimal_recipe_json()),
        ("tacos.paprikarecipe", &recipe_with_sections_json()),
    ]);

    let (recipes, errors) = parse_paprikarecipes_archive(&archive);
    assert!(errors.is_empty());
    assert_eq!(recipes.len(), 3);

    let (prepared, report) = convert_paprika_batch(recipes, &[], &[]);

    assert_eq!(report.imported, 3);
    assert_eq!(report.skipped, 0);
    assert_eq!(report.failed, 0);
    assert_eq!(prepared.len(), 3);

    // Verify the chicken adobo recipe
    let adobo = prepared
        .iter()
        .find(|p| p.recipe.slug == "classic-chicken-adobo")
        .unwrap();
    assert_eq!(adobo.recipe.title, "Classic Chicken Adobo");
    assert_eq!(adobo.recipe.source.as_deref(), Some("Lola's Kitchen"));
    assert_eq!(adobo.recipe.ingredients.len(), 8);
    assert_eq!(adobo.recipe.steps.len(), 7);
    assert_eq!(
        adobo.recipe.tags,
        vec!["filipino", "chicken", "main course"]
    );
    assert_eq!(adobo.file_name, "classic-chicken-adobo.cook");

    // Verify .cook text contains essential elements
    assert!(adobo.cook_text.contains("title: Classic Chicken Adobo"));
    assert!(adobo.cook_text.contains("import source: paprika"));
    assert!(
        adobo
            .cook_text
            .contains("paprika uid: A1B2C3D4-E5F6-7890-ABCD-EF1234567890")
    );
    assert!(adobo.cook_text.contains("source: Lola's Kitchen"));
    assert!(adobo.cook_text.contains("Combine chicken"));
}

#[test]
fn parse_single_paprikarecipe_file() {
    let json = full_recipe_json();
    let gzipped = gzip_json(&json);
    let recipe = parse_paprikarecipe(&gzipped).unwrap();
    assert_eq!(recipe.name, "Classic Chicken Adobo");
}

// ─────────────────────────────────────────────────────────────────
// Duplicate detection
// ─────────────────────────────────────────────────────────────────

#[test]
fn duplicate_detection_by_source_url() {
    let archive = create_archive(&[
        (
            "r1.paprikarecipe",
            &serde_json::json!({
                "name": "Recipe One",
                "source_url": "https://example.com/recipe-one"
            })
            .to_string(),
        ),
        (
            "r2.paprikarecipe",
            &serde_json::json!({
                "name": "Recipe Two",
                "source_url": "https://example.com/recipe-two"
            })
            .to_string(),
        ),
    ]);

    let (recipes, _) = parse_paprikarecipes_archive(&archive);

    let existing_urls = vec!["https://example.com/recipe-one".to_string()];
    let (prepared, report) = convert_paprika_batch(recipes, &[], &existing_urls);

    assert_eq!(report.imported, 1);
    assert_eq!(report.skipped, 1);
    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0].recipe.title, "Recipe Two");
}

#[test]
fn slug_collision_gets_suffix() {
    let archive = create_archive(&[
        (
            "r1.paprikarecipe",
            &serde_json::json!({
                "name": "Chicken Soup",
                "uid": "UID-001"
            })
            .to_string(),
        ),
        (
            "r2.paprikarecipe",
            &serde_json::json!({
                "name": "Chicken Soup",
                "uid": "UID-002",
                "source_url": "https://other.com/soup"
            })
            .to_string(),
        ),
    ]);

    let (recipes, _) = parse_paprikarecipes_archive(&archive);
    let (prepared, report) = convert_paprika_batch(recipes, &[], &[]);

    assert_eq!(report.imported, 2);
    assert_eq!(prepared[0].file_name, "chicken-soup.cook");
    assert_eq!(prepared[1].file_name, "chicken-soup-2.cook");
}

#[test]
fn existing_slug_collision_gets_suffix() {
    let archive = create_archive(&[(
        "r1.paprikarecipe",
        &serde_json::json!({
            "name": "Chicken Soup"
        })
        .to_string(),
    )]);

    let (recipes, _) = parse_paprikarecipes_archive(&archive);
    let existing_slugs = vec!["chicken-soup".to_string()];
    let (prepared, report) = convert_paprika_batch(recipes, &existing_slugs, &[]);

    assert_eq!(report.imported, 1);
    assert_eq!(prepared[0].file_name, "chicken-soup-2.cook");
}

// ─────────────────────────────────────────────────────────────────
// Cook text round-trip validation
// ─────────────────────────────────────────────────────────────────

#[test]
fn cook_text_round_trips_through_parser() {
    let archive = create_archive(&[("adobo.paprikarecipe", &full_recipe_json())]);

    let (recipes, _) = parse_paprikarecipes_archive(&archive);
    let (prepared, _) = convert_paprika_batch(recipes, &[], &[]);

    let prep = &prepared[0];

    // Parse the generated .cook text
    let parsed = fond_domain::parse_cook(&prep.cook_text, &prep.recipe.slug).unwrap();

    assert_eq!(parsed.title, "Classic Chicken Adobo");
    assert_eq!(parsed.source.as_deref(), Some("Lola's Kitchen"));
    assert_eq!(
        parsed.source_url.as_deref(),
        Some("https://example.com/chicken-adobo")
    );
    assert_eq!(parsed.servings.as_deref(), Some("4"));
    assert_eq!(parsed.prep_time.as_deref(), Some("15 min + marinating"));
    assert_eq!(parsed.cook_time.as_deref(), Some("50 min"));
    assert_eq!(parsed.total_time.as_deref(), Some("1 hr 5 min"));

    // Tags survive
    assert!(parsed.tags.contains(&"filipino".to_string()));
    assert!(parsed.tags.contains(&"chicken".to_string()));
    assert!(parsed.tags.contains(&"main course".to_string()));
}

#[test]
fn cook_text_contains_all_ingredients() {
    let archive = create_archive(&[("adobo.paprikarecipe", &full_recipe_json())]);

    let (recipes, _) = parse_paprikarecipes_archive(&archive);
    let (prepared, _) = convert_paprika_batch(recipes, &[], &[]);

    let cook = &prepared[0].cook_text;

    // All 8 ingredients from the full recipe should be in the .cook text
    assert!(
        cook.contains("chicken thighs"),
        "missing chicken thighs: {cook}"
    );
    assert!(cook.contains("soy sauce"), "missing soy sauce: {cook}");
    assert!(
        cook.contains("white vinegar"),
        "missing white vinegar: {cook}"
    );
    assert!(cook.contains("garlic"), "missing garlic: {cook}");
    assert!(cook.contains("bay leaves"), "missing bay leaves: {cook}");
    assert!(
        cook.contains("black peppercorns"),
        "missing peppercorns: {cook}"
    );
    assert!(cook.contains("cooking oil"), "missing cooking oil: {cook}");
}

#[test]
fn cook_text_contains_notes_as_comments() {
    let archive = create_archive(&[("adobo.paprikarecipe", &full_recipe_json())]);

    let (recipes, _) = parse_paprikarecipes_archive(&archive);
    let (prepared, _) = convert_paprika_batch(recipes, &[], &[]);

    let cook = &prepared[0].cook_text;

    assert!(cook.contains("-- Notes --"), "should have notes section");
    assert!(
        cook.contains("-- For extra flavor"),
        "should contain first note: {cook}"
    );
}

// ─────────────────────────────────────────────────────────────────
// Ingredient section handling
// ─────────────────────────────────────────────────────────────────

#[test]
fn sections_in_ingredients_become_cooklang_sections() {
    let archive = create_archive(&[("tacos.paprikarecipe", &recipe_with_sections_json())]);

    let (recipes, _) = parse_paprikarecipes_archive(&archive);
    let (prepared, _) = convert_paprika_batch(recipes, &[], &[]);

    let cook = &prepared[0].cook_text;

    // Section headers should become Cooklang sections
    assert!(
        cook.contains("== For the Birria =="),
        "missing birria section: {cook}"
    );
    assert!(
        cook.contains("== For the Consommé =="),
        "missing consommé section: {cook}"
    );
    assert!(
        cook.contains("== For Assembly =="),
        "missing assembly section: {cook}"
    );
}

// ─────────────────────────────────────────────────────────────────
// File reading (via disk)
// ─────────────────────────────────────────────────────────────────

#[test]
fn read_paprikarecipes_file_from_disk() {
    let archive = create_archive(&[
        ("r1.paprikarecipe", &full_recipe_json()),
        ("r2.paprikarecipe", &minimal_recipe_json()),
    ]);

    let tmp = tempfile::NamedTempFile::with_suffix(".paprikarecipes").unwrap();
    std::fs::write(tmp.path(), &archive).unwrap();

    let (recipes, errors) = read_paprika_file(tmp.path()).unwrap();

    assert_eq!(recipes.len(), 2);
    assert!(errors.is_empty());
}

#[test]
fn read_single_paprikarecipe_from_disk() {
    let json = minimal_recipe_json();
    let gzipped = gzip_json(&json);

    let tmp = tempfile::NamedTempFile::with_suffix(".paprikarecipe").unwrap();
    std::fs::write(tmp.path(), &gzipped).unwrap();

    let (recipes, errors) = read_paprika_file(tmp.path()).unwrap();

    assert_eq!(recipes.len(), 1);
    assert!(errors.is_empty());
    assert_eq!(recipes[0].name, "Quick Scrambled Eggs");
}

#[test]
fn read_unsupported_extension_returns_error() {
    let tmp = tempfile::NamedTempFile::with_suffix(".txt").unwrap();
    std::fs::write(tmp.path(), "not a recipe").unwrap();

    let result = read_paprika_file(tmp.path());
    assert!(result.is_err());
}

// ─────────────────────────────────────────────────────────────────
// Performance
// ─────────────────────────────────────────────────────────────────

#[test]
fn import_500_recipes_under_10_seconds() {
    let mut entries: Vec<(String, String)> = Vec::with_capacity(500);
    for i in 0..500 {
        let json = serde_json::json!({
            "name": format!("Recipe #{i}"),
            "uid": format!("UID-{i:04}"),
            "ingredients": format!("{} cups flour\n{} eggs\n1 tsp salt", i % 5 + 1, i % 3 + 1),
            "directions": format!("Step 1: Preheat to {}°F.\nStep 2: Mix.\nStep 3: Cook {} min.", 300 + (i % 200), 15 + (i % 45)),
            "categories": ["Test", "Batch"],
        })
        .to_string();
        entries.push((format!("UID-{i:04}.paprikarecipe"), json));
    }

    let refs: Vec<(&str, &str)> = entries
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let start = std::time::Instant::now();
    let archive = create_archive(&refs);
    let (recipes, errors) = parse_paprikarecipes_archive(&archive);
    assert!(errors.is_empty());
    assert_eq!(recipes.len(), 500);

    let (prepared, report) = convert_paprika_batch(recipes, &[], &[]);
    let elapsed = start.elapsed();

    assert_eq!(report.imported, 500);
    assert_eq!(prepared.len(), 500);
    assert!(
        elapsed.as_secs() < 10,
        "500 recipe import took too long: {elapsed:?}"
    );
}

// ─────────────────────────────────────────────────────────────────
// Error handling
// ─────────────────────────────────────────────────────────────────

#[test]
fn corrupt_entry_does_not_block_batch() {
    let archive = {
        let buf = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(buf);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        // Valid recipe
        let gzipped1 = gzip_json(&full_recipe_json());
        zip.start_file("GOOD.paprikarecipe", options).unwrap();
        zip.write_all(&gzipped1).unwrap();

        // Corrupt: valid gzip but invalid JSON
        let corrupt = gzip_json("{ not valid JSON }}}");
        zip.start_file("BAD.paprikarecipe", options).unwrap();
        zip.write_all(&corrupt).unwrap();

        // Another valid recipe
        let gzipped2 = gzip_json(&minimal_recipe_json());
        zip.start_file("GOOD2.paprikarecipe", options).unwrap();
        zip.write_all(&gzipped2).unwrap();

        zip.finish().unwrap().into_inner()
    };

    let (recipes, errors) = parse_paprikarecipes_archive(&archive);
    assert_eq!(recipes.len(), 2);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].entry_name.contains("BAD"));
}
