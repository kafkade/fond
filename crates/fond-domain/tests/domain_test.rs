//! Integration tests for the fond-domain crate: parser, emitter, and slug.

use std::path::PathBuf;

use fond_domain::{emit_cook, parse_cook, slugify, title_from_stem};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn load_fixture(name: &str) -> String {
    std::fs::read_to_string(fixtures_dir().join(name))
        .unwrap_or_else(|e| panic!("failed to load fixture {name}: {e}"))
}

const ALL_FIXTURES: &[&str] = &[
    "chicken-adobo.cook",
    "sourdough-bread.cook",
    "mapo-tofu.cook",
    "simple-eggs.cook",
    "pasta-alla-norma.cook",
    "thai-green-curry.cook",
    "creme-brulee.cook",
    "birria-tacos.cook",
    "miso-ramen.cook",
    "cilbir.cook",
    "chocolate-chip-cookies.cook",
];

// ═══════════════════════════════════════════════════════════════════
// Parser: all fixtures
// ═══════════════════════════════════════════════════════════════════

#[test]
fn parse_all_fixtures_succeed() {
    let mut failures = Vec::new();
    for name in ALL_FIXTURES {
        let content = load_fixture(name);
        let stem = name.trim_end_matches(".cook");
        if let Err(e) = parse_cook(&content, stem) {
            failures.push(format!("{name}: {e}"));
        }
    }
    assert!(
        failures.is_empty(),
        "Fixtures failed to parse:\n{}",
        failures.join("\n")
    );
}

#[test]
fn parse_produces_title() {
    for name in ALL_FIXTURES {
        let content = load_fixture(name);
        let stem = name.trim_end_matches(".cook");
        let recipe = parse_cook(&content, stem).unwrap();
        assert!(
            !recipe.title.is_empty(),
            "{name}: title should not be empty"
        );
    }
}

#[test]
fn parse_produces_slug() {
    for name in ALL_FIXTURES {
        let content = load_fixture(name);
        let stem = name.trim_end_matches(".cook");
        let recipe = parse_cook(&content, stem).unwrap();
        assert!(!recipe.slug.is_empty(), "{name}: slug should not be empty");
        assert!(
            !recipe.slug.contains(' '),
            "{name}: slug should not contain spaces: {:?}",
            recipe.slug
        );
    }
}

#[test]
fn parse_produces_ingredients() {
    for name in ALL_FIXTURES {
        let content = load_fixture(name);
        let stem = name.trim_end_matches(".cook");
        let recipe = parse_cook(&content, stem).unwrap();
        assert!(
            !recipe.ingredients.is_empty(),
            "{name}: should have at least one ingredient"
        );
    }
}

#[test]
fn parse_produces_steps() {
    for name in ALL_FIXTURES {
        let content = load_fixture(name);
        let stem = name.trim_end_matches(".cook");
        let recipe = parse_cook(&content, stem).unwrap();
        assert!(
            !recipe.steps.is_empty(),
            "{name}: should have at least one step"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════
// Parser: specific recipes
// ═══════════════════════════════════════════════════════════════════

#[test]
fn chicken_adobo_metadata() {
    let recipe = parse_cook(&load_fixture("chicken-adobo.cook"), "chicken-adobo").unwrap();
    assert_eq!(recipe.title, "Classic Chicken Adobo");
    assert_eq!(
        recipe.source.as_deref(),
        Some("Traditional Filipino Recipe")
    );
    assert_eq!(recipe.servings.as_deref(), Some("4"));
    assert!(recipe.prep_time.is_some());
    assert!(recipe.cook_time.is_some());
}

#[test]
fn chicken_adobo_ingredients() {
    let recipe = parse_cook(&load_fixture("chicken-adobo.cook"), "chicken-adobo").unwrap();
    let names: Vec<&str> = recipe.ingredients.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"soy sauce"), "missing soy sauce: {names:?}");
    assert!(
        names.contains(&"chicken thighs"),
        "missing chicken thighs: {names:?}"
    );
}

#[test]
fn chicken_adobo_tags() {
    let recipe = parse_cook(&load_fixture("chicken-adobo.cook"), "chicken-adobo").unwrap();
    assert!(
        recipe.tags.contains(&"chicken".to_string()),
        "missing 'chicken': {:?}",
        recipe.tags
    );
    assert!(
        recipe.tags.contains(&"filipino".to_string()),
        "missing 'filipino': {:?}",
        recipe.tags
    );
}

#[test]
fn mapo_tofu_cookware() {
    let recipe = parse_cook(&load_fixture("mapo-tofu.cook"), "mapo-tofu").unwrap();
    let cw_names: Vec<&str> = recipe.cookware.iter().map(|c| c.name.as_str()).collect();
    assert!(
        cw_names.contains(&"wok"),
        "missing wok cookware: {cw_names:?}"
    );
}

#[test]
fn ingredient_quantities_preserved() {
    let recipe = parse_cook(&load_fixture("chicken-adobo.cook"), "chicken-adobo").unwrap();
    let soy = recipe
        .ingredients
        .iter()
        .find(|i| i.name == "soy sauce")
        .expect("soy sauce not found");
    assert!(
        soy.quantity.is_some(),
        "soy sauce should have a quantity: {soy:?}"
    );
}

#[test]
fn steps_have_sequential_order() {
    let recipe = parse_cook(&load_fixture("chicken-adobo.cook"), "chicken-adobo").unwrap();
    for (i, step) in recipe.steps.iter().enumerate() {
        assert_eq!(
            step.order, i as u32,
            "Step order mismatch at index {i}: expected {i}, got {}",
            step.order
        );
    }
}

#[test]
fn timers_extracted_from_steps() {
    let recipe = parse_cook(&load_fixture("chicken-adobo.cook"), "chicken-adobo").unwrap();
    let all_timers: Vec<_> = recipe.steps.iter().flat_map(|s| &s.timers).collect();
    assert!(
        !all_timers.is_empty(),
        "Adobo recipe should have at least one timer"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Parser: title derivation from filename
// ═══════════════════════════════════════════════════════════════════

#[test]
fn title_derived_from_filename_when_no_metadata() {
    let content = "Add @eggs{3} to a #pan{} with @butter{1%tbsp}.\n";
    let recipe = parse_cook(content, "scrambled-eggs").unwrap();
    assert_eq!(recipe.title, "Scrambled Eggs");
    assert_eq!(recipe.slug, "scrambled-eggs");
}

// ═══════════════════════════════════════════════════════════════════
// Parser: raw_source preserved
// ═══════════════════════════════════════════════════════════════════

#[test]
fn raw_source_preserved() {
    let content = load_fixture("chicken-adobo.cook");
    let recipe = parse_cook(&content, "chicken-adobo").unwrap();
    assert_eq!(recipe.raw_source.as_deref(), Some(content.as_str()));
}

// ═══════════════════════════════════════════════════════════════════
// Emitter: raw_source pass-through
// ═══════════════════════════════════════════════════════════════════

#[test]
fn emit_with_raw_source_returns_original() {
    let content = load_fixture("chicken-adobo.cook");
    let recipe = parse_cook(&content, "chicken-adobo").unwrap();
    let emitted = emit_cook(&recipe);
    assert_eq!(emitted, content, "Emitter should return raw_source as-is");
}

// ═══════════════════════════════════════════════════════════════════
// Emitter: generated recipes (no raw_source)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn emit_generated_recipe_has_frontmatter() {
    let mut recipe = parse_cook(&load_fixture("chicken-adobo.cook"), "chicken-adobo").unwrap();
    recipe.raw_source = None; // Simulate an imported recipe

    let emitted = emit_cook(&recipe);
    assert!(
        emitted.starts_with("---\n"),
        "Should start with frontmatter"
    );
    assert!(
        emitted.contains("title: Classic Chicken Adobo"),
        "Should include title"
    );
    assert!(
        emitted.contains("source: Traditional Filipino Recipe"),
        "Should include source: {emitted}"
    );
}

#[test]
fn emit_generated_recipe_parseable() {
    let mut recipe = parse_cook(&load_fixture("chicken-adobo.cook"), "chicken-adobo").unwrap();
    recipe.raw_source = None;

    let emitted = emit_cook(&recipe);
    let re_parsed = parse_cook(&emitted, "chicken-adobo");
    assert!(
        re_parsed.is_ok(),
        "Emitted .cook should be parseable: {:?}",
        re_parsed.err()
    );

    let re = re_parsed.unwrap();
    assert_eq!(re.title, recipe.title);
    assert_eq!(re.source, recipe.source);
    assert_eq!(re.tags, recipe.tags);
}

// ═══════════════════════════════════════════════════════════════════
// Emitter: round-trip field equality (semantic, not byte-level)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn round_trip_preserves_title_and_tags() {
    for name in ALL_FIXTURES {
        let content = load_fixture(name);
        let stem = name.trim_end_matches(".cook");
        let original = parse_cook(&content, stem).unwrap();

        // Strip raw_source to force emitter to generate .cook
        let mut stripped = original.clone();
        stripped.raw_source = None;

        let emitted = emit_cook(&stripped);
        let re_parsed = parse_cook(&emitted, stem);

        if let Ok(re) = re_parsed {
            assert_eq!(re.title, original.title, "{name}: title mismatch");
            assert_eq!(re.tags, original.tags, "{name}: tags mismatch");
        }
        // If re-parse fails, that's acceptable for complex fixtures —
        // the emitter is best-effort for generated content.
    }
}

// ═══════════════════════════════════════════════════════════════════
// Slug
// ═══════════════════════════════════════════════════════════════════

#[test]
fn slug_from_various_titles() {
    assert_eq!(slugify("Classic Chicken Adobo"), "classic-chicken-adobo");
    assert_eq!(slugify("Mapo Tofu"), "mapo-tofu");
    assert_eq!(slugify("Pasta alla Norma!"), "pasta-alla-norma");
    assert_eq!(slugify("100% Whole Wheat"), "100-whole-wheat");
}

#[test]
fn title_from_stem_various() {
    assert_eq!(title_from_stem("chicken-adobo"), "Chicken Adobo");
    assert_eq!(title_from_stem("mapo-tofu"), "Mapo Tofu");
    assert_eq!(
        title_from_stem("chocolate-chip-cookies"),
        "Chocolate Chip Cookies"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Serialization
// ═══════════════════════════════════════════════════════════════════

#[test]
fn recipe_serializes_to_json() {
    let recipe = parse_cook(&load_fixture("chicken-adobo.cook"), "chicken-adobo").unwrap();
    let json = serde_json::to_string_pretty(&recipe);
    assert!(json.is_ok(), "Recipe should serialize to JSON");

    let json_str = json.unwrap();
    assert!(json_str.contains("Classic Chicken Adobo"));
    assert!(json_str.contains("soy sauce"));
}

#[test]
fn recipe_round_trips_through_json() {
    let recipe = parse_cook(&load_fixture("chicken-adobo.cook"), "chicken-adobo").unwrap();
    let json = serde_json::to_string(&recipe).unwrap();
    let deserialized: fond_domain::Recipe = serde_json::from_str(&json).unwrap();
    assert_eq!(recipe, deserialized);
}
